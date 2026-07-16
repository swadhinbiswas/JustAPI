"""Integration test for FastAPI/Starlette-style Request surface parity.

Exercises the `Request` (and `HTTPConnection`) attribute/method surface exposed
on handlers: scope, app, url, base_url, headers, query_params, path_params,
cookies, client, method, state, body, json, form, and the mapping protocol.
"""

import json

from justapi import JustAPIApp, JustAPITestClient, APIRouter


def test_request_attributes_and_mapping():
    app = JustAPIApp()

    captured = {}

    async def echo(request):
        captured["request"] = request
        # mapping protocol + attribute access
        assert request["method"] == "POST"
        assert "method" in request
        assert request.get("method") == "POST"
        assert request.get("missing", "dflt") == "dflt"
        request["note"] = "stored"
        assert request["note"] == "stored"
        return {"ok": True}

    app.post("/items/{item_id}", echo)
    client = JustAPITestClient(app)
    resp = client.post_with(
        "/items/42?q=hello&skip=10",
        b'{"a": 1}',
        [
            ("Content-Type", "application/json"),
            ("X-Token", "abc"),
            ("Cookie", "session=xyz; theme=dark"),
        ],
    )
    assert resp["status"] == 200

    r = captured["request"]
    # Core attributes
    assert r.method == "POST"
    assert r.scope["method"] == "POST"
    assert r.scope["type"] == "http"
    assert r.path_params["item_id"] == "42"
    assert r.query_params["q"] == "hello"
    assert r.query_params.get("skip") == "10"
    assert r.query_params.get("nope", "d") == "d"
    assert r.headers["x-token"] == "abc"
    assert r.headers.get("x-token") == "abc"
    assert r.cookies["session"] == "xyz"
    assert r.cookies["theme"] == "dark"
    # client is a (host, port) tuple when the transport supplies one
    assert r.client is None or (isinstance(r.client, tuple) and len(r.client) == 2)
    # app is surfaced
    assert r.app is not None


def test_request_body_json_form():
    app = JustAPIApp()

    async def inspect(request):
        body = request.body()
        parsed = request.json()
        return {"len": len(body), "parsed": parsed}

    app.post("/data", inspect)
    client = JustAPITestClient(app)
    resp = client.post_with(
        "/data", b'{"x": 7}', [("Content-Type", "application/json")]
    )
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data["len"] == 8
    assert data["parsed"] == {"x": 7}

    async def form_inspect(request):
        form = request.form()
        return {"a": form.get("a"), "b": form.get("b")}

    app.post("/form", form_inspect)
    client2 = JustAPITestClient(app)
    resp2 = client2.post_with(
        "/form",
        b"a=1&b=two",
        [("Content-Type", "application/x-www-form-urlencoded")],
    )
    assert resp2["status"] == 200
    data2 = json.loads(bytes(resp2["body"]))
    assert data2["a"] == "1"
    assert data2["b"] == "two"


def test_request_state_isolation_and_url_for():
    app = JustAPIApp()

    async def handler(request):
        request.state.user = "alice"
        assert request.state.user == "alice"
        url = request.url
        base = request.base_url
        # url_for resolves a registered named route
        assert request.url_for("item-detail", item_id=42) == "/items/42"
        return {"host": url.host, "base": str(base)}

    app.get("/items/{item_id}", handler, name="item-detail")
    client = JustAPITestClient(app)
    resp = client.get("/items/7")
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data["host"]
    assert data["base"]


def test_app_url_for_and_include_router():
    app = JustAPIApp()
    admin = APIRouter(prefix="/admin")

    @admin.get("/ping", name="admin-ping")
    async def ping(request):
        return {"pong": True}

    app.include_router(admin)
    app.get("/home/{foo}", lambda r: {}, name="item-home")
    # Named route registered directly and via include_router
    assert app.url_for("item-home", foo="x") == "/home/x"
    assert app.url_for("admin-ping") == "/admin/ping"

    client = JustAPITestClient(app)
    resp = client.get("/admin/ping")
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data["pong"] is True


def test_sync_handler_body_json_no_event_loop():
    """`request.body()` / `request.json()` must work inside a *sync* handler,
    where no asyncio event loop is running (e.g. under the test client). They are
    pure synchronous parses and must not require `await`."""
    app = JustAPIApp()

    def handler(request):
        return {"len": len(request.body()), "parsed": request.json()}

    app.post("/data", handler)
    client = JustAPITestClient(app)
    resp = client.post_with(
        "/data", b'{"x": 7}', [("Content-Type", "application/json")]
    )
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data["len"] == 8
    assert data["parsed"] == {"x": 7}


if __name__ == "__main__":
    test_request_attributes_and_mapping()
    test_request_body_json_form()
    test_request_state_isolation_and_url_for()
    test_sync_handler_body_json_no_event_loop()
    print("PASS: request parity")
