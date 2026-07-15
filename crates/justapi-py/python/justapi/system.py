"""Introspection + AI-assist surface for JustAPI applications.

Provides:

* :func:`build_help` / :func:`build_openapi` -- rich, machine-readable
  descriptions of every route (signatures, parameters, schemas, docstrings,
  example snippets) that an AI agent or editor can consume.
* :func:`register_system_routes` -- mounts ``GET /_system/help``,
  ``GET /_system/help/{name}`` and ``GET /_system/openapi`` so the same data is
  available over HTTP to any tool (including the bundled MCP server).

The data model mirrors FastAPI/Starlette's OpenAPI output but is enriched with
Python call signatures and handler docstrings, which are the most useful signals
for code-generation agents.
"""

import inspect
import json
import typing
from urllib.parse import quote

from .params import Path, Query, Header, Cookie, Body, File, Form
from .responses import JSONResponse
from ._justapi import Request
from .websockets import WebSocket


_PARAM_KINDS = {
    "Path": "path",
    "Query": "query",
    "Header": "header",
    "Cookie": "cookie",
    "Body": "body",
    "File": "file",
    "Form": "form",
}

_PARAM_TYPES = (Path, Query, Header, Cookie, Body, File, Form)


def _safe_hints(func):
    try:
        return typing.get_type_hints(func)
    except Exception:
        return getattr(func, "__annotations__", {})


def _ann_str(ann):
    if ann is None:
        return "Any"
    if isinstance(ann, str):
        return ann
    name = getattr(ann, "__name__", None)
    if name:
        return name
    return repr(ann).replace("typing.", "")


def _classify_param(name, param, hints, method, path):
    import re

    ann = hints.get(name)
    default = param.default
    path_in_tmpl = bool(re.search(r"\{" + re.escape(name) + r"\}", path or ""))
    info = {
        "name": name,
        "annotation": _ann_str(ann),
        "in": "query",
        "required": False,
        "default": None,
        "alias": None,
    }
    if path_in_tmpl:
        info["in"] = "path"
        info["required"] = True
        return info
    if isinstance(default, _PARAM_TYPES):
        info["in"] = _PARAM_KINDS[type(default).__name__]
        info["alias"] = getattr(default, "alias", None)
        dflt = getattr(default, "default", inspect.Parameter.empty)
        if dflt is inspect.Parameter.empty:
            info["required"] = True
        else:
            info["default"] = dflt
        return info
    ann_name = getattr(ann, "__name__", None)
    if ann is Request or ann_name == "Request":
        info["in"] = "request"
        return info
    if ann_name in ("WebSocket",):
        info["in"] = "websocket"
        return info
    write_methods = {"POST", "PUT", "PATCH", "QUERY"}
    if method in write_methods and param.default is inspect.Parameter.empty:
        info["in"] = "body"
        info["required"] = True
        return info
    if param.default is not inspect.Parameter.empty:
        info["in"] = "query"
        info["required"] = False
        info["default"] = default
        return info
    info["in"] = "query"
    info["required"] = True
    return info


def _build_descriptor(route):
    handler = route.get("handler")
    method = route.get("method", "GET")
    path = route.get("path", "")
    descriptor = {
        "name": route.get("name"),
        "method": method,
        "path": path,
        "summary": route.get("summary"),
        "description": route.get("description"),
        "tags": route.get("tags") or [],
        "deprecated": bool(route.get("deprecated")),
        "status_code": route.get("status_code"),
        "operation_id": route.get("operation_id"),
        "responses": route.get("responses"),
        "experimental": bool(route.get("experimental")),
    }
    params = []
    return_ann = None
    docstring = None
    is_websocket = method == "WS"
    if callable(handler):
        docstring = inspect.getdoc(handler)
        descriptor["docstring"] = docstring
        try:
            sig = inspect.signature(handler)
        except (ValueError, TypeError):
            sig = None
        hints = _safe_hints(handler)
        return_ann = hints.get("return")
        if sig is not None:
            descriptor["signature"] = f"def {getattr(handler, '__name__', 'handler')}{sig}"
            for pname, p in sig.parameters.items():
                if pname == "self":
                    continue
                classified = _classify_param(pname, p, hints, method, path)
                if classified["in"] in ("request", "websocket"):
                    continue
                params.append(classified)
    descriptor["parameters"] = params
    descriptor["returns"] = _ann_str(return_ann)
    descriptor["is_websocket"] = is_websocket
    descriptor["example"] = _example_snippet(descriptor)
    descriptor["explanation"] = _explain(descriptor)
    return descriptor


def _example_snippet(d):
    """Generate a minimal client + handler example for a route."""
    method = d["method"]
    path = d["path"]
    pp = [p for p in d["parameters"] if p["in"] == "path"]
    qp = [p for p in d["parameters"] if p["in"] == "query" and p.get("required")]
    path_example = path
    for p in pp:
        path_example = path_example.replace(
            "{" + p["name"] + "}", str(p.get("default") or "<" + p["name"] + ">")
        )
    query = ""
    if qp:
        query = "?" + "&".join(f"{p['name']}={p.get('default') or '<value>'}" for p in qp)
    lines = [
        "import requests",
        "",
        f"resp = requests.{method.lower()}(\"http://localhost:8000{path_example}{query}\")",
        "print(resp.status_code, resp.json())",
    ]
    return "\n".join(lines)


def _explain(d):
    parts = []
    if d.get("summary"):
        parts.append(d["summary"])
    if d.get("description"):
        parts.append(d["description"])
    if d.get("docstring"):
        parts.append(d["docstring"])
    param_lines = []
    for p in d["parameters"]:
        req = "required" if p.get("required") else "optional"
        alias = f" (alias={p['alias']})" if p.get("alias") else ""
        param_lines.append(
            f"- {p['name']} ({p['in']}, {req}, type={p['annotation']}){alias}"
        )
    if param_lines:
        parts.append("Parameters:\n" + "\n".join(param_lines))
    if d.get("returns") and d["returns"] != "Any":
        parts.append(f"Returns: {d['returns']}")
    if d.get("status_code"):
        parts.append(f"Default status: {d['status_code']}")
    return "\n".join(parts)


def collect_routes(app):
    """Return a list of per-route descriptors for an app (excludes /_system)."""
    out = []
    for route in getattr(app, "routes", []):
        if not isinstance(route, dict):
            continue
        if route.get("path", "").startswith("/_system"):
            continue
        if route.get("include_in_schema") is False:
            continue
        out.append(_build_descriptor(route))
    return out


def build_help(app):
    """Rich, AI-friendly description of the whole app."""
    return {
        "app": {
            "title": getattr(app, "title", "JustAPIApp"),
            "version": getattr(app, "version", "1.0.0"),
        },
        "named_routes": getattr(app, "_named_routes", {}),
        "route_count": len(collect_routes(app)),
        "routes": collect_routes(app),
    }


def build_openapi(app):
    """Synthesize a minimal OpenAPI 3.1 document from registered routes."""
    paths: dict = {}
    for d in collect_routes(app):
        path = d["path"]
        method = "get" if d["method"] == "WS" else d["method"].lower()
        op: dict = {}
        if d.get("summary"):
            op["summary"] = d["summary"]
        if d.get("description"):
            op["description"] = d["description"]
        if d.get("tags"):
            op["tags"] = d["tags"]
        if d.get("operation_id"):
            op["operation_id"] = d["operation_id"]
        if d.get("deprecated"):
            op["deprecated"] = True
        parameters = []
        request_body = None
        for p in d["parameters"]:
            if p["in"] == "path":
                parameters.append(
                    {
                        "name": p["name"],
                        "in": "path",
                        "required": True,
                        "schema": {"type": "string"},
                    }
                )
            elif p["in"] == "query":
                parameters.append(
                    {
                        "name": p["name"],
                        "in": "query",
                        "required": p.get("required", False),
                        "schema": {"type": "string"},
                    }
                )
            elif p["in"] == "header":
                parameters.append(
                    {
                        "name": p["name"],
                        "in": "header",
                        "required": p.get("required", False),
                        "schema": {"type": "string"},
                    }
                )
            elif p["in"] in ("body", "form", "file"):
                request_body = {
                    "content": {"application/json": {"schema": {"type": "object"}}}
                }
        if parameters:
            op["parameters"] = parameters
        if request_body:
            op["requestBody"] = request_body
        op["responses"] = d.get("responses") or {
            str(d.get("status_code") or 200): {"description": "Successful Response"}
        }
        paths.setdefault(path, {})[method] = op
    return {
        "openapi": "3.1.0",
        "info": {
            "title": getattr(app, "title", "JustAPIApp"),
            "version": getattr(app, "version", "1.0.0"),
        },
        "paths": paths,
    }


def _help_handler_factory(app):
    def help_handler(request=None):
        if request is not None:
            qp = {}
            try:
                qp = dict(request.query_params or {})
            except Exception:
                qp = {}
            name = qp.get("name")
            path = qp.get("path")
            if name or path:
                for d in collect_routes(app):
                    if (name and d.get("name") == name) or (path and d.get("path") == path):
                        return JSONResponse(d)
                return JSONResponse(
                    {"detail": f"route name={name!r} path={path!r} not found"},
                    status_code=404,
                )
        return JSONResponse(build_help(app))

    def openapi_handler():
        return JSONResponse(build_openapi(app))

    def named_handler(name: str):
        for d in collect_routes(app):
            if d.get("name") == name or d.get("path") == name:
                return JSONResponse(d)
        return JSONResponse({"detail": f"route {name!r} not found"}, status_code=404)

    async def tools_handler(request=None):
        return JSONResponse({"tools": app.list_tools(), "count": len(app.list_tools())})

    async def tools_call_handler(request):
        body = await request.json()
        name = body.get("name")
        arguments = body.get("arguments", {})
        if not name:
            return JSONResponse({"detail": "missing 'name'"}, status_code=400)
        try:
            result = app.call_tool(name, arguments)
        except (KeyError, Exception) as e:  # noqa: B014 - call_tool raises KeyError for unknown
            return JSONResponse({"detail": str(e)}, status_code=404)
        if inspect.iscoroutine(result):
            result = await result
        return JSONResponse(
            {
                "content": [{"type": "text", "text": json.dumps(result, default=str)}],
                "isError": False,
            }
        )

    return (
        help_handler,
        openapi_handler,
        named_handler,
        tools_handler,
        tools_call_handler,
    )


def register_system_routes(app):
    """Mount the ``/_system`` introspection routes onto an app.

    These routes are excluded from ``build_help`` output, so there is no
    recursion. They are served by the same Rust runtime as user routes.
    """
    help_handler, openapi_handler, named_handler, tools_handler, tools_call_handler = (
        _help_handler_factory(app)
    )
    app.get("/_system/help")(help_handler)
    app.get("/_system/openapi")(openapi_handler)
    app.get("/_system/help/{name}")(named_handler)
    app.get("/_system/tools")(tools_handler)
    app.post("/_system/tools/call")(tools_call_handler)
    return app
