import asyncio
import inspect
import json
import contextvars
import concurrent.futures
import threading
import traceback

try:
    import orjson
    _dumps = lambda obj: orjson.dumps(obj, default=str)
except Exception:  # pragma: no cover - orjson is optional
    _dumps = lambda obj: json.dumps(obj, default=str).encode("utf-8")

# Import the Rust validator once, at module-load time. Importing it lazily
# *inside* `validate_body` (via a relative `from ._justapi import ...`)
# fails when that function is invoked from a GIL-pool worker thread
# ("attempted relative import with no known parent package"), which silently
# disables request-body validation on the live server path. Binding it
# here — on the main thread, with the package context intact — avoids
# that and keeps the Rust-native validator active under real concurrency.
try:
    from ._justapi import validate_value as _rust_validate_value
except Exception:  # pragma: no cover - defensive
    try:
        from justapi._justapi import validate_value as _rust_validate_value
    except Exception:
        _rust_validate_value = None

from justapi._justapi import validate_value

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
    # Streaming responses are returned unwrapped so the Rust side can pump the
    # generator into an SSE / TokenStream / validated-stream response.
    if type(result).__name__ in ("TokenStreamResponse", "StreamingResponse", "ValidatedStreamResponse"):
        return result

    # Schedule response-attached background tasks after the response is built.
    bg = getattr(result, "background", None)
    if bg is not None:
        bg.run()

    if hasattr(result, "to_dict"):
        return result.to_dict()
    if isinstance(result, dict):
        # Treat as a response envelope only when it carries a `body`, an explicit
        # `__response__` sentinel, or is `status`-only (`{"status": 204}` /
        # `{"status": 200, "headers": [...]}`). A data dict with a `"status"`
        # field *plus other keys* (`{"status": "ok", "products": 5}`) must be
        # serialized as normal JSON, otherwise its body is silently dropped
        # (BUG-1, PRODUCTION_PLAN.md P0.3).
        keys = set(result.keys())
        is_envelope = (
            result.get("__response__") is True
            or (
                "status" in result
                and keys <= {"status", "headers", "__response__"}
            )
            or (
                "body" in result
                and keys <= {"body", "status", "headers", "__response__"}
            )
        )
        if is_envelope:
            if "body" in result and isinstance(result["body"], str):
                result["body"] = result["body"].encode("utf-8")
            return result

    body = _dumps(result)
    if isinstance(body, str):
        body = body.encode("utf-8")
    return {
        "status": 200,
        "headers": [(b"content-type", b"application/json")],
        "body": body,
    }

def call_handler(handler, request):
    """Call a native Python handler with a request object.

    Returns the handler's raw return value (dict, list, str, ``Response``,
    streaming response, …). Response serialization is performed on the Rust
    side (``serialize_response``), which mirrors Robyn's fast path and avoids a
    Python ``wrap_result`` round-trip on every request.
    """
    try:
        result = handler(request)
        if inspect.isawaitable(result) and not isinstance(result, concurrent.futures.Future):
            loop = _get_loop()
            return asyncio.run_coroutine_threadsafe(result, loop)

        # Streaming responses are returned unwrapped so the Rust side can
        # pump the generator into an SSE / TokenStream / validated-stream response.
        if type(result).__name__ in ("TokenStreamResponse", "StreamingResponse", "ValidatedStreamResponse"):
            return result

        return result
    except Exception as e:
        if type(e).__name__ not in ("HTTPException", "RequestValidationError"):
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


def parse_body(body_bytes):
    """Parse a request body's JSON into a Python object (dict/list/scalar).

    Returns ``None`` for an empty body or invalid JSON. Used by the dispatch
    layer to attach the already-validated/parsed body to ``Request`` so schema
    routes receive the parsed object instead of re-parsing raw bytes.
    """
    if not body_bytes:
        return None
    try:
        return json.loads(body_bytes.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return None


def validate_body(schema_fn, body_bytes):
    """Validate a request body against a route's body schema.

    `schema_fn` is one of:
    * a ``justapi.Schema`` subclass — its generated JSON Schema is validated
      through the Rust ``validate_value`` engine (mirrors the native fast path);
    * a plain callable ``fn(body_dict) -> list[str] | None`` — invoked directly
      (legacy path), returning error strings or an empty/None list when valid.
    """
    if schema_fn is None:
        return []

    try:
        body_str = body_bytes.decode("utf-8")
        body_data = json.loads(body_str)
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        return [f"Invalid JSON body: {e}"]

    # Schema subclass: validate via its JSON Schema through the Rust engine.
    schema_json = getattr(schema_fn, "_schema_json", None)
    if callable(schema_json) and _rust_validate_value is not None:
        try:
            return _rust_validate_value(schema_json(), json.dumps(body_data))
        except Exception as e:  # pragma: no cover - defensive
            return [f"schema validation error: {e}"]

    # Legacy callable schema.
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


def _pump_validated_stream(generator, schema_json, sender, mode="ndjson"):
    """Pump a generator of JSON-serialisable objects, validating each against
    `schema_json` (in Rust) before forwarding. Invalid items abort the stream.

    `mode` is "ndjson" (one object per line) or "array" (a single JSON array).
    Sync and async generators are both supported.
    """
    async def pump():
        # Support both sync and async generators.
        if hasattr(generator, "__aiter__"):
            agen = generator
        else:
            async def awrap():
                for item in generator:
                    yield item
            agen = awrap()
        try:
            if mode == "array":
                sender.send(b"[")
            first = True
            async for item in agen:
                item_json = json.dumps(item, default=str)
                errors = validate_value(schema_json, item_json)
                if errors:
                    sender.send_error("validation failed: " + "; ".join(errors))
                    return
                if mode == "array":
                    if not first:
                        sender.send(b",")
                    sender.send(item_json.encode("utf-8"))
                    first = False
                else:
                    sender.send((item_json + "\n").encode("utf-8"))
            if mode == "array":
                sender.send(b"]")
        except Exception as e:
            import traceback as _tb
            _tb.print_exc()
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
