import asyncio
import inspect
import json
import contextvars
import concurrent.futures
import threading
import traceback

_trace_id_var = contextvars.ContextVar("trace_id", default=None)
_span_id_var = contextvars.ContextVar("span_id", default=None)

def set_trace_context(trace_id, span_id):
    _trace_id_var.set(trace_id)
    _span_id_var.set(span_id)

def get_trace_context():
    return {
        "trace_id": _trace_id_var.get(),
        "span_id": _span_id_var.get(),
    }

_loop = None
_loop_thread = None
_loop_lock = None

def _start_loop(loop):
    asyncio.set_event_loop(loop)
    loop.run_forever()

def _get_loop():
    global _loop, _loop_thread, _loop_lock
    if _loop_lock is None:
        # This itself is a slight race but much safer than creating loops
        _loop_lock = threading.Lock()

    with _loop_lock:
        if _loop is None:
            _loop = asyncio.new_event_loop()
            _loop_thread = threading.Thread(target=_start_loop, args=(_loop,), daemon=True)
            _loop_thread.start()
    return _loop

def wrap_result(result):
    if hasattr(result, "to_dict"):
        return result.to_dict()
    if isinstance(result, dict) and ("body" in result or "status" in result):
        if "body" in result and isinstance(result["body"], str):
            result["body"] = result["body"].encode("utf-8")
        return result

    body = json.dumps(result, default=str).encode("utf-8")
    return {
        "status": 200,
        "headers": [(b"content-type", b"application/json")],
        "body": body,
    }

def call_handler(handler, request):
    """Call a native Python handler with a request dict.

    The handler may be sync or async. Returns a response dict
    with "status", "headers", and "body" keys.
    """
    try:
        result = handler(request)
        if inspect.isawaitable(result) and not isinstance(result, concurrent.futures.Future):
            loop = _get_loop()
            return asyncio.run_coroutine_threadsafe(result, loop)

        # Streaming responses are returned unwrapped so the Rust side can
        # pump the generator into an SSE / TokenStream response.
        if type(result).__name__ in ("TokenStreamResponse", "StreamingResponse"):
            return result

        return wrap_result(result)
    except Exception as e:
        print(f"ERROR in call_handler: {repr(e)}")
        traceback.print_exc()
        raise e


def call_batch_handler(handler, requests):
    """Call a native Python batch handler with a list of request dicts.

    The handler may be sync or async. Returns a list of response dicts
    with "status", "headers", and "body" keys.
    """
    try:
        result = handler(requests)
        if inspect.isawaitable(result) and not isinstance(result, concurrent.futures.Future):
            loop = _get_loop()
            future = asyncio.run_coroutine_threadsafe(result, loop)
            # The future will resolve to a list of results, we need to wrap them
            # We can't do it directly here since it's a concurrent.futures.Future,
            # so we'll just return the future and wrap the results in Rust.
            return future

        return [wrap_result(r) for r in result]
    except Exception as e:
        print(f"ERROR in call_batch_handler: {repr(e)}")
        traceback.print_exc()
        raise e


def validate_body(schema_fn, body_bytes):
    """Validate a request body against a schema function.

    The schema function receives the parsed JSON body and should return
    a list of error strings, or None/[] if valid.
    """
    if schema_fn is None:
        return []

    try:
        body_str = body_bytes.decode("utf-8")
        body_data = json.loads(body_str)
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        return [f"Invalid JSON body: {e}"]

    result = schema_fn(body_data)
    if result is None:
        return []
    return result

def _pump_stream(generator, sender):
    async def pump():
        try:
            async for chunk in generator:
                if isinstance(chunk, str):
                    chunk = chunk.encode("utf-8")
                sender.send(chunk)
        except Exception as e:
            sender.send_error(str(e))
        finally:
            sender.close()

    loop = _get_loop()
    asyncio.run_coroutine_threadsafe(pump(), loop)

def call_plugin_hook(plugin, hook_name):
    """Call a plugin lifecycle hook (sync or async)."""
    if not hasattr(plugin, hook_name):
        return
    hook = getattr(plugin, hook_name)
    result = hook()
    if inspect.isawaitable(result) and not isinstance(result, concurrent.futures.Future):
        loop = _get_loop()
        future = asyncio.run_coroutine_threadsafe(result, loop)
        # block the current thread until the coroutine finishes, 
        # since plugin hooks (like startup) must finish before proceeding
        future.result()


def run_ws_handler(coro):
    """Schedule a WebSocket handler coroutine on the daemon event loop.

    The coroutine owns the WebSocket connection for its lifetime; we do not
    block waiting for it to complete.
    """
    loop = _get_loop()
    asyncio.run_coroutine_threadsafe(coro, loop)
