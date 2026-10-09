"""Secure-headers verification (OWASP A05 — automated header check).

`app.enable_secure_headers()` must emit the hardened header set on every
response; this test runs in CI and is the automated check for the OWASP
checklist item.
"""
from justapi import JustAPIApp, JustAPITestClient


def test_secure_headers_present():
    app = JustAPIApp()
    app.enable_secure_headers()

    @app.get("/hdr")
    def hdr():
        return {"ok": True}

    resp = JustAPITestClient(app).get("/hdr")
    assert resp["status"] == 200
    headers = {k.lower(): v for k, v in resp["headers"].items()}
    assert headers.get("x-content-type-options") == "nosniff"
    assert headers.get("x-frame-options") == "DENY"
    assert "default-src 'self'" in headers.get("content-security-policy", "")
    assert headers.get("x-xss-protection") == "0"
    # HSTS is opt-in (plaintext dev server): absent by default, present when asked.
    assert "strict-transport-security" not in headers


def test_secure_headers_with_hsts():
    app = JustAPIApp()
    app.enable_secure_headers(with_hsts=True)

    @app.get("/hdr")
    def hdr():
        return {"ok": True}

    resp = JustAPITestClient(app).get("/hdr")
    headers = {k.lower(): v for k, v in resp["headers"].items()}
    assert "max-age=" in headers.get("strict-transport-security", "")
