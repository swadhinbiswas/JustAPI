"""Tests for the introspection (``/_system``) endpoints and the MCP server tools."""

import json

import pytest

from justapi import JustAPIApp, Request, Query, JustAPITestClient
from justapi.responses import JSONResponse
from justapi import build_help, build_openapi
from justapi.mcp_server import _dispatch, TOOLS


def _make_app():
    app = JustAPIApp()

    @app.get("/items/{item_id}", name="item-detail", tags=["items"], summary="Get an item")
    def get_item(item_id: int, q: Query = Query(None), request: Request = None):
        """Return a single item by id."""
        return {"item_id": item_id, "q": q}

    @app.post("/items", tags=["items"])
    def create_item(body: dict):
        return JSONResponse({"created": body}, status_code=201)

    app.enable_system_routes()
    return app


def _jget(client, path):
    r = client.get(path)
    return r["status"], json.loads(bytes(r["body"]))


def test_help_lists_all_routes():
    app = _make_app()
    status, data = _jget(JustAPITestClient(app), "/_system/help")
    assert status == 200
    assert data["route_count"] == 2
    paths = {r["path"] for r in data["routes"]}
    assert paths == {"/items/{item_id}", "/items"}


def test_param_classification():
    app = _make_app()
    status, data = _jget(JustAPITestClient(app), "/_system/help")
    get_route = next(r for r in data["routes"] if r["path"] == "/items/{item_id}")
    by_name = {p["name"]: p for p in get_route["parameters"]}
    assert by_name["item_id"]["in"] == "path"
    assert by_name["item_id"]["required"] is True
    assert by_name["q"]["in"] == "query"
    assert by_name["q"]["required"] is False
    # the raw `request` param is not surfaced
    assert "request" not in by_name


def test_body_param_classification():
    app = _make_app()
    status, data = _jget(JustAPITestClient(app), "/_system/help")
    post_route = next(r for r in data["routes"] if r["path"] == "/items")
    assert post_route["parameters"][0]["in"] == "body"


def test_openapi_synth():
    app = _make_app()
    status, oa = _jget(JustAPITestClient(app), "/_system/openapi")
    assert status == 200
    assert oa["openapi"].startswith("3.")
    item_path = oa["paths"]["/items/{item_id}"]["get"]
    assert item_path["parameters"][0]["in"] == "path"
    assert item_path["parameters"][0]["name"] == "item_id"


def test_named_route_lookup_and_404():
    app = _make_app()
    c = JustAPITestClient(app)
    assert _jget(c, "/_system/help/item-detail")[0] == 200
    assert _jget(c, "/_system/help/nope")[0] == 404
    # path-based lookup via query param
    assert _jget(c, "/_system/help?path=/items/{item_id}")[0] == 200


def test_mcp_tools_list():
    assert {t["name"] for t in TOOLS} == {
        "list_routes",
        "get_signature",
        "explain_endpoint",
        "generate_snippet",
    }


def test_mcp_dispatch():
    app = _make_app()
    # Start the app over HTTP so the MCP dispatcher can reach /_system.
    # NOTE: JustAPITestClient takes ownership of the app's routes, so we must
    # not construct one before starting the real server.
    import socket, threading, time

    def free_port():
        s = socket.socket()
        s.bind(("127.0.0.1", 0))
        p = s.getsockname()[1]
        s.close()
        return p

    port = free_port()
    threading.Thread(target=app.run, args=(f"127.0.0.1:{port}",), daemon=True).start()
    time.sleep(0.6)
    url = f"http://127.0.0.1:{port}"

    listing = _dispatch("list_routes", {"base_url": url})
    assert "2 routes" in listing

    sig = _dispatch("get_signature", {"name_or_path": "item-detail", "base_url": url})
    assert "def get_item" in sig

    snippet = _dispatch("generate_snippet", {"name_or_path": "/items/{item_id}", "base_url": url})
    assert "requests.get" in snippet

    explain = _dispatch("explain_endpoint", {"name_or_path": "item-detail", "base_url": url})
    assert "Get an item" in explain


# --- Native MCP tool registry (@app.tool) ---


def _make_app_with_tools():
    app = JustAPIApp()

    @app.tool(description="Add two integers")
    def add(a: int, b: int) -> int:
        return a + b

    @app.tool(name="greet", description="Greet someone by name")
    def greet(name: str) -> str:
        return f"hello, {name}"

    app.enable_system_routes()
    return app


def test_register_tool_and_list():
    app = _make_app_with_tools()
    tools = app.list_tools()
    names = {t["name"] for t in tools}
    assert names == {"add", "greet"}
    add_tool = next(t for t in tools if t["name"] == "add")
    assert add_tool["description"] == "Add two integers"
    # Schema inferred from type hints: a, b both required integers.
    props = add_tool["inputSchema"]["properties"]
    assert props["a"]["type"] == "integer"
    assert props["b"]["type"] == "integer"
    assert set(add_tool["inputSchema"]["required"]) == {"a", "b"}


def test_call_tool_sync():
    app = _make_app_with_tools()
    assert app.call_tool("add", {"a": 2, "b": 3}) == 5
    assert app.call_tool("greet", {"name": "Ada"}) == "hello, Ada"


def test_call_tool_unknown_raises():
    app = _make_app_with_tools()
    import pytest

    with pytest.raises(KeyError):
        app.call_tool("nope", {})


def test_system_tools_endpoint():
    app = _make_app_with_tools()
    status, data = _jget(JustAPITestClient(app), "/_system/tools")
    assert status == 200
    assert data["count"] == 2
    assert {t["name"] for t in data["tools"]} == {"add", "greet"}


def test_system_tools_call_endpoint():
    app = _make_app_with_tools()
    c = JustAPITestClient(app)
    r = c.post("/_system/tools/call", json.dumps({"name": "add", "arguments": {"a": 4, "b": 6}}).encode())
    status = r["status"]
    body = json.loads(bytes(r["body"]))
    assert status == 200
    assert body["isError"] is False
    assert json.loads(body["content"][0]["text"]) == 10


def test_mcp_native_tool_dispatch():
    """The bundled MCP server should expose + invoke the app's @app.tool."""
    app = _make_app_with_tools()
    import socket, threading, time

    def free_port():
        s = socket.socket()
        s.bind(("127.0.0.1", 0))
        p = s.getsockname()[1]
        s.close()
        return p

    port = free_port()
    threading.Thread(target=app.run, args=(f"127.0.0.1:{port}",), daemon=True).start()
    time.sleep(0.6)
    url = f"http://127.0.0.1:{port}"

    # tools/list now includes the native tool
    from justapi.mcp_server import list_app_tools

    native = {t["name"] for t in list_app_tools(url)}
    assert "add" in native

    # tools/call dispatches through the MCP server
    from justapi.mcp_server import _dispatch_app_tool

    out = _dispatch_app_tool(url, "add", {"a": 1, "b": 2})
    assert json.loads(out) == 3
