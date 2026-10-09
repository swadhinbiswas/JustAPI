"""CORS middleware bridge — real end-to-end assertions.

Covers:
- ``add_cors()`` bridges to the Rust ``Cors`` middleware (not a Python shim)
- OPTIONS preflight (204 + Access-Control-* headers)
- explicit-origin allow/deny
- CORS + JWT together: preflight must NOT be rejected by auth, and auth
  errors (401) must carry CORS headers so browsers can read them
"""
import json
import time

from justapi import JustAPIApp, JustAPITestClient, JwtAuth


def _hdr(resp):
    return {k.lower(): v for k, v in resp["headers"].items()}


def test_cors_wildcard_get():
    app = JustAPIApp()
    app.add_cors()

    @app.get("/x")
    def x():
        return {"ok": True}

    resp = JustAPITestClient(app).get_with("/x", [("Origin", "https://any.example")])
    assert resp["status"] == 200
    assert _hdr(resp).get("access-control-allow-origin") == "*"


def test_cors_explicit_origin_allowed_and_denied():
    app = JustAPIApp()
    app.add_cors(allow_origins=["https://app.example.com"])

    @app.get("/x")
    def x():
        return {"ok": True}

    c = JustAPITestClient(app)
    allowed = c.get_with("/x", [("Origin", "https://app.example.com")])
    assert _hdr(allowed).get("access-control-allow-origin") == "https://app.example.com"

    denied = c.get_with("/x", [("Origin", "https://evil.example")])
    # Disallowed origin: request still runs, but no ACAO header is emitted.
    assert "access-control-allow-origin" not in _hdr(denied)


def test_cors_preflight_options():
    app = JustAPIApp()
    app.add_cors(allow_origins=["https://app.example.com"], allow_methods=["GET", "POST"])

    @app.get("/x")
    def x():
        return {"ok": True}

    c = JustAPITestClient(app)
    resp = c.options_with(
        "/x",
        [
            ("Origin", "https://app.example.com"),
            ("Access-Control-Request-Method", "POST"),
        ],
    )
    assert resp["status"] == 204
    hdr = _hdr(resp)
    assert hdr.get("access-control-allow-origin") == "https://app.example.com"
    assert "POST" in hdr.get("access-control-allow-methods", "")


def test_cors_with_jwt_preflight_not_blocked_and_401_has_cors():
    """Regression: CORS must wrap JWT.

    Before Phase 53 the server chain ran JWT outside CORS, so an OPTIONS
    preflight was rejected with 401 and 401 responses carried no CORS headers
    (browsers then show an opaque network error instead of the 401).
    """
    secret = "test-secret"
    app = JustAPIApp()
    app.add_cors(allow_origins=["https://app.example.com"], allow_methods=["GET", "POST"])
    app.set_jwt_auth(secret, "HS256")

    @app.get("/me")
    def me(request):
        return {"auth": request.auth}

    c = JustAPITestClient(app)

    # 1) Real preflight (OPTIONS) is answered by CORS with 204 — JWT must not
    #    reject it (browsers send no credentials on preflight).
    pre = c.options_with(
        "/me",
        [
            ("Origin", "https://app.example.com"),
            ("Access-Control-Request-Method", "GET"),
        ],
    )
    assert pre["status"] == 204, pre
    assert _hdr(pre).get("access-control-allow-origin") == "https://app.example.com"

    # 2) No token -> 401, and the response still carries CORS headers.
    resp = c.get_with("/me", [("Origin", "https://app.example.com")])
    assert resp["status"] == 401
    assert _hdr(resp).get("access-control-allow-origin") == "https://app.example.com"

    # 2) Valid token -> 200, claims visible via request.auth.
    token = JwtAuth(secret=secret, algorithm="HS256").encode(
        {"sub": "u1", "exp": int(time.time()) + 60}
    )
    resp = c.get_with(
        "/me",
        [("Origin", "https://app.example.com"), ("Authorization", f"Bearer {token}")],
    )
    assert resp["status"] == 200, resp
    body = json.loads(bytes(resp["body"]))
    assert body["auth"]["sub"] == "u1"

    # 3) Bad token -> 401 + CORS headers.
    resp = c.get_with(
        "/me",
        [("Origin", "https://app.example.com"), ("Authorization", "Bearer not.a.token")],
    )
    assert resp["status"] == 401
    assert _hdr(resp).get("access-control-allow-origin") == "https://app.example.com"
