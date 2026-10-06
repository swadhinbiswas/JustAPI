"""Header convert_underscores + bool/int/float coercion parity (Phase 53)."""
import json
from justapi import JustAPIApp, JustAPITestClient, Header


def _body(resp):
    raw = resp["body"]
    return json.loads(bytes(raw).decode("utf-8"))


def test_header_underscore_to_dash():
    app = JustAPIApp()

    @app.post("/h")
    def h(user_agent: str = Header("missing")):
        return {"ua": user_agent}

    c = JustAPITestClient(app)
    resp = c.post_with("/h", b"", [("user-agent", "pytest")])
    assert resp["status"] == 200, resp
    assert _body(resp) == {"ua": "pytest"}


def test_header_no_convert_opt_out():
    app = JustAPIApp()

    @app.post("/h")
    def h(x_custom: str = Header("missing", convert_underscores=False)):
        return {"v": x_custom}

    c = JustAPITestClient(app)
    # dash form must NOT match when conversion is off
    resp = c.post_with("/h", b"", [("x-custom", "dash")])
    assert resp["status"] == 200, resp
    assert _body(resp) == {"v": "missing"}


def test_bool_int_float_coercion():
    app = JustAPIApp()

    @app.get("/flags")
    def flags(verbose: bool, count: int, ratio: float):
        return {"verbose": verbose, "count": count, "ratio": ratio}

    c = JustAPITestClient(app)
    resp = c.get("/flags?verbose=true&count=42&ratio=2.5")
    assert resp["status"] == 200, resp
    assert _body(resp) == {"verbose": True, "count": 42, "ratio": 2.5}

    resp = c.get("/flags?verbose=0&count=7&ratio=1")
    assert resp["status"] == 200, resp
    assert _body(resp) == {"verbose": False, "count": 7, "ratio": 1.0}


def test_bool_invalid_is_422():
    app = JustAPIApp()

    @app.get("/f")
    def f(verbose: bool):
        return {"verbose": verbose}

    c = JustAPITestClient(app)
    resp = c.get("/f?verbose=maybe")
    assert resp["status"] == 422, resp
