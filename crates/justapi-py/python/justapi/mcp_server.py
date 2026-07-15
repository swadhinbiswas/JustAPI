"""Minimal, dependency-free MCP server for JustAPI applications.

Speaks the Model Context Protocol over stdio using newline-delimited
JSON-RPC 2.0. It exposes two layers of tools that talk to the running app's
``/_system`` endpoints (see :mod:`justapi.system`):

Introspection utilities (so an agent can discover an API):

* ``list_routes``      -- summarize every registered route
* ``get_signature``    -- full signature / parameters for one route
* ``explain_endpoint`` -- a natural-language explanation + example
* ``generate_snippet`` -- a ready-to-run client snippet

And, crucially, the app's **native MCP tools** registered with
``@app.tool(...)``. These are fetched from ``GET /_system/tools`` and invoked
via ``POST /_system/tools/call`` -- so the MCP server is a real agent surface,
not a wrapper around HTTP routes.

Point it at a running app with system routes enabled::

    app.enable_system_routes()   # also registers /_system/tools + /_system/tools/call
    app.run("127.0.0.1:8000")

    # then, in another process / from an MCP client:
    python -m justapi.mcp_server --base-url http://127.0.0.1:8000

No third-party dependencies are required (stdlib ``urllib`` + ``json`` only),
so the server runs anywhere the ``justapi`` package is importable.
"""

import argparse
import json
import sys
import urllib.request
import urllib.error


DEFAULT_BASE_URL = "http://127.0.0.1:8000"
PROTOCOL_VERSION = "2024-11-05"


TOOLS = [
    {
        "name": "list_routes",
        "description": (
            "List every route registered on a JustAPI application, with method, "
            "path, name, tags and a one-line summary. Use this first to discover "
            "what an API exposes."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "base_url": {
                    "type": "string",
                    "description": "Base URL of the running JustAPI app (with /_system enabled).",
                },
                "tag": {
                    "type": "string",
                    "description": "Optional tag to filter routes by.",
                },
            },
            "required": [],
        },
    },
    {
        "name": "get_signature",
        "description": (
            "Return the full Python signature, parameters (name, location, type, "
            "required), return type and docstring for a single route, looked up by "
            "its name or path."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "name_or_path": {
                    "type": "string",
                    "description": "Route name (e.g. 'item-detail') or path (e.g. '/items/{item_id}').",
                },
                "base_url": {"type": "string"},
            },
            "required": ["name_or_path"],
        },
    },
    {
        "name": "explain_endpoint",
        "description": (
            "Return a plain-language explanation of what a route does, its "
            "parameters and an example request, ideal for generating code or docs."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "name_or_path": {"type": "string"},
                "base_url": {"type": "string"},
            },
            "required": ["name_or_path"],
        },
    },
    {
        "name": "generate_snippet",
        "description": (
            "Generate a ready-to-run client code snippet that calls a route. "
            "Currently emits Python using the requests library."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "name_or_path": {"type": "string"},
                "language": {
                    "type": "string",
                    "description": "Target language for the snippet (default: python).",
                },
                "base_url": {"type": "string"},
            },
            "required": ["name_or_path"],
        },
    },
]


def _http_json(url):
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _post_json(url, payload):
    body = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=body,
        headers={"Content-Type": "application/json", "Accept": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _fetch_route(base_url, name_or_path):
    import urllib.parse

    base = (base_url or DEFAULT_BASE_URL).rstrip("/")
    key = "path" if str(name_or_path).startswith("/") else "name"
    qs = urllib.parse.urlencode({key: name_or_path})
    data = _http_json(f"{base}/_system/help?{qs}")
    return data


def list_app_tools(base_url):
    """Fetch the app's native ``@app.tool`` registrations in MCP shape."""
    base = (base_url or DEFAULT_BASE_URL).rstrip("/")
    try:
        data = _http_json(f"{base}/_system/tools")
    except urllib.error.URLError:
        return []
    return data.get("tools", [])


def _dispatch(tool_name, args):
    base_url = args.get("base_url", DEFAULT_BASE_URL)
    base = (base_url or DEFAULT_BASE_URL).rstrip("/")
    if tool_name == "list_routes":
        data = _http_json(f"{base}/_system/help")
        tag = args.get("tag")
        routes = data.get("routes", [])
        if tag:
            routes = [r for r in routes if tag in (r.get("tags") or [])]
        lines = []
        for r in routes:
            summary = (r.get("summary") or r.get("description") or "").strip().splitlines()
            summary = summary[0] if summary else ""
            lines.append(
                f"{r['method']:6} {r['path']:35} name={r.get('name')}  {summary}"
            )
        text = f"App: {data.get('app', {}).get('title')} | {len(routes)} routes\n\n" + "\n".join(lines)
        return text
    if tool_name == "get_signature":
        r = _fetch_route(base_url, args["name_or_path"])
        params = "\n".join(
            f"  - {p['name']} ({p['in']}, {'required' if p.get('required') else 'optional'}, "
            f"type={p['annotation']})" + (f" alias={p['alias']}" if p.get("alias") else "")
            for p in r.get("parameters", [])
        )
        text = (
            f"{r['method']} {r['path']}  [name={r.get('name')}]\n"
            f"signature: {r.get('signature')}\n"
            f"returns: {r.get('returns')}\n"
            f"parameters:\n{params}\n"
            f"docstring:\n{r.get('docstring') or '(none)'}\n"
        )
        return text
    if tool_name == "explain_endpoint":
        r = _fetch_route(base_url, args["name_or_path"])
        return r.get("explanation") or "(no description available)"
    if tool_name == "generate_snippet":
        r = _fetch_route(base_url, args["name_or_path"])
        lang = (args.get("language") or "python").lower()
        if lang != "python":
            return f"Snippet generation for '{lang}' is not supported yet. Python example:\n\n{r.get('example', '')}"
        return r.get("example") or "(no example available)"
    raise ValueError(f"unknown tool: {tool_name}")


def _dispatch_app_tool(base_url, name, arguments):
    """Invoke a native ``@app.tool`` and return MCP content text."""
    base = (base_url or DEFAULT_BASE_URL).rstrip("/")
    data = _post_json(f"{base}/_system/tools/call", {"name": name, "arguments": arguments or {}})
    if data.get("isError"):
        raise RuntimeError(data.get("error", "tool call failed"))
    # Flatten the content blocks into a single text payload.
    parts = []
    for block in data.get("content", []):
        parts.append(block.get("text", ""))
    return "\n".join(parts)


def _handle(request):
    method = request.get("method")
    req_id = request.get("id")
    if method == "initialize":
        return {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "justapi-mcp", "version": "1.0.0"},
            },
        }
    if method == "ping":
        return {"jsonrpc": "2.0", "id": req_id, "result": {}}
    if method == "tools/list":
        base = DEFAULT_BASE_URL
        # Merge the introspection utilities with the app's native tools.
        tools = list(TOOLS)
        for t in list_app_tools(base):
            tools.append(
                {
                    "name": t["name"],
                    "description": t.get("description", ""),
                    "inputSchema": t.get("inputSchema", {"type": "object", "properties": {}}),
                }
            )
        return {"jsonrpc": "2.0", "id": req_id, "result": {"tools": tools}}
    if method == "tools/call":
        params = request.get("params", {}) or {}
        name = params.get("name")
        args = params.get("arguments", {}) or {}
        try:
            # Native app tools are dispatched directly; everything else is an
            # introspection utility handled locally.
            if any(t["name"] == name for t in list_app_tools(DEFAULT_BASE_URL)):
                text = _dispatch_app_tool(DEFAULT_BASE_URL, name, args)
            else:
                text = _dispatch(name, args)
            is_error = False
        except Exception as e:  # noqa: BLE001 - report any failure back to the client
            text = f"Error: {e}"
            is_error = True
        return {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "content": [{"type": "text", "text": text}],
                "isError": is_error,
            },
        }
    return {
        "jsonrpc": "2.0",
        "id": req_id,
        "error": {"code": -32601, "message": f"method not found: {method}"},
    }


def _main_loop():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            continue
        method = request.get("method", "")
        if method.startswith("notifications/"):
            continue
        response = _handle(request)
        if response is not None:
            sys.stdout.write(json.dumps(response) + "\n")
            sys.stdout.flush()


def main():
    global DEFAULT_BASE_URL
    parser = argparse.ArgumentParser(description="JustAPI MCP server (stdio)")
    parser.add_argument(
        "--base-url",
        default=DEFAULT_BASE_URL,
        help="Default base URL of the running JustAPI app with /_system enabled.",
    )
    args = parser.parse_args()
    DEFAULT_BASE_URL = args.base_url
    _main_loop()


if __name__ == "__main__":
    main()
