import os
import tempfile

from justapi import JustAPIApp, JustAPITestClient
from justapi.responses import (
    Response,
    RedirectResponse,
    JSONResponse,
    FileResponse,
    HTMLResponse,
)


def test_set_cookie_and_delete_cookie():
    r = Response("hi", media_type="text/plain")
    r.set_cookie("session", "abc123", max_age=3600, httponly=True, samesite="lax")
    cookies = [v for k, v in r.headers if k == b"set-cookie"]
    assert cookies == [b"session=abc123; Max-Age=3600; path=/; httponly; samesite=lax"]

    r.delete_cookie("session")
    expired = [v for k, v in r.headers if k == b"set-cookie" and b"expires=" in v]
    assert len(expired) == 1
    assert b"session=" in expired[0] and b"Max-Age=0" in expired[0]


def test_redirect_response_sets_location():
    r = RedirectResponse("/login", status_code=302)
    assert r.status_code == 302
    loc = [v for k, v in r.headers if k == b"location"]
    assert loc == [b"/login"]


def test_file_response_serves_content():
    with tempfile.NamedTemporaryFile(suffix=".txt", delete=False) as tf:
        tf.write(b"hello from disk")
        path = tf.name
    try:
        r = FileResponse(path)
        assert r.status_code == 200
        assert r.body == b"hello from disk"
        ctype = [v for k, v in r.headers if k == b"content-type"]
        assert ctype == [b"text/plain"]
        cdisp = [v for k, v in r.headers if k == b"content-disposition"]
        assert cdisp and b'filename="' in cdisp[0]
    finally:
        os.unlink(path)


def test_response_end_to_end_via_testclient():
    app = JustAPIApp()

    @app.get("/set")
    def set_cookie(req):
        r = JSONResponse({"ok": True})
        r.set_cookie("token", "xyz", httponly=True)
        return r

    @app.get("/go")
    def go(req):
        return RedirectResponse("/set", status_code=307)

    client = JustAPITestClient(app)
    resp = client.get("/set")
    assert resp["status"] == 200
    assert "token=xyz" in resp["headers"].get("set-cookie", "")

    resp2 = client.get("/go")
    assert resp2["status"] == 307
    assert resp2["headers"].get("location") == "/set"


def test_status_key_in_body_not_dropped():
    # BUG-1 (PRODUCTION_PLAN.md P0.3): a response dict that contains a
    # top-level "status" field must NOT be misclassified as a legacy
    # {"status", "body"} envelope. Its body must be returned intact.
    app = JustAPIApp()

    @app.get("/with_status")
    def with_status(req):
        return {"status": "ok", "products": 5}

    client = JustAPITestClient(app)
    resp = client.get("/with_status")
    assert resp["status"] == 200
    # orjson produces compact JSON (no spaces); the key point is the "status"
    # field is preserved in the body, not dropped as a legacy envelope.
    assert resp["body"] == b'{"status":"ok","products":5}'


def test_explicit_envelope_still_works():
    # A real envelope (with "body") is still passed through unchanged.
    app = JustAPIApp()

    @app.get("/env")
    def env(req):
        return {"__response__": True, "status": 201, "body": "created"}

    client = JustAPITestClient(app)
    resp = client.get("/env")
    assert resp["status"] == 201
    assert resp["body"] == b"created"

