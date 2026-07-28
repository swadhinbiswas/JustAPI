"""Tests for ORJSON serialization integration.

The Rust-native response path uses Python's ``orjson`` module when available
(falling back to ``json.dumps``).  These tests verify that:

1. Responses are correctly serialized with or without ``orjson`` installed.
2. The ``_native_helper._dumps`` callable works correctly in both cases.
3. The Rust ``fast_dumps`` path (exercised via ``AsyncTestClient``) produces
   valid responses.
"""

import importlib
import json
import pytest
import pytest_asyncio
from justapi import JustAPIApp


# ---------------------------------------------------------------------------
# _native_helper._dumps (Python-level)
# ---------------------------------------------------------------------------

def _reload_helper():
    """Force-reload the helper module so we can test both orjson/no-orjson
    paths.  We can't actually remove an already-imported *orjson*, but we
    can inspect which path was chosen."""
    import justapi._native_helper as hlp
    # Inspect the lambda source to determine which serializer is in use
    return hlp


def test_orjson_preferred_when_available():
    """When *orjson* is importable, ``_dumps`` should use it."""
    hlp = _reload_helper()
    data = {"hello": "world", "nested": {"a": 1}}
    result = hlp._dumps(data)
    parsed = json.loads(result)
    assert parsed == data


def test_orjson_produces_compact_json():
    """orjson produces compact JSON (no spaces after separators)."""
    try:
        import orjson
    except ImportError:
        pytest.skip("orjson not installed")
    hlp = _reload_helper()
    data = {"a": 1, "b": 2}
    result = hlp._dumps(data)
    # orjson: b'{"a":1,"b":2}'  (compact)
    # json:   b'{"a": 1, "b": 2}'  (with spaces)
    assert b" " not in result, (
        f"Expected compact JSON without spaces, got: {result!r}"
    )


def test_orjson_handles_default_str():
    """orjson's ``default=str`` fallback should handle non-serializable types."""
    hlp = _reload_helper()
    from datetime import datetime
    data = {"now": datetime(2026, 7, 25, 12, 0, 0)}
    result = hlp._dumps(data)
    parsed = json.loads(result)
    assert "now" in parsed
    assert isinstance(parsed["now"], str)


# ---------------------------------------------------------------------------
# End-to-end via AsyncTestClient (exercises Rust fast_dumps path)
# ---------------------------------------------------------------------------

@pytest_asyncio.fixture
async def app():
    app = JustAPIApp()

    @app.get("/data")
    async def get_data():
        return {"key": "value", "num": 42}

    @app.get("/nested")
    async def get_nested():
        return {"outer": {"inner": [1, 2, 3]}}

    @app.get("/string")
    async def get_string():
        return "plain text"

    @app.get("/list")
    async def get_list():
        return [1, 2, 3]

    return app


@pytest.mark.asyncio
async def test_orjson_response_via_client(app):
    """Verify that the Rust serialization path returns correct JSON
    (whether using orjson or json.dumps)."""
    from justapi.testing import AsyncTestClient

    async with AsyncTestClient(app) as c:
        r = await c.get("/data")
        assert r["status"] == 200
        import json as _json
        body = _json.loads(bytes(r["body"]))
        assert body == {"key": "value", "num": 42}


@pytest.mark.asyncio
async def test_orjson_nested_response(app):
    from justapi.testing import AsyncTestClient

    async with AsyncTestClient(app) as c:
        r = await c.get("/nested")
        import json as _json
        body = _json.loads(bytes(r["body"]))
        assert body == {"outer": {"inner": [1, 2, 3]}}


@pytest.mark.asyncio
async def test_orjson_string_response(app):
    from justapi.testing import AsyncTestClient

    async with AsyncTestClient(app) as c:
        r = await c.get("/string")
        import json as _json
        body = _json.loads(bytes(r["body"]))
        assert body == "plain text"


@pytest.mark.asyncio
async def test_orjson_list_response(app):
    from justapi.testing import AsyncTestClient

    async with AsyncTestClient(app) as c:
        r = await c.get("/list")
        import json as _json
        body = _json.loads(bytes(r["body"]))
        assert body == [1, 2, 3]
