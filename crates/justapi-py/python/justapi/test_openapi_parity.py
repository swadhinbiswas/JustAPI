"""Integration test for FastAPI APIRouter parity (OpenAPI metadata, head/options/trace, frontend, include_router)."""
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.request
import urllib.error

SERVER_SCRIPT = r"""
import tempfile, os
from justapi import JustAPIApp, APIRouter

app = JustAPIApp()

static_dir = tempfile.mkdtemp()
with open(os.path.join(static_dir, "index.html"), "w") as f:
    f.write("<html><body>SPA</body></html>")

async def list_items(request):
    return [{"id": 1}]

async def create_item(request):
    return {"id": 2}

async def hidden(request):
    return {"ok": True}

async def head_items(request):
    return {"ok": True}

async def options_items(request):
    return {"ok": True}

async def trace_items(request):
    return {"ok": True}

app.get(
    "/items",
    list_items,
    tags=["items"],
    summary="List items",
    description="Return all items.",
    deprecated=True,
    responses={404: {"description": "Not found"}},
    operation_id="list_items",
    openapi_extra={"x-custom": "hello"},
)
app.post("/items", create_item, tags=["items"], status_code=201)
app.get("/hidden", hidden, include_in_schema=False)
# Explicit head/options/trace are registered on paths without a GET (GET
# already auto-registers HEAD, mirroring FastAPI/Starlette behavior).
app.head("/head1", head_items)
app.options("/opts1", options_items)
app.trace("/trace1", trace_items)

admin = APIRouter(prefix="/admin", tags=["admin"])
@admin.get("/ping", tags=["health"])
async def ping(request):
    return {"pong": True}
app.include_router(admin)

app.frontend("/static", static_dir, html=True)

app.run("127.0.0.1:9871")
"""

ADDR = "127.0.0.1:9871"


def _get(path, method="GET"):
    req = urllib.request.Request(f"http://{ADDR}{path}", method=method)
    return urllib.request.urlopen(req)


def test_openapi_parity():
    proc = subprocess.Popen(
        [sys.executable, "-c", SERVER_SCRIPT],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    time.sleep(1.0)
    try:
        # OpenAPI metadata
        resp = _get("/openapi.json")
        spec = json.loads(resp.read())
        paths = spec["paths"]

        # GET /items metadata
        get_items = paths["/items"]["get"]
        assert get_items["tags"] == ["items"], get_items
        assert get_items["summary"] == "List items"
        assert get_items["description"] == "Return all items."
        assert get_items.get("deprecated") is True
        assert get_items["operationId"] == "list_items"
        assert get_items["x-custom"] == "hello", get_items
        assert "404" in get_items["responses"]
        # default success code 200
        assert "200" in get_items["responses"]

        # POST /items custom status code
        post_items = paths["/items"]["post"]
        assert "201" in post_items["responses"], post_items

        # HEAD is excluded from OpenAPI (auto-registered from GET)
        assert "head" not in paths["/items"]
        # Explicit OPTIONS / TRACE are present on their own paths
        assert "head" in paths["/head1"]
        assert "options" in paths["/opts1"]
        assert "trace" in paths["/trace1"]

        # include_in_schema=False route is excluded
        assert "/hidden" not in paths

        # include_router merged tags: /admin/ping has router tags + route tags
        ping = paths["/admin/ping"]["get"]
        assert "admin" in ping["tags"], ping["tags"]
        assert "health" in ping["tags"], ping["tags"]

        # Frontend SPA serving + fallback
        resp = _get("/static/")
        assert b"SPA" in resp.read(), "frontend index not served"
        resp = _get("/static/does-not-exist")
        assert b"SPA" in resp.read(), "frontend fallback (index.html) not served"

        # head/options respond at runtime
        assert _get("/head1", method="HEAD").status == 200
        assert _get("/opts1", method="OPTIONS").status == 200

        print("PASS: openapi parity + frontend + methods")
    finally:
        proc.terminate()
        try:
            proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.communicate()


if __name__ == "__main__":
    test_openapi_parity()
