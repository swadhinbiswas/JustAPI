import inspect
import functools
import json
import re
import typing
from urllib.parse import quote
from ._justapi import _JustAPIApp, TokenStreamResponse, ValidatedStreamResponse, Database
from .exceptions import WebSocketException
from .websockets import WebSocket
from .system import register_system_routes, build_help, build_openapi
from .responses import PlainTextResponse, HTMLResponse, JSONResponse

_PY_TYPE_TO_JSON = {
    str: "string",
    int: "integer",
    float: "number",
    bool: "boolean",
    list: "array",
    dict: "object",
    type(None): "null",
}


# Built-in health/observability handlers. Registered on every JustAPIApp so the
# Python app exposes Kubernetes probe + metrics endpoints without relying on the
# Rust server's default-route registry (which is bypassed for Python apps, where
# routing happens in the Python handler). The Rust server emits richer Prometheus
# metrics via `with_health_registry` when used standalone.
def _builtin_health():
    return {"status": "ok", "components": {}}


def _builtin_live():
    return {"status": "alive"}


def _builtin_ready(self):
    ready, report = self._app.health_ready()
    try:
        payload = json.loads(report)
    except (ValueError, TypeError):
        payload = {"status": "ready" if ready else "not_ready"}
    if not ready:
        return JSONResponse(payload, status_code=503)
    return JSONResponse(payload)


def _builtin_metrics(self):
    body = self._app.metrics_prometheus()
    return PlainTextResponse(body)


def _builtin_openapi(app):
    """Serve the generated OpenAPI 3.1 document for the Python app."""
    return JSONResponse(build_openapi(app))


def _builtin_docs(app):
    """Serve Swagger UI (interactive API documentation) for the Python app."""
    html = (
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n"
        "  <meta charset=\"UTF-8\">\n  <title>API Docs — Swagger UI</title>\n"
        "  <link rel=\"stylesheet\" href=\"https://unpkg.com/swagger-ui-dist/swagger-ui.css\">\n"
        "</head>\n<body>\n  <div id=\"swagger-ui\"></div>\n"
        "  <script src=\"https://unpkg.com/swagger-ui-dist/swagger-ui-bundle.js\"></script>\n"
        "  <script>\n"
        "    window.onload = () => {\n"
        "      window.ui = SwaggerUIBundle({ url: '/openapi.json', dom_id: '#swagger-ui' });\n"
        "    };\n"
        "  </script>\n</body>\n</html>\n"
    )
    return HTMLResponse(html)


def _builtin_redoc(app):
    """Serve ReDoc (alternative API documentation) for the Python app."""
    html = (
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n"
        "  <meta charset=\"UTF-8\">\n  <title>API Docs — ReDoc</title>\n"
        "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n"
        "</head>\n<body>\n  <redoc spec-url='/openapi.json'></redoc>\n"
        "  <script src=\"https://unpkg.com/redoc/bundles/redoc.standalone.js\"></script>\n"
        "</body>\n</html>\n"
    )
    return HTMLResponse(html)


def _model_json_schema(ann):
    """Best-effort JSON schema from a Pydantic / dataclass / model annotation."""
    try:
        if hasattr(ann, "model_json_schema"):
            return ann.model_json_schema()
    except Exception:
        pass
    try:
        if hasattr(ann, "__pydantic_model__"):
            return ann.__pydantic_model__.schema()
    except Exception:
        pass
    try:
        import dataclasses

        if dataclasses.is_dataclass(ann):
            props, required = {}, []
            for f in dataclasses.fields(ann):
                props[f.name] = {"type": _PY_TYPE_TO_JSON.get(f.type, "string")}
                if f.default is dataclasses.MISSING:
                    required.append(f.name)
            return {"type": "object", "properties": props, "required": required}
    except Exception:
        pass
    return None


def _infer_tool_schema(func):
    """Infer a JSON Schema from a callable's signature / type hints."""
    sig = inspect.signature(func)
    hints = typing.get_type_hints(func)
    props, required = {}, []
    for pname, p in sig.parameters.items():
        if pname == "self":
            continue
        ann = hints.get(pname, p.annotation)
        if ann is inspect.Parameter.empty:
            ann = str
        schema = _model_json_schema(ann)
        if schema is None:
            schema = {"type": _PY_TYPE_TO_JSON.get(ann, "string")}
        props[pname] = schema
        if p.default is inspect.Parameter.empty:
            required.append(pname)
    return {
        "type": "object",
        "properties": props,
        "required": required,
    }


class RequestValidationError(Exception):
    def __init__(self, errors):
        self.errors = errors
        self.detail = errors
        super().__init__(str(errors))

class Depends:
    def __init__(self, dependency: typing.Callable, use_cache: bool = True):
        self.dependency = dependency
        self.use_cache = use_cache


class Session:
    """Agent session state, backed by the Rust session store on the app.

    Obtain one by declaring a ``session: Session`` parameter on a route handler
    (the id is read from the ``justapi_session`` cookie or ``?session=`` query
    param) or by calling ``app.create_session()`` directly.
    """

    def __init__(self, app: "JustAPIApp", session_id: str):
        self._app = app
        self.id = session_id

    def get(self):
        return self._app.get_session(self.id)

    def set(self, data: dict):
        return self._app.set_session(self.id, data)

    def update(self, **fields):
        return self._app.update_session(self.id, **fields)

    def delete(self):
        return self._app.delete_session(self.id)


def adaptive_batch(max_size: int = 32, window_ms: int = 10):
    """
    Decorator to enable adaptive batching for this route.
    The wrapped handler must accept a `requests: list[T]` and return a `list[R]` of the same size.
    """
    def wrapper(func):
        func.__batch_size__ = max_size
        func.__batch_window_ms__ = window_ms
        return func
    return wrapper

class JustAPIApp:
    def __init__(self, dependencies: typing.List[Depends] = None):
        self._app = _JustAPIApp()
        self.dependencies = dependencies or []
        self.exception_handlers = {}
        self.middlewares = []
        self._named_routes = {}
        self.routes = []
        self.title = "JustAPIApp"
        self.version = "1.0.0"

        # Built-in probe/metrics endpoints for the Python app.
        self.get("/health", _builtin_health, include_in_schema=False, name="builtin_health")
        self.get("/live", _builtin_live, include_in_schema=False, name="builtin_live")
        self.get("/ready", lambda: _builtin_ready(self), include_in_schema=False, name="builtin_ready")
        self.get("/metrics", lambda: _builtin_metrics(self), include_in_schema=False, name="builtin_metrics")
        # Interactive API documentation for the Python app. The core server's
        # /docs, /redoc, /openapi.json live only in the default router, which the
        # Python app replaces via with_handler(); these builtin routes restore
        # that DX so the README's "automatic interactive API documentation"
        # claim holds for Python apps too.
        self.get("/openapi.json", lambda: _builtin_openapi(self), include_in_schema=False, name="builtin_openapi")
        self.get("/docs", lambda: _builtin_docs(self), include_in_schema=False, name="builtin_docs")
        self.get("/redoc", lambda: _builtin_redoc(self), include_in_schema=False, name="builtin_redoc")

    def _record(self, method, path, handler, *, body_schema=None, schema=None,
                 experimental=False, **kw):
        """Record a route's metadata on the Python side for introspection.

        The Rust runtime owns actual dispatch; this list powers
        ``build_help`` / ``build_openapi`` and the ``/_system`` endpoints.
        """
        name = kw.get("name")
        if name is not None:
            self._named_routes[name] = path
        self.routes.append({
            "method": method,
            "path": path,
            "handler": handler,
            "body_schema": body_schema,
            "schema": schema,
            "experimental": experimental,
            "dependencies": [],
            "middlewares": [],
            "tags": kw.get("tags") or [],
            "summary": kw.get("summary"),
            "description": kw.get("description"),
            "deprecated": kw.get("deprecated"),
            "status_code": kw.get("status_code"),
            "responses": kw.get("responses"),
            "operation_id": kw.get("operation_id"),
            "openapi_extra": kw.get("openapi_extra"),
            "include_in_schema": kw.get("include_in_schema", True),
            "name": name,
        })

    def middleware(self, middleware_type: str = "http"):
        def decorator(func):
            if middleware_type == "http":
                self.middlewares.append(func)
            return func
        return decorator

    def add_exception_handler(self, exc_class, handler):
        self.exception_handlers[exc_class] = handler

    def _handle_exception(self, request, exc):
        for cls in type(exc).mro():
            if cls in self.exception_handlers:
                res = self.exception_handlers[cls](request, exc)
                return res
        raise exc

    async def _resolve_dependencies_list(self, deps: typing.List[Depends], request: dict):
        if "_dep_cache" not in request:
            request["_dep_cache"] = {}
            
        for dep in deps:
            dep_sig = inspect.signature(dep.dependency)
            dep_kwargs = await self._resolve_kwargs(dep_sig, request)
            
            if dep.use_cache and dep.dependency in request["_dep_cache"]:
                continue
                
            if inspect.iscoroutinefunction(dep.dependency):
                res = await dep.dependency(**dep_kwargs)
            else:
                res = dep.dependency(**dep_kwargs)
                
            if dep.use_cache:
                request["_dep_cache"][dep.dependency] = res

    async def _resolve_kwargs(self, sig, request):
        if "query_params" not in request:
            import urllib.parse
            qs = request.get("query_string", b"").decode("utf-8")
            parsed = urllib.parse.parse_qs(qs)
            # parse_qs returns lists, we just take the first item for now
            request["query_params"] = {k: v[0] for k, v in parsed.items()}
            
        if "_dep_cache" not in request:
            request["_dep_cache"] = {}

        kwargs = {}
        for name, param in sig.parameters.items():
            if isinstance(param.default, Depends):
                if param.default.use_cache and param.default.dependency in request["_dep_cache"]:
                    kwargs[name] = request["_dep_cache"][param.default.dependency]
                    continue
                    
                dep_sig = inspect.signature(param.default.dependency)
                dep_kwargs = await self._resolve_kwargs(dep_sig, request)
                if inspect.iscoroutinefunction(param.default.dependency):
                    res = await param.default.dependency(**dep_kwargs)
                else:
                    res = param.default.dependency(**dep_kwargs)
                    
                if param.default.use_cache:
                    request["_dep_cache"][param.default.dependency] = res
                kwargs[name] = res
                
            elif name in ("request", "req", "r", "_request") or (hasattr(param.annotation, "__name__") and param.annotation.__name__ == "Request"):
                kwargs[name] = request
            elif (hasattr(param.annotation, "__name__") and param.annotation.__name__ == "Session") or name == "session":
                sid = (request.cookies).get(getattr(self, "_session_cookie", "justapi_session"))
                if sid is None:
                    sid = request.query_params.get("session")
                if sid is None:
                    if getattr(self, "_sessions_enabled", False):
                        sid = self.create_session()
                        request["_session_id"] = sid
                elif getattr(self, "_sessions_enabled", False) and self.get_session(sid) is None:
                    self.create_session(id=sid)
                    request["_session_id"] = sid
                kwargs[name] = Session(self, sid)
            else:
                from .params import Param, Path, Query, Header, Cookie, Body, File, Form

                alias = name
                param_type = None
                default = inspect.Parameter.empty

                if isinstance(param.default, Param):
                    alias = param.default.alias or name
                    param_type = type(param.default)
                    default = param.default.default
                else:
                    default = param.default
                
                val = None
                found = False
                
                if param_type is Path or (param_type is None and alias in request.path_params):
                    val = request.path_params.get(alias)
                    if val is not None: found = True

                if not found and (param_type is Query or (param_type is None and alias in request.query_params)):
                    val = request.query_params.get(alias)
                    if val is not None: found = True

                if not found and param_type is Header:
                    val = request.headers.get(alias)
                    if val is not None:
                        found = True

                if not found and param_type is Cookie:
                    val = request.cookies.get(alias)
                    if val is not None: found = True
                    
                if not found and (param_type is File or param_type is Form or (hasattr(param.annotation, "__name__") and param.annotation.__name__ == "UploadFile")):
                    form_data = request.get("form")
                    if form_data:
                        val = form_data.get(alias)
                        if val is not None: found = True

                if not found and (param_type is Body or name == "body" or name == "payload"):
                    val = request.get("body")
                    if val is not None: found = True
                
                if not found and (name == "background_tasks" or (hasattr(param.annotation, "__name__") and param.annotation.__name__ == "BackgroundTasks")):
                    val = request.get("background_tasks")
                    found = True

                if found:
                    try:
                        if val is not None and param.annotation is int and isinstance(val, str):
                            kwargs[name] = int(val)
                        elif val is not None and param.annotation is float and isinstance(val, str):
                            kwargs[name] = float(val)
                        else:
                            kwargs[name] = val
                    except (ValueError, TypeError) as e:
                        raise RequestValidationError([{"loc": [alias], "msg": str(e), "type": "type_error"}])
                else:
                    if default is inspect.Parameter.empty:
                        raise RequestValidationError([{"loc": [alias], "msg": "field required", "type": "value_error.missing"}])
                    kwargs[name] = default
        return kwargs

    def _resolve_dependencies_list_sync(self, deps: typing.List[Depends], request: dict):
        if "_dep_cache" not in request:
            request["_dep_cache"] = {}
            
        for dep in deps:
            dep_sig = inspect.signature(dep.dependency)
            dep_kwargs = self._resolve_kwargs_sync(dep_sig, request)
            
            if dep.use_cache and dep.dependency in request["_dep_cache"]:
                continue
                
            res = dep.dependency(**dep_kwargs)
            if dep.use_cache:
                request["_dep_cache"][dep.dependency] = res

    def _resolve_kwargs_sync(self, sig, request):
        if "query_params" not in request:
            import urllib.parse
            qs = request.get("query_string", b"").decode("utf-8")
            parsed = urllib.parse.parse_qs(qs)
            request["query_params"] = {k: v[0] for k, v in parsed.items()}
            
        if "_dep_cache" not in request:
            request["_dep_cache"] = {}

        kwargs = {}
        for name, param in sig.parameters.items():
            if isinstance(param.default, Depends):
                if param.default.use_cache and param.default.dependency in request["_dep_cache"]:
                    kwargs[name] = request["_dep_cache"][param.default.dependency]
                    continue
                    
                dep_sig = inspect.signature(param.default.dependency)
                dep_kwargs = self._resolve_kwargs_sync(dep_sig, request)
                res = param.default.dependency(**dep_kwargs)
                
                if param.default.use_cache:
                    request["_dep_cache"][param.default.dependency] = res
                kwargs[name] = res
                
            elif name in ("request", "req", "r", "_request") or (hasattr(param.annotation, "__name__") and param.annotation.__name__ == "Request"):
                kwargs[name] = request
            elif (hasattr(param.annotation, "__name__") and param.annotation.__name__ == "Session") or name == "session":
                sid = (request.cookies).get(getattr(self, "_session_cookie", "justapi_session"))
                if sid is None:
                    sid = request.query_params.get("session")
                if sid is None:
                    if getattr(self, "_sessions_enabled", False):
                        sid = self.create_session()
                        request["_session_id"] = sid
                elif getattr(self, "_sessions_enabled", False) and self.get_session(sid) is None:
                    self.create_session(id=sid)
                    request["_session_id"] = sid
                kwargs[name] = Session(self, sid)
            else:
                from .params import Param, Path, Query, Header, Cookie, Body, File, Form

                alias = name
                param_type = None
                default = inspect.Parameter.empty

                if isinstance(param.default, Param):
                    alias = param.default.alias or name
                    param_type = type(param.default)
                    default = param.default.default
                else:
                    default = param.default
                
                val = None
                found = False
                
                if param_type is Path or (param_type is None and alias in request.path_params):
                    val = request.path_params.get(alias)
                    if val is not None: found = True

                if not found and (param_type is Query or (param_type is None and alias in request.query_params)):
                    val = request.query_params.get(alias)
                    if val is not None: found = True

                if not found and param_type is Header:
                    val = request.headers.get(alias)
                    if val is not None:
                        found = True

                if not found and param_type is Cookie:
                    val = request.cookies.get(alias)
                    if val is not None: found = True

                if not found and (param_type is File or param_type is Form or (hasattr(param.annotation, "__name__") and param.annotation.__name__ == "UploadFile")):
                    form_data = request.get("form")
                    if form_data:
                        val = form_data.get(alias)
                        if val is not None: found = True
                    
                if not found and (param_type is Body or name == "body" or name == "payload"):
                    val = request.get("body")
                    if val is not None: found = True
                
                if not found and (name == "background_tasks" or (hasattr(param.annotation, "__name__") and param.annotation.__name__ == "BackgroundTasks")):
                    val = request.get("background_tasks")
                    found = True

                if found:
                    try:
                        if val is not None and param.annotation is int and isinstance(val, str):
                            kwargs[name] = int(val)
                        elif val is not None and param.annotation is float and isinstance(val, str):
                            kwargs[name] = float(val)
                        else:
                            kwargs[name] = val
                    except (ValueError, TypeError) as e:
                        raise RequestValidationError([{"loc": [alias], "msg": str(e), "type": "type_error"}])
                else:
                    if default is inspect.Parameter.empty:
                        raise RequestValidationError([{"loc": [alias], "msg": "field required", "type": "value_error.missing"}])
                    kwargs[name] = default
        return kwargs
    def _apply_middlewares(self, base_handler, route_middlewares=None):
        if not self.middlewares and not route_middlewares:
            return base_handler
        
        all_middlewares = self.middlewares.copy()
        if route_middlewares:
            all_middlewares.extend(route_middlewares)
            
        chain = base_handler
        # apply in order: the first middleware added is the outermost
        for mw in reversed(all_middlewares):
            def make_wrapper(mw_func, next_func):
                async def wrapped(request):
                    return await mw_func(request, next_func)
                return wrapped
            chain = make_wrapper(mw, chain)
        return chain

    def _wrap_handler(self, handler, route_dependencies=None, route_middlewares=None, native=False):
        # The native Rust fast path validates + echoes the request and returns
        # without ever calling the Python handler, so it would silently SKIP any
        # route- or app-level `dependencies` (e.g. auth) and `middlewares`. That
        # is a security hole, so refuse the combination outright. (App-level
        # middleware is also bypassed by the native fast path by design — it is a
        # validate-and-echo shortcut — so do not rely on auth running there.)
        if native and (
            self.dependencies
            or route_dependencies
            or self.middlewares
            or route_middlewares
        ):
            raise ValueError(
                "native=True cannot be combined with dependencies or route "
                "middlewares: the native fast path validates and echoes the "
                "request without invoking the handler, so those would be "
                "silently bypassed. Use a non-native route, or move the logic "
                "into the native validation schema."
            )

        sig = inspect.signature(handler)
        route_deps = route_dependencies or []
        all_deps = self.dependencies + route_deps

        is_async = inspect.iscoroutinefunction(handler)
        for name, param in sig.parameters.items():
            if isinstance(param.default, Depends):
                if inspect.iscoroutinefunction(param.default.dependency):
                    is_async = True

        for dep in all_deps:
            if inspect.iscoroutinefunction(dep.dependency):
                is_async = True

        has_bg = "background_tasks" in sig.parameters or any(
            p.annotation.__name__ == "BackgroundTasks" for p in sig.parameters.values() if hasattr(p.annotation, "__name__")
        )

        # A handler needs the Python `Request` object (and thus one must be
        # built per request) if it takes any parameters, has any dependencies
        # to resolve, or is wrapped by any (route- or app-level) middleware —
        # middleware runs against the request and would break on an empty dict.
        # 0-parameter, dependency-free, middleware-free handlers can skip it.
        needs_request = (
            bool(sig.parameters)
            or bool(all_deps)
            or bool(self.middlewares)
            or bool(route_middlewares)
        )

        if is_async:
            async def async_wrapper(request):
                try:
                    bg_tasks = None
                    if has_bg:
                        from .background import BackgroundTasks
                        bg_tasks = BackgroundTasks()
                        request["background_tasks"] = bg_tasks

                    await self._resolve_dependencies_list(all_deps, request)
                    kwargs = await self._resolve_kwargs(sig, request)
                    
                    if inspect.iscoroutinefunction(handler):
                        result = await handler(**kwargs)
                    else:
                        result = handler(**kwargs)
                        
                    if bg_tasks: bg_tasks()
                    return result
                except Exception as e:
                    return self._handle_exception(request, e)
            wrapper = self._apply_middlewares(async_wrapper, route_middlewares)
        else:
            def sync_wrapper(request):
                try:
                    bg_tasks = None
                    if has_bg:
                        from .background import BackgroundTasks
                        bg_tasks = BackgroundTasks()
                        request["background_tasks"] = bg_tasks

                    self._resolve_dependencies_list_sync(all_deps, request)
                    kwargs = self._resolve_kwargs_sync(sig, request)
                    result = handler(**kwargs)
                    if bg_tasks: bg_tasks()
                    return result
                except Exception as e:
                    return self._handle_exception(request, e)
            
            if self.middlewares or route_middlewares:
                import asyncio
                import functools
                import contextvars
                async def async_base_handler(request):
                    loop = asyncio.get_running_loop()
                    ctx = contextvars.copy_context()
                    return await loop.run_in_executor(None, ctx.run, sync_wrapper, request)
                wrapper = self._apply_middlewares(async_base_handler, route_middlewares)
            else:
                wrapper = sync_wrapper

        wrapper._needs_request = needs_request
        return wrapper

    def _resolve_batch_config(self, func):
        batch_size = getattr(func, "__batch_size__", None)
        batch_window_ms = getattr(func, "__batch_window_ms__", None)
        return batch_size, batch_window_ms

    def _wrap_batch_handler(self, handler, native=False):
        if native and (self.dependencies or self.middlewares):
            raise ValueError(
                "native=True cannot be combined with dependencies or route "
                "middlewares: the native fast path validates and echoes the "
                "request without invoking the handler, so those would be "
                "silently bypassed."
            )
        is_async = inspect.iscoroutinefunction(handler)
        if is_async:
            async def async_batch_wrapper(requests):
                return await handler(requests)
            return async_batch_wrapper
        else:
            def sync_batch_wrapper(requests):
                return handler(requests)
            return sync_batch_wrapper

    def _meta(self, responses, openapi_extra):
        """Serialize the OpenAPI metadata dicts to JSON strings for the Rust layer."""
        import json
        responses_json = json.dumps(responses) if responses is not None else None
        extra_json = json.dumps(openapi_extra) if openapi_extra is not None else None
        return responses_json, extra_json

    def _route_kw(self, tags=None, summary=None, description=None, deprecated=None,
                  status_code=None, responses=None, operation_id=None, openapi_extra=None,
                  include_in_schema=True, name=None):
        responses_json, extra_json = self._meta(responses, openapi_extra)
        return dict(
            tags=tags,
            summary=summary,
            description=description,
            deprecated=deprecated or False,
            status_code=status_code,
            responses=responses_json,
            operation_id=operation_id,
            openapi_extra=extra_json,
            include_in_schema=include_in_schema,
            name=name,
        )

    def get(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True, query_schema=None, native: bool = False, crud_table: str = None, crud_columns: typing.List[str] = None):
        if name is not None: self._named_routes[name] = path
        kw = self._route_kw(tags, summary, description, deprecated, status_code, responses, operation_id, openapi_extra, include_in_schema, name=name)
        kw["native"] = native
        kw["crud_table"] = crud_table
        kw["crud_columns"] = crud_columns
        if handler is None and crud_table is not None:
            def _crud_noop(request):
                raise AssertionError("crud routes must not invoke the Python handler")
            self._app.get(path, _crud_noop, query_schema=query_schema, **kw)
            self._record("GET", path, _crud_noop, query_schema=query_schema, **kw)
            return _crud_noop
        if handler is None:
            def decorator(func):
                self._app.get(path, self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares), query_schema=query_schema, **kw)
                self._record("GET", path, func, query_schema=query_schema, **kw)
                return func
            return decorator
        self._app.get(path, self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares), query_schema=query_schema, **kw)
        self._record("GET", path, handler, query_schema=query_schema, **kw)
        return handler

    def post(self, path: str, handler=None, body_schema=None, schema=None, query_schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True, native: bool = False, crud_table: str = None, crud_columns: typing.List[str] = None):
        if name is not None: self._named_routes[name] = path
        kw = self._route_kw(tags, summary, description, deprecated, status_code, responses, operation_id, openapi_extra, include_in_schema, name=name)
        kw["native"] = native
        kw["crud_table"] = crud_table
        kw["crud_columns"] = crud_columns
        if handler is None and crud_table is not None:
            # Rust-native CRUD route with no Python handler: register a no-op
            # that is never invoked because the CRUD path short-circuits in Rust.
            def _crud_noop(request):
                raise AssertionError("crud routes must not invoke the Python handler")
            self._app.post(path, _crud_noop, body_schema, schema, query_schema=query_schema, **kw)
            self._record("POST", path, _crud_noop, body_schema=body_schema, schema=schema, query_schema=query_schema, **kw)
            return _crud_noop
        if handler is None:
            def decorator(func):
                batch_size, batch_window_ms = self._resolve_batch_config(func)
                wrapped = self._wrap_batch_handler(func, native=native) if batch_size else self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares, native=native)
                self._app.post(path, wrapped, body_schema, schema, query_schema=query_schema, batch_size=batch_size, batch_window_ms=batch_window_ms, **kw)
                self._record("POST", path, func, body_schema=body_schema, schema=schema, query_schema=query_schema, **kw)
                return func
            return decorator
        batch_size, batch_window_ms = self._resolve_batch_config(handler)
        wrapped = self._wrap_batch_handler(handler, native=native) if batch_size else self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares, native=native)
        self._app.post(path, wrapped, body_schema, schema, query_schema=query_schema, batch_size=batch_size, batch_window_ms=batch_window_ms, **kw)
        self._record("POST", path, handler, body_schema=body_schema, schema=schema, query_schema=query_schema, **kw)
        return handler

    def put(self, path: str, handler=None, body_schema=None, schema=None, query_schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True, native: bool = False, crud_table: str = None, crud_columns: typing.List[str] = None):
        if name is not None: self._named_routes[name] = path
        kw = self._route_kw(tags, summary, description, deprecated, status_code, responses, operation_id, openapi_extra, include_in_schema, name=name)
        kw["native"] = native
        kw["crud_table"] = crud_table
        kw["crud_columns"] = crud_columns
        if handler is None and crud_table is not None:
            def _crud_noop(request):
                raise AssertionError("crud routes must not invoke the Python handler")
            self._app.put(path, _crud_noop, body_schema, schema, query_schema=query_schema, **kw)
            self._record("PUT", path, _crud_noop, body_schema=body_schema, schema=schema, query_schema=query_schema, **kw)
            return _crud_noop
        if handler is None:
            def decorator(func):
                batch_size, batch_window_ms = self._resolve_batch_config(func)
                wrapped = self._wrap_batch_handler(func, native=native) if batch_size else self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares, native=native)
                self._app.put(path, wrapped, body_schema, schema, query_schema=query_schema, batch_size=batch_size, batch_window_ms=batch_window_ms, **kw)
                self._record("PUT", path, func, body_schema=body_schema, schema=schema, query_schema=query_schema, **kw)
                return func
            return decorator
        batch_size, batch_window_ms = self._resolve_batch_config(handler)
        wrapped = self._wrap_batch_handler(handler, native=native) if batch_size else self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares, native=native)
        self._app.put(path, wrapped, body_schema, schema, query_schema=query_schema, batch_size=batch_size, batch_window_ms=batch_window_ms, **kw)
        self._record("PUT", path, handler, body_schema=body_schema, schema=schema, query_schema=query_schema, **kw)
        return handler

    def delete(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True, query_schema=None, native: bool = False, crud_table: str = None, crud_columns: typing.List[str] = None):
        if name is not None: self._named_routes[name] = path
        kw = self._route_kw(tags, summary, description, deprecated, status_code, responses, operation_id, openapi_extra, include_in_schema, name=name)
        kw["crud_table"] = crud_table
        kw["crud_columns"] = crud_columns
        if handler is None and crud_table is not None:
            def _crud_noop(request):
                raise AssertionError("crud routes must not invoke the Python handler")
            self._app.delete(path, _crud_noop, query_schema=query_schema, native=native, **kw)
            self._record("DELETE", path, _crud_noop, query_schema=query_schema, native=native, **kw)
            return _crud_noop
        if handler is None:
            def decorator(func):
                self._app.delete(path, self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares), query_schema=query_schema, native=native, **kw)
                self._record("DELETE", path, func, query_schema=query_schema, native=native, **kw)
                return func
            return decorator
        self._app.delete(path, self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares), query_schema=query_schema, native=native, **kw)
        self._record("DELETE", path, handler, query_schema=query_schema, native=native, **kw)
        return handler

    def patch(self, path: str, handler=None, body_schema=None, schema=None, query_schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True, native: bool = False, crud_table: str = None, crud_columns: typing.List[str] = None):
        if name is not None: self._named_routes[name] = path
        kw = self._route_kw(tags, summary, description, deprecated, status_code, responses, operation_id, openapi_extra, include_in_schema, name=name)
        kw["native"] = native
        kw["crud_table"] = crud_table
        kw["crud_columns"] = crud_columns
        if handler is None and crud_table is not None:
            def _crud_noop(request):
                raise AssertionError("crud routes must not invoke the Python handler")
            self._app.patch(path, _crud_noop, body_schema, schema, query_schema=query_schema, **kw)
            self._record("PATCH", path, _crud_noop, body_schema=body_schema, schema=schema, query_schema=query_schema, **kw)
            return _crud_noop
        if handler is None:
            def decorator(func):
                batch_size, batch_window_ms = self._resolve_batch_config(func)
                wrapped = self._wrap_batch_handler(func, native=native) if batch_size else self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares, native=native)
                self._app.patch(path, wrapped, body_schema, schema, query_schema=query_schema, batch_size=batch_size, batch_window_ms=batch_window_ms, **kw)
                self._record("PATCH", path, func, body_schema=body_schema, schema=schema, query_schema=query_schema, **kw)
                return func
            return decorator
        batch_size, batch_window_ms = self._resolve_batch_config(handler)
        wrapped = self._wrap_batch_handler(handler, native=native) if batch_size else self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares, native=native)
        self._app.patch(path, wrapped, body_schema, schema, query_schema=query_schema, batch_size=batch_size, batch_window_ms=batch_window_ms, **kw)
        self._record("PATCH", path, handler, body_schema=body_schema, schema=schema, query_schema=query_schema, **kw)
        return handler

    def query(self, path: str, handler=None, body_schema=None, schema=None, query_schema=None, experimental: bool = True, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True, native: bool = False):
        """Register a route for the HTTP QUERY method (RFC 10008).

        QUERY is safe and idempotent like GET, but carries a request body
        like POST — ideal for queries too large for the URI. By default the
        generated OpenAPI operation is tagged ``experimental``.
        """
        kw = self._route_kw(tags, summary, description, deprecated, status_code, responses, operation_id, openapi_extra, include_in_schema, name=name)
        kw["native"] = native
        if handler is None:
            def decorator(func):
                self._app.query(path, self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares, native=native), body_schema, schema, query_schema=query_schema, experimental=experimental, **kw)
                self._record("QUERY", path, func, body_schema=body_schema, schema=schema, query_schema=query_schema, experimental=experimental, **kw)
                return func
            return decorator
        self._app.query(path, self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares, native=native), body_schema, schema, query_schema=query_schema, experimental=experimental, **kw)
        self._record("QUERY", path, handler, body_schema=body_schema, schema=schema, query_schema=query_schema, experimental=experimental, **kw)
        return handler

    def head(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True, query_schema=None, native: bool = False):
        if name is not None: self._named_routes[name] = path
        kw = self._route_kw(tags, summary, description, deprecated, status_code, responses, operation_id, openapi_extra, include_in_schema, name=name)
        if handler is None:
            def decorator(func):
                self._app.head(path, self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares), query_schema=query_schema, native=native, **kw)
                self._record("HEAD", path, func, query_schema=query_schema, native=native, **kw)
                return func
            return decorator
        self._app.head(path, self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares), query_schema=query_schema, native=native, **kw)
        self._record("HEAD", path, handler, query_schema=query_schema, native=native, **kw)
        return handler

    def options(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True, query_schema=None, native: bool = False):
        if name is not None: self._named_routes[name] = path
        kw = self._route_kw(tags, summary, description, deprecated, status_code, responses, operation_id, openapi_extra, include_in_schema, name=name)
        if handler is None:
            def decorator(func):
                self._app.options(path, self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares), query_schema=query_schema, native=native, **kw)
                self._record("OPTIONS", path, func, query_schema=query_schema, native=native, **kw)
                return func
            return decorator
        self._app.options(path, self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares), query_schema=query_schema, native=native, **kw)
        self._record("OPTIONS", path, handler, query_schema=query_schema, native=native, **kw)
        return handler

    def trace(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True, query_schema=None, native: bool = False):
        kw = self._route_kw(tags, summary, description, deprecated, status_code, responses, operation_id, openapi_extra, include_in_schema, name=name)
        if handler is None:
            def decorator(func):
                self._app.trace(path, self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares), query_schema=query_schema, native=native, **kw)
                self._record("TRACE", path, func, query_schema=query_schema, native=native, **kw)
                return func
            return decorator
        self._app.trace(path, self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares), query_schema=query_schema, native=native, **kw)
        self._record("TRACE", path, handler, query_schema=query_schema, native=native, **kw)
        return handler

    def frontend(self, path: str, directory: str, html: bool = True, check_dir: bool = True):
        """Serve a static frontend (SPA) from ``directory`` under ``path``.

        When ``html`` is true unknown routes fall back to ``index.html``
        (SPA client-side routing), mirroring FastAPI's ``StaticFiles(html=True)``.
        """
        fallback = "index.html" if html else None
        self._app.frontend(path, directory, fallback, check_dir)
        return directory

    def add_plugin(self, plugin):
        self._app.use(plugin)

    def use(self, plugin):
        self._app.use(plugin)

    def set_database(
        self,
        db,
        init_sql=None,
        pragmas=None,
        wal=False,
        acquire_timeout=None,
        idle_timeout=None,
        max_lifetime=None,
        health_check_interval=None,
        isolation=None,
    ):
        def _build(existing):
            return Database(
                existing.url,
                max_connections=existing.max_connections,
                init_sql=init_sql if init_sql is not None else existing.init_sql,
                pragmas=pragmas if pragmas is not None else existing.pragmas,
                acquire_timeout=acquire_timeout
                if acquire_timeout is not None
                else existing.acquire_timeout,
                idle_timeout=idle_timeout if idle_timeout is not None else existing.idle_timeout,
                max_lifetime=max_lifetime if max_lifetime is not None else existing.max_lifetime,
                health_check_interval=health_check_interval
                if health_check_interval is not None
                else existing.health_check_interval,
                isolation=isolation if isolation is not None else existing.isolation,
            )

        if isinstance(db, str):
            db = Database(
                db,
                init_sql=init_sql,
                pragmas=pragmas,
                acquire_timeout=acquire_timeout,
                idle_timeout=idle_timeout,
                max_lifetime=max_lifetime,
                health_check_interval=health_check_interval,
                isolation=isolation,
            )
        else:
            db = _build(db)
        if wal:
            existing = list(db.pragmas or [])
            if "journal_mode=WAL" not in existing:
                existing.append("journal_mode=WAL")
            db = Database(
                db.url,
                max_connections=db.max_connections,
                init_sql=db.init_sql,
                pragmas=existing,
                acquire_timeout=db.acquire_timeout,
                idle_timeout=db.idle_timeout,
                max_lifetime=db.max_lifetime,
                health_check_interval=db.health_check_interval,
                isolation=db.isolation,
            )
        self._app.set_database(db)

    @property
    def db(self):
        """Resolved DB pool handle (`DbPool`), or `None` before `app.run()`.

        Use it from handlers to run arbitrary, injection-safe SQL in Rust:

            @app.get("/reports")
            def reports(request):
                return app.db.query("SELECT * FROM items WHERE qty > ?", [3])
        """
        return self._app.db_pool()
        
    def enable_gateway(self, config_path: str):
        self._app.enable_gateway(config_path)

    def enable_circuit_breaker(self, failure_threshold: int = 5, reset_timeout_ms: int = 10000):
        """
        Enables a per-route circuit breaker.
        If a handler fails `failure_threshold` times in a row, the circuit opens,
        immediately rejecting requests with 503 Service Unavailable for `reset_timeout_ms`.
        """
        self._app.enable_circuit_breaker(failure_threshold, reset_timeout_ms)

    def enable_request_coalescing(self, headers: typing.List[str] = None):
        """
        Enable request coalescing (singleflight).

        When many concurrent, identical requests arrive for the same route, only
        one is allowed to reach the handler; the rest share its response. This
        collapses thundering-herd traffic on hot, read-only endpoints (e.g. a
        leaderboard or a model lookup) into a single upstream call.

        By default requests are keyed on ``(method, uri)``. Pass ``headers``
        (e.g. ``["accept"]``) to also key on those request header values, so
        distinct representations of the same resource are not collapsed together.
        """
        self._app.enable_request_coalescing(headers)

    def enable_secure_headers(self, with_hsts: bool = False):
        """
        Apply safe HTTP security headers to every response:
        ``X-Content-Type-Options: nosniff``, ``X-Frame-Options: DENY``,
        ``Content-Security-Policy: default-src 'self'``, and
        ``X-XSS-Protection: 0``.

        HSTS is omitted by default because the Python server terminates
        connections in plaintext. Pass ``with_hsts=True`` only when you
        terminate TLS in-process. Note that the default CSP (``'self'``) will
        block third-party/CDN resources — relax it via the Rust
        ``SecurityHeaders`` builder if your frontend needs them.
        """
        self._app.enable_secure_headers(with_hsts)

    def register_health_check(self, name: str, check: typing.Callable[[], bool]):
        """
        Register a dependency readiness probe used by the ``/ready`` endpoint.

        ``check`` is a zero-argument callable invoked synchronously under the
        GIL. Returning truthy (or raising nothing) means the dependency is
        healthy; returning falsy or raising marks it unhealthy and makes
        ``/ready`` return 503. The ``name`` identifies the component in the
        readiness report.
        """
        self._app.register_health_check(name, check)

    def set_grpc_addr(self, addr: str):
        self._app.set_grpc_addr(addr)
        
    def add_grpc_service(self, servicer, add_func_to_server):
        server = _JustAPIGrpcMockServer(self)
        add_func_to_server(servicer, server)

    def include_controller(self, controller_cls):
        path_prefix = getattr(controller_cls, "__controller_path__", "")
        controller_deps = getattr(controller_cls, "__controller_dependencies__", [])
        
        # Instantiate controller
        instance = controller_cls()
        
        for name, method in inspect.getmembers(instance, predicate=inspect.ismethod):
            # check if it has routing info
            if hasattr(method, "__route_info__"):
                route = method.__route_info__
                path = path_prefix + route["path"]
                combined_deps = controller_deps + route["dependencies"]
                
                if route["method"] == "GET":
                    self.get(path, method, dependencies=combined_deps)
                elif route["method"] == "POST":
                    self.post(path, method, body_schema=route["body_schema"], schema=route["schema"], dependencies=combined_deps)
                elif route["method"] == "PUT":
                    self.put(path, method, body_schema=route["body_schema"], schema=route["schema"], dependencies=combined_deps)
                elif route["method"] == "DELETE":
                    self.delete(path, method, dependencies=combined_deps)
                elif route["method"] == "QUERY":
                    self.query(path, method, body_schema=route["body_schema"], schema=route["schema"], experimental=route.get("experimental", True), dependencies=combined_deps)
                elif route["method"] == "SSE":
                    self._app.get(path, self._wrap_sse_handler(method))
                elif route["method"] == "WS":
                    self._app.websocket(path, method)

    def route(self, path: str, methods: typing.List[str] = None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if methods is None:
            methods = ["GET"]
        def decorator(func):
            for method in methods:
                m = method.upper()
                if m == "GET":
                    self.get(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "POST":
                    self.post(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "PUT":
                    self.put(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "DELETE":
                    self.delete(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "PATCH":
                    self.patch(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "QUERY":
                    self.query(path, func, dependencies=dependencies, middlewares=middlewares)
            return func
        return decorator

    def include_router(self, router, prefix: str = "", tags: typing.List[str] = None):
        for route in router.routes:
            path = prefix + route["path"]
            # Combine dependencies: App -> Router -> Route
            combined_deps = router.dependencies + route["dependencies"]
            combined_mws = router.middlewares + route.get("middlewares", [])
            # Include-level tags, then router-level tags, then the route's
            # own tags (FastAPI merge order).
            route_tags = (tags or []) + (router.tags or []) + (route.get("tags") or [])

            kw = dict(
                dependencies=combined_deps,
                middlewares=combined_mws,
                tags=route_tags or None,
                summary=route.get("summary"),
                description=route.get("description"),
                deprecated=route.get("deprecated", False),
                status_code=route.get("status_code"),
                responses=route.get("responses"),
                operation_id=route.get("operation_id"),
                openapi_extra=route.get("openapi_extra"),
                include_in_schema=route.get("include_in_schema", True),
                name=route.get("name"),
            )

            if route["method"] == "GET":
                self.get(path, route["handler"], **kw)
            elif route["method"] == "POST":
                self.post(path, route["handler"], body_schema=route.get("body_schema"), schema=route.get("schema"), **kw)
            elif route["method"] == "PUT":
                self.put(path, route["handler"], body_schema=route.get("body_schema"), schema=route.get("schema"), **kw)
            elif route["method"] == "PATCH":
                self.patch(path, route["handler"], body_schema=route.get("body_schema"), schema=route.get("schema"), **kw)
            elif route["method"] == "DELETE":
                self.delete(path, route["handler"], **kw)
            elif route["method"] == "QUERY":
                self.query(path, route["handler"], body_schema=route.get("body_schema"), schema=route.get("schema"), experimental=route.get("experimental", True), **kw)
            elif route["method"] == "HEAD":
                self.head(path, route["handler"], **kw)
            elif route["method"] == "OPTIONS":
                self.options(path, route["handler"], **kw)
            elif route["method"] == "TRACE":
                self.trace(path, route["handler"], **kw)
            elif route["method"] == "FRONTEND":
                self.frontend(path, route["directory"], html=route.get("html", True), check_dir=route.get("check_dir", True))
            elif route["method"] == "TRACE":
                self.trace(path, route["handler"], **kw)
            elif route["method"] == "SSE":
                self._app.get(path, self._wrap_sse_handler(route["handler"]))
            elif route["method"] == "WS":
                self._app.websocket(path, route["handler"])

    def url_for(self, name, **path_params):
        """Build a URL path for a named route, substituting path parameters.

        Mirrors FastAPI/Starlette ``request.url_for(name, **params)``.
        """
        try:
            path = self._named_routes[name]
        except KeyError:
            raise KeyError(f"No route named {name!r} is registered")
        def repl(m):
            p = m.group(1)
            if p not in path_params:
                raise KeyError(f"Missing path parameter {p!r} for route {name!r}")
            return quote(str(path_params[p]), safe="")
        return re.sub(r"{([^}]+)}", repl, path)

    # --- Native MCP tool surface (Rust registry; Python is thin glue) ---

    def tool(self, func=None, *, name=None, description=None, input_schema=None, annotations=None):
        """Register a Python callable as a native MCP tool.

        The tool is stored in the Rust registry and exposed over
        ``/_system/tools`` and the bundled MCP server. The input JSON schema is
        inferred from the function signature / type hints unless ``input_schema``
        is given. Schema inference is Python glue; the registry and serving live
        in Rust.

        Usable both bare (``@app.tool``) and with arguments
        (``@app.tool(description="...")``).

        Example::

            @app.tool(description="Add two numbers")
            def add(a: int, b: int) -> int:
                return a + b
        """
        def decorator(f):
            tool_name = name or f.__name__
            tool_desc = description or (inspect.getdoc(f) or f"Tool {tool_name}")
            schema_json = input_schema
            if schema_json is None:
                schema_json = _infer_tool_schema(f)
            if isinstance(schema_json, (dict, list)):
                schema_json = json.dumps(schema_json)
            self._app.register_tool(tool_name, tool_desc, schema_json, f)
            return f
        if func is not None and callable(func):
            return decorator(func)
        return decorator

    def list_tools(self):
        """Return registered tools in MCP ``tools/list`` shape (list of dicts)."""
        return self._app.list_tools()

    def call_tool(self, name, arguments=None):
        """Invoke a registered tool; returns the handler result (or a coroutine
        if the handler is async)."""
        return self._app.call_tool(name, json.dumps(arguments or {}))

    # --- Agent session state (Rust-backed store) ---

    def create_session(self, data: dict = None, id: str = None):
        """Create a new session, optionally seeded with a JSON dict and/or a
        caller-supplied id (used to materialize a known session id such as one
        passed via ``?session=``)."""
        raw = json.dumps(data) if data is not None else None
        return self._app.create_session(id, raw)

    def get_session(self, id: str):
        """Return a session's data as a dict, or None if unknown."""
        raw = self._app.get_session(id)
        return json.loads(raw) if raw else None

    def set_session(self, id: str, data: dict):
        return self._app.set_session(id, json.dumps(data))

    def update_session(self, id: str, **fields):
        return self._app.update_session(id, json.dumps(fields))

    def delete_session(self, id: str):
        return self._app.delete_session(id)

    def enable_sessions(self, cookie_name: str = "justapi_session"):
        """Enable automatic session id resolution for injected ``Session`` params."""
        self._sessions_enabled = True
        self._session_cookie = cookie_name
        return self

    # --- Streaming validated structured output ---

    def stream_json(self, path: str, schema=None, mode: str = "ndjson", handler=None, dependencies=None):
        """Register a route that streams JSON objects, validating each against
        ``schema`` (a JSON Schema dict/str) before it is sent to the client.

        The handler must return a (sync or async) generator yielding
        JSON-serialisable Python objects. ``mode`` is ``"ndjson"`` (one object
        per line) or ``"array"`` (a single JSON array). Validation runs in Rust.

        The handler's parameters participate in normal dependency injection, so
        ``session: Session`` and query-string params work exactly as on a regular
        route.
        """
        schema_json = schema if isinstance(schema, str) else json.dumps(schema or {})

        def decorator(func):
            @functools.wraps(func)
            def adapted(*args, **kwargs):
                gen = func(*args, **kwargs)
                return ValidatedStreamResponse(gen, schema_json, mode)

            self._app.get(path, self._wrap_handler(adapted, route_dependencies=dependencies))
            self._record("GET", path, func)
            return func

        if handler is None:
            return decorator
        return decorator(handler)

    def enable_system_routes(self):
        """Mount ``/_system/help``, ``/_system/help/{name}`` and ``/_system/openapi``.

        These expose the app's full route metadata (signatures, parameters,
        schemas, docstrings, examples) over HTTP for editors and AI agents.
        """
        register_system_routes(self)
        return self

    def run(self, addr: str, max_body_size: int = 50 * 1024 * 1024):
        self._app.run(addr, max_body_size)


    def _wrap_sse_handler(self, func):
        """Wrap a generator/async-generator handler into a TokenStreamResponse."""
        if inspect.isasyncgenfunction(func):
            def wrapper(request):
                return TokenStreamResponse(func(request))
            return wrapper

        def wrapper(request):
            gen = func(request)

            async def agen():
                for item in gen:
                    yield item

            return TokenStreamResponse(agen())
        return wrapper

    def sse(self, path: str, handler=None):
        """Register a Server-Sent Events route.

        The handler may be a sync or async generator yielding strings/bytes.
        Each yielded item is streamed to the client as an SSE event.
        """
        if handler is None:
            def decorator(func):
                self._app.get(path, self._wrap_sse_handler(func))
                return func
            return decorator
        self._app.get(path, self._wrap_sse_handler(handler))
        return handler

    def websocket(self, path: str, handler=None):
        """Register a WebSocket route.

        The handler is an async function receiving a single `WebSocket` object:
            async def handler(ws):
                await ws.accept()
                msg = await ws.receive_text()
                await ws.send_text(msg)
                await ws.close()
        """
        if handler is None:
            def decorator(func):
                self._app.websocket(path, func)
                return func
            return decorator
        self._app.websocket(path, handler)
        return handler

    @property
    def _inner(self):
        return self._app

class APIRouter:
    def __init__(self, prefix: str = "", tags: typing.List[str] = None, responses: dict = None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, callbacks: typing.List[typing.Callable] = None, deprecated: bool = False, name: str = None, include_in_schema: bool = True):
        self.prefix = prefix
        self.tags = tags or []
        self.responses = responses
        self.dependencies = dependencies or []
        self.middlewares = middlewares or []
        self.callbacks = callbacks or []
        self.deprecated = deprecated
        self.include_in_schema = include_in_schema
        self.routes = []
        self._named_routes = {}

    def _store(self, method, path, handler, body_schema=None, schema=None, experimental=False,
               dependencies=None, middlewares=None, tags=None, summary=None, description=None,
               deprecated=None, status_code=None, responses=None, operation_id=None,
               openapi_extra=None, include_in_schema=None, name=None):
        full_path = self.prefix + path
        # Route-level tags override the router default; include-level merging
        # happens in JustAPIApp.include_router.
        if name is not None:
            self._named_routes[name] = full_path
        route_tags = tags if tags is not None else (self.tags or [])
        route_responses = responses if responses is not None else self.responses
        route_deprecated = deprecated if deprecated else self.deprecated
        route_include = include_in_schema if include_in_schema is not None else self.include_in_schema
        self.routes.append({
            "method": method,
            "path": full_path,
            "handler": handler,
            "body_schema": body_schema,
            "schema": schema,
            "experimental": experimental,
            "dependencies": dependencies or [],
            "middlewares": middlewares or [],
            "tags": route_tags or [],
            "summary": summary,
            "description": description,
            "deprecated": route_deprecated,
            "status_code": status_code,
            "responses": route_responses,
            "operation_id": operation_id,
            "openapi_extra": openapi_extra,
            "include_in_schema": route_include,
            "name": name,
        })

    def get(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True):
        if handler is None:
            def decorator(func):
                self._store("GET", path, func, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
                return func
            return decorator
        self._store("GET", path, handler, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
        return handler

    def post(self, path: str, handler=None, body_schema=None, schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True):
        if handler is None:
            def decorator(func):
                self._store("POST", path, func, body_schema=body_schema, schema=schema, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
                return func
            return decorator
        self._store("POST", path, handler, body_schema=body_schema, schema=schema, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
        return handler

    def put(self, path: str, handler=None, body_schema=None, schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True):
        if handler is None:
            def decorator(func):
                self._store("PUT", path, func, body_schema=body_schema, schema=schema, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
                return func
            return decorator
        self._store("PUT", path, handler, body_schema=body_schema, schema=schema, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
        return handler

    def patch(self, path: str, handler=None, body_schema=None, schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True):
        if handler is None:
            def decorator(func):
                self._store("PATCH", path, func, body_schema=body_schema, schema=schema, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
                return func
            return decorator
        self._store("PATCH", path, handler, body_schema=body_schema, schema=schema, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
        return handler

    def delete(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True):
        if handler is None:
            def decorator(func):
                self._store("DELETE", path, func, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
                return func
            return decorator
        self._store("DELETE", path, handler, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
        return handler

    def query(self, path: str, handler=None, body_schema=None, schema=None, experimental: bool = True, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True):
        if handler is None:
            def decorator(func):
                self._store("QUERY", path, func, body_schema=body_schema, schema=schema, experimental=experimental, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
                return func
            return decorator
        self._store("QUERY", path, handler, body_schema=body_schema, schema=schema, experimental=experimental, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
        return handler

    def head(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True):
        if handler is None:
            def decorator(func):
                self._store("HEAD", path, func, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
                return func
            return decorator
        self._store("HEAD", path, handler, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
        return handler

    def options(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True):
        if handler is None:
            def decorator(func):
                self._store("OPTIONS", path, func, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
                return func
            return decorator
        self._store("OPTIONS", path, handler, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
        return handler

    def trace(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None, tags: typing.List[str] = None, summary: str = None, description: str = None, deprecated: bool = False, status_code: int = None, responses: dict = None, operation_id: str = None, openapi_extra: dict = None, name: str = None, include_in_schema: bool = True):
        if handler is None:
            def decorator(func):
                self._store("TRACE", path, func, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
                return func
            return decorator
        self._store("TRACE", path, handler, dependencies=dependencies, middlewares=middlewares, tags=tags, summary=summary, description=description, deprecated=deprecated, status_code=status_code, responses=responses, operation_id=operation_id, openapi_extra=openapi_extra, include_in_schema=include_in_schema, name=name)
        return handler

    def frontend(self, path: str, directory: str, html: bool = True, check_dir: bool = True):
        """Register a static frontend (SPA) mount under ``path``."""
        self.routes.append({"method": "FRONTEND", "path": path, "directory": directory, "html": html, "check_dir": check_dir})

    def sse(self, path: str, handler=None, dependencies: typing.List[Depends] = None):
        if handler is None:
            def decorator(func):
                self.routes.append({"method": "SSE", "path": self.prefix + path, "handler": func, "dependencies": dependencies or []})
                return func
            return decorator
        self.routes.append({"method": "SSE", "path": self.prefix + path, "handler": handler, "dependencies": dependencies or []})
        return handler

    def websocket(self, path: str, handler=None, dependencies: typing.List[Depends] = None):
        if handler is None:
            def decorator(func):
                self.routes.append({"method": "WS", "path": self.prefix + path, "handler": func, "dependencies": dependencies or []})
                return func
            return decorator
        self.routes.append({"method": "WS", "path": self.prefix + path, "handler": handler, "dependencies": dependencies or []})
        return handler

    def route(self, path: str, methods: typing.List[str] = None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if methods is None:
            methods = ["GET"]
        def decorator(func):
            for method in methods:
                m = method.upper()
                if m == "GET":
                    self.get(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "POST":
                    self.post(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "PUT":
                    self.put(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "DELETE":
                    self.delete(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "PATCH":
                    self.patch(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "QUERY":
                    self.query(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "HEAD":
                    self.head(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "OPTIONS":
                    self.options(path, func, dependencies=dependencies, middlewares=middlewares)
                elif m == "TRACE":
                    self.trace(path, func, dependencies=dependencies, middlewares=middlewares)
            return func
        return decorator

    def include_router(self, router, prefix: str = "", tags: typing.List[str] = None):
        self._named_routes.update(router._named_routes)
        for route in router.routes:
            new_route = route.copy()
            # route["path"] already has router.prefix; prepend our prefix + include prefix.
            new_route["path"] = self.prefix + prefix + route["path"]
            new_route["dependencies"] = router.dependencies + route["dependencies"]
            new_route["middlewares"] = router.middlewares + route.get("middlewares", [])
            # Include-level tags are prepended to the route's existing tags.
            new_route["tags"] = (tags or []) + route.get("tags", [])
            self.routes.append(new_route)

    def url_for(self, name, **path_params):
        """Build a URL path for a named route, substituting path parameters."""
        try:
            path = self._named_routes[name]
        except KeyError:
            raise KeyError(f"No route named {name!r} is registered")
        def repl(m):
            p = m.group(1)
            if p not in path_params:
                raise KeyError(f"Missing path parameter {p!r} for route {name!r}")
            return quote(str(path_params[p]), safe="")
        return re.sub(r"{([^}]+)}", repl, path)

class Controller:
    """Base class for controller classes."""
    pass

def controller(path: str, dependencies: typing.List[Depends] = None):
    def decorator(cls):
        cls.__controller_path__ = path
        cls.__controller_dependencies__ = dependencies or []
        return cls
    return decorator

def route_get(path: str, dependencies: typing.List[Depends] = None):
    def decorator(func):
        func.__route_info__ = {"method": "GET", "path": path, "dependencies": dependencies or []}
        return func
    return decorator

def route_post(path: str, body_schema=None, schema=None, dependencies: typing.List[Depends] = None):
    def decorator(func):
        func.__route_info__ = {"method": "POST", "path": path, "body_schema": body_schema, "schema": schema, "dependencies": dependencies or []}
        return func
    return decorator

def route_put(path: str, body_schema=None, schema=None, dependencies: typing.List[Depends] = None):
    def decorator(func):
        func.__route_info__ = {"method": "PUT", "path": path, "body_schema": body_schema, "schema": schema, "dependencies": dependencies or []}
        return func
    return decorator

def route_patch(path: str, body_schema=None, schema=None, dependencies: typing.List[Depends] = None):
    def decorator(func):
        func.__route_info__ = {"method": "PATCH", "path": path, "body_schema": body_schema, "schema": schema, "dependencies": dependencies or []}
        return func
    return decorator

def route_delete(path: str, dependencies: typing.List[Depends] = None):
    def decorator(func):
        func.__route_info__ = {"method": "DELETE", "path": path, "dependencies": dependencies or []}
        return func
    return decorator

def route_query(path: str, body_schema=None, schema=None, experimental: bool = True, dependencies: typing.List[Depends] = None):
    def decorator(func):
        func.__route_info__ = {"method": "QUERY", "path": path, "body_schema": body_schema, "schema": schema, "experimental": experimental, "dependencies": dependencies or []}
        return func
    return decorator

def route_sse(path: str, dependencies: typing.List[Depends] = None):
    def decorator(func):
        func.__route_info__ = {"method": "SSE", "path": path, "dependencies": dependencies or []}
        return func
    return decorator

def route_websocket(path: str, dependencies: typing.List[Depends] = None):
    def decorator(func):
        @functools.wraps(func)
        async def wrapper(websocket):
            ws = WebSocket(websocket)
            try:
                return await func(ws)
            except WebSocketException as exc:
                # FastAPI/Starlette semantics: a raised WebSocketException
                # closes the socket with the given close code and reason.
                await ws.close(exc.code, exc.reason)
                return
        wrapper.__route_info__ = {"method": "WS", "path": path, "dependencies": dependencies or []}
        return wrapper
    return decorator

class _JustAPIGrpcContext:
    def __init__(self):
        self._code = None
        self._details = None
    def set_code(self, code):
        self._code = code
    def set_details(self, details):
        self._details = details
    def abort(self, code, details):
        raise Exception(f"gRPC Abort: {code} - {details}")

class _JustAPIGrpcMockServer:
    def __init__(self, app):
        self.app = app
        
    def add_generic_rpc_handlers(self, generic_rpc_handlers):
        for handler in generic_rpc_handlers:
            if hasattr(handler, "_method_handlers"):
                self._register_handlers(handler._method_handlers)
            
    def add_registered_method_handlers(self, service_name, method_handlers):
        self._register_handlers(method_handlers)
        
    def _register_handlers(self, method_handlers):
        import inspect
        for method_name, m_handler in method_handlers.items():
            if not method_name.startswith("/"):
                # If they pass raw names without the leading slash
                pass
            def make_wrapper(mh):
                def wrapper(payload: bytes) -> bytes:
                    req = mh.request_deserializer(payload)
                    ctx = _JustAPIGrpcContext()
                    if mh.unary_unary is not None:
                        # Extract the actual method if it's bound
                        func = mh.unary_unary
                        if hasattr(func, "__func__"):
                            real_func = func.__func__
                        else:
                            real_func = func
                            
                        if inspect.iscoroutinefunction(real_func):
                            import asyncio
                            res = asyncio.run(func(req, ctx))
                        else:
                            res = func(req, ctx)
                        return mh.response_serializer(res)
                    else:
                        raise NotImplementedError("Only unary-unary gRPC is supported currently")
                return wrapper
                
            self.app._app.add_grpc_service(method_name, make_wrapper(m_handler))

from ._justapi import JustAPITestClient as _JustAPITestClient

class JustAPITestClient:
    def __new__(cls, app, database=None):
        inner = getattr(app, "_inner", app)
        return _JustAPITestClient(inner, database=database)

JustAPI = JustAPIApp
