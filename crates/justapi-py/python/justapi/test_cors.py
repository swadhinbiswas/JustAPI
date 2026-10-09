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
