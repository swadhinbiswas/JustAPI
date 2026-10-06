"""Header convert_underscores + bool/int/float coercion parity (Phase 53)."""
import json
import pytest
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


def test_query_constraints_422_and_ok():
    from justapi import Query

    app = JustAPIApp()

    @app.get("/items")
    def items(limit: int = Query(10, ge=1, le=100), q: str = Query("", min_length=0, max_length=10)):
        return {"limit": limit, "q": q}

    c = JustAPITestClient(app)
    assert c.get("/items?limit=5")["status"] == 200
    assert c.get("/items?limit=0")["status"] == 422
    assert c.get("/items?limit=101")["status"] == 422
    assert c.get("/items?q=" + "x" * 11)["status"] == 422


def test_pattern_constraint():
    from justapi import Query

    app = JustAPIApp()

    @app.get("/code")
    def code(v: str = Query("", pattern=r"^[A-Z]{3}$")):
        return {"v": v}

    c = JustAPITestClient(app)
    assert c.get("/code?v=ABC")["status"] == 200
    assert c.get("/code?v=abc")["status"] == 422


def test_get_with_headers():
    app = JustAPIApp()

    @app.get("/who")
    def who(user_agent: str = Header("missing")):
        return {"ua": user_agent}

    c = JustAPITestClient(app)
    resp = c.get_with("/who", [("user-agent", "pytest-get")])
    assert resp["status"] == 200, resp
    assert _body(resp) == {"ua": "pytest-get"}


def test_enum_coercion():
    import enum

    class Color(str, enum.Enum):
        RED = "red"
        BLUE = "blue"

    app = JustAPIApp()

    @app.get("/paint")
    def paint(color: Color):
        return {"color": color.value}

    c = JustAPITestClient(app)
    assert c.get("/paint?color=red")["status"] == 200
    assert c.get("/paint?color=green")["status"] == 422


def test_enum_in_openapi():
    import enum
    from justapi.system import build_openapi

    class Color(str, enum.Enum):
        RED = "red"
        BLUE = "blue"

    app = JustAPIApp()

    @app.get("/paint")
    def paint(color: Color):
        return {"color": color.value}

    doc = build_openapi(app)
    schema = doc["paths"]["/paint"]["get"]["parameters"][0]["schema"]
    assert schema["enum"] == ["red", "blue"]


def test_list_query_params():
    from typing import List
    from justapi import Query

    app = JustAPIApp()

    @app.get("/tags")
    def tags(tag: List[str]):
        return {"tags": tag}

    @app.get("/nums")
    def nums(n: List[int] = Query(..., min_length=2)):
        return {"n": n}

    c = JustAPITestClient(app)
    resp = c.get("/tags?tag=a&tag=b&tag=c")
    assert resp["status"] == 200, resp
    assert _body(resp) == {"tags": ["a", "b", "c"]}
    resp = c.get("/nums?n=1&n=2&n=3")
    assert resp["status"] == 200, resp
    assert _body(resp) == {"n": [1, 2, 3]}
    assert c.get("/nums?n=1")["status"] == 422  # min_length=2


@pytest.mark.asyncio
async def test_async_client_headers():
    from justapi.testing import AsyncTestClient

    app = JustAPIApp()

    @app.post("/h")
    def h(user_agent: str = Header("missing")):
        return {"ua": user_agent}

    @app.put("/h")
    def h_put(user_agent: str = Header("missing")):
        return {"ua": user_agent}

    @app.delete("/h")
    def h_del(user_agent: str = Header("missing")):
        return {"ua": user_agent}

    async with AsyncTestClient(app) as c:
        resp = await c.post("/h", b"", headers=[("user-agent", "async-post")])
        assert resp["status"] == 200
        assert _body(resp) == {"ua": "async-post"}
        resp = await c.get("/h", headers=[("user-agent", "async-get")])
        assert resp["status"] == 405  # GET not registered on POST-only route
        resp = await c.put("/h", b"", headers=[("user-agent", "async-put")])
        assert resp["status"] == 200
        assert _body(resp) == {"ua": "async-put"}
        resp = await c.delete("/h", headers=[("user-agent", "async-del")])
        assert resp["status"] == 200
        assert _body(resp) == {"ua": "async-del"}
