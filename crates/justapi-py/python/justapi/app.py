import inspect
import functools
import typing
from ._justapi import _JustAPIApp, TokenStreamResponse

class RequestValidationError(Exception):
    def __init__(self, errors):
        self.errors = errors
        super().__init__(str(errors))

class Depends:
    def __init__(self, dependency: typing.Callable, use_cache: bool = True):
        self.dependency = dependency
        self.use_cache = use_cache

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
                
            elif name == "request" or (hasattr(param.annotation, "__name__") and param.annotation.__name__ == "Request"):
                kwargs[name] = request
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
                
                if param_type is Path or (param_type is None and alias in request.get("path_params", {})):
                    val = request.get("path_params", {}).get(alias)
                    if val is not None: found = True
                
                if not found and (param_type is Query or (param_type is None and alias in request.get("query_params", {}))):
                    val = request.get("query_params", {}).get(alias)
                    if val is not None: found = True
                    
                if not found and param_type is Header:
                    headers = request.get("headers", [])
                    if isinstance(headers, list):
                        for hk, hv in headers:
                            if hk.decode("utf-8").lower() == alias.lower():
                                val = hv.decode("utf-8")
                                found = True
                                break
                    
                if not found and param_type is Cookie:
                    val = request.get("cookies", {}).get(alias)
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
                
            elif name == "request" or (hasattr(param.annotation, "__name__") and param.annotation.__name__ == "Request"):
                kwargs[name] = request
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
                
                if param_type is Path or (param_type is None and alias in request.get("path_params", {})):
                    val = request.get("path_params", {}).get(alias)
                    if val is not None: found = True
                
                if not found and (param_type is Query or (param_type is None and alias in request.get("query_params", {}))):
                    val = request.get("query_params", {}).get(alias)
                    if val is not None: found = True
                    
                if not found and param_type is Header:
                    headers = request.get("headers", [])
                    if isinstance(headers, list):
                        for hk, hv in headers:
                            if hk.decode("utf-8").lower() == alias.lower():
                                val = hv.decode("utf-8")
                                found = True
                                break
                    
                if not found and param_type is Cookie:
                    val = request.get("cookies", {}).get(alias)
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

    def _wrap_handler(self, handler, route_dependencies=None, route_middlewares=None):
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
                    from ._native_helper import wrap_result
                    return wrap_result(result)
                except Exception as e:
                    return self._handle_exception(request, e)
            return self._apply_middlewares(async_wrapper, route_middlewares)
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
                    from ._native_helper import wrap_result
                    return wrap_result(result)
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
                return self._apply_middlewares(async_base_handler, route_middlewares)
                
            return sync_wrapper

    def _resolve_batch_config(self, func):
        batch_size = getattr(func, "__batch_size__", None)
        batch_window_ms = getattr(func, "__batch_window_ms__", None)
        return batch_size, batch_window_ms

    def _wrap_batch_handler(self, handler):
        is_async = inspect.iscoroutinefunction(handler)
        if is_async:
            async def async_batch_wrapper(requests):
                return await handler(requests)
            return async_batch_wrapper
        else:
            def sync_batch_wrapper(requests):
                return handler(requests)
            return sync_batch_wrapper

    def get(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if handler is None:
            def decorator(func):
                self._app.get(path, self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares))
                return func
            return decorator
        self._app.get(path, self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares))
        return handler

    def post(self, path: str, handler=None, body_schema=None, schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if handler is None:
            def decorator(func):
                batch_size, batch_window_ms = self._resolve_batch_config(func)
                wrapped = self._wrap_batch_handler(func) if batch_size else self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares)
                self._app.post(path, wrapped, body_schema, schema, batch_size=batch_size, batch_window_ms=batch_window_ms)
                return func
            return decorator
        batch_size, batch_window_ms = self._resolve_batch_config(handler)
        wrapped = self._wrap_batch_handler(handler) if batch_size else self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares)
        self._app.post(path, wrapped, body_schema, schema, batch_size=batch_size, batch_window_ms=batch_window_ms)
        return handler

    def put(self, path: str, handler=None, body_schema=None, schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if handler is None:
            def decorator(func):
                batch_size, batch_window_ms = self._resolve_batch_config(func)
                wrapped = self._wrap_batch_handler(func) if batch_size else self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares)
                self._app.put(path, wrapped, body_schema, schema, batch_size=batch_size, batch_window_ms=batch_window_ms)
                return func
            return decorator
        batch_size, batch_window_ms = self._resolve_batch_config(handler)
        wrapped = self._wrap_batch_handler(handler) if batch_size else self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares)
        self._app.put(path, wrapped, body_schema, schema, batch_size=batch_size, batch_window_ms=batch_window_ms)
        return handler

    def delete(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if handler is None:
            def decorator(func):
                self._app.delete(path, self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares))
                return func
            return decorator
        self._app.delete(path, self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares))
        return handler

    def patch(self, path: str, handler=None, body_schema=None, schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if handler is None:
            def decorator(func):
                batch_size, batch_window_ms = self._resolve_batch_config(func)
                wrapped = self._wrap_batch_handler(func) if batch_size else self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares)
                self._app.patch(path, wrapped, body_schema, schema, batch_size=batch_size, batch_window_ms=batch_window_ms)
                return func
            return decorator
        batch_size, batch_window_ms = self._resolve_batch_config(handler)
        wrapped = self._wrap_batch_handler(handler) if batch_size else self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares)
        self._app.patch(path, wrapped, body_schema, schema, batch_size=batch_size, batch_window_ms=batch_window_ms)
        return handler

    def query(self, path: str, handler=None, body_schema=None, schema=None, experimental: bool = True, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        """Register a route for the HTTP QUERY method (RFC 10008).

        QUERY is safe and idempotent like GET, but carries a request body
        like POST — ideal for queries too large for the URI. By default the
        generated OpenAPI operation is tagged ``experimental``.
        """
        if handler is None:
            def decorator(func):
                self._app.query(path, self._wrap_handler(func, route_dependencies=dependencies, route_middlewares=middlewares), body_schema, schema, experimental)
                return func
            return decorator
        self._app.query(path, self._wrap_handler(handler, route_dependencies=dependencies, route_middlewares=middlewares), body_schema, schema, experimental)
        return handler

    def add_plugin(self, plugin):
        self._app.use(plugin)

    def use(self, plugin):
        self._app.use(plugin)

    def set_database(self, db):
        self._app.set_database(db)
        
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

    def include_router(self, router, prefix: str = ""):
        for route in router.routes:
            path = prefix + route["path"]
            # Combine dependencies: App -> Router -> Route
            combined_deps = router.dependencies + route["dependencies"]
            combined_mws = router.middlewares + route.get("middlewares", [])

            if route["method"] == "GET":
                self.get(path, route["handler"], dependencies=combined_deps, middlewares=combined_mws)
            elif route["method"] == "POST":
                self.post(path, route["handler"], body_schema=route["body_schema"], schema=route["schema"], dependencies=combined_deps, middlewares=combined_mws)
            elif route["method"] == "PUT":
                self.put(path, route["handler"], body_schema=route["body_schema"], schema=route["schema"], dependencies=combined_deps, middlewares=combined_mws)
            elif route["method"] == "PATCH":
                self.patch(path, route["handler"], body_schema=route["body_schema"], schema=route["schema"], dependencies=combined_deps, middlewares=combined_mws)
            elif route["method"] == "DELETE":
                self.delete(path, route["handler"], dependencies=combined_deps, middlewares=combined_mws)
            elif route["method"] == "QUERY":
                self.query(path, route["handler"], body_schema=route.get("body_schema"), schema=route.get("schema"), experimental=route.get("experimental", True), dependencies=combined_deps, middlewares=combined_mws)
            elif route["method"] == "SSE":
                self._app.get(path, self._wrap_sse_handler(route["handler"]))
            elif route["method"] == "WS":
                self._app.websocket(path, route["handler"])

    def run(self, addr: str):
        self._app.run(addr)

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
    def __init__(self, prefix: str = "", dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        self.prefix = prefix
        self.dependencies = dependencies or []
        self.middlewares = middlewares or []
        self.routes = []

    def get(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if handler is None:
            def decorator(func):
                self.routes.append({"method": "GET", "path": self.prefix + path, "handler": func, "dependencies": dependencies or [], "middlewares": middlewares or []})
                return func
            return decorator
        self.routes.append({"method": "GET", "path": self.prefix + path, "handler": handler, "dependencies": dependencies or [], "middlewares": middlewares or []})
        return handler

    def post(self, path: str, handler=None, body_schema=None, schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if handler is None:
            def decorator(func):
                self.routes.append({"method": "POST", "path": self.prefix + path, "handler": func, "body_schema": body_schema, "schema": schema, "dependencies": dependencies or [], "middlewares": middlewares or []})
                return func
            return decorator
        self.routes.append({"method": "POST", "path": self.prefix + path, "handler": handler, "body_schema": body_schema, "schema": schema, "dependencies": dependencies or [], "middlewares": middlewares or []})
        return handler

    def put(self, path: str, handler=None, body_schema=None, schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if handler is None:
            def decorator(func):
                self.routes.append({"method": "PUT", "path": self.prefix + path, "handler": func, "body_schema": body_schema, "schema": schema, "dependencies": dependencies or [], "middlewares": middlewares or []})
                return func
            return decorator
        self.routes.append({"method": "PUT", "path": self.prefix + path, "handler": handler, "body_schema": body_schema, "schema": schema, "dependencies": dependencies or [], "middlewares": middlewares or []})
        return handler

    def patch(self, path: str, handler=None, body_schema=None, schema=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if handler is None:
            def decorator(func):
                self.routes.append({"method": "PATCH", "path": self.prefix + path, "handler": func, "body_schema": body_schema, "schema": schema, "dependencies": dependencies or [], "middlewares": middlewares or []})
                return func
            return decorator
        self.routes.append({"method": "PATCH", "path": self.prefix + path, "handler": handler, "body_schema": body_schema, "schema": schema, "dependencies": dependencies or [], "middlewares": middlewares or []})
        return handler

    def delete(self, path: str, handler=None, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if handler is None:
            def decorator(func):
                self.routes.append({"method": "DELETE", "path": self.prefix + path, "handler": func, "dependencies": dependencies or [], "middlewares": middlewares or []})
                return func
            return decorator
        self.routes.append({"method": "DELETE", "path": self.prefix + path, "handler": handler, "dependencies": dependencies or [], "middlewares": middlewares or []})
        return handler

    def query(self, path: str, handler=None, body_schema=None, schema=None, experimental: bool = True, dependencies: typing.List[Depends] = None, middlewares: typing.List[typing.Callable] = None):
        if handler is None:
            def decorator(func):
                self.routes.append({"method": "QUERY", "path": self.prefix + path, "handler": func, "body_schema": body_schema, "schema": schema, "experimental": experimental, "dependencies": dependencies or [], "middlewares": middlewares or []})
                return func
            return decorator
        self.routes.append({"method": "QUERY", "path": self.prefix + path, "handler": handler, "body_schema": body_schema, "schema": schema, "experimental": experimental, "dependencies": dependencies or [], "middlewares": middlewares or []})
        return handler

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
        func.__route_info__ = {"method": "WS", "path": path, "dependencies": dependencies or []}
        return func
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
