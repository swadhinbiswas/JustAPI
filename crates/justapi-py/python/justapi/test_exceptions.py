"""Tests for FastAPI-compatible exception handling.

Covers:
- ``HTTPException`` -> proper JSON ``{"detail": ...}`` response with the
  exception's status code and headers.
- Unhandled exceptions -> 500 (not swallowed).
- ``RequestValidationError`` raised during param binding -> 422.
- ``WebSocketException`` -> the ``@app.websocket`` wrapper closes the socket
  with the given close code / reason.
"""

import asyncio
import json

import pytest

from justapi import (
    HTTPException,
    JustAPIApp,
    JustAPITestClient,
    RequestValidationError,
    WebSocketException,
    route_websocket,
)


def _header(resp, name):
    for k, v in resp["headers"].items():
        if k.lower() == name.lower():
            return v
    return None


def test_http_exception_basic():
    app = JustAPIApp()

    @app.get("/missing")
    async def missing(request):
        raise HTTPException(status_code=404, detail="Not found")

    resp = JustAPITestClient(app).get("/missing")
    assert resp["status"] == 404
    assert _header(resp, "content-type") == "application/json"
    assert json.loads(resp["body"])["detail"] == "Not found"


def test_http_exception_with_headers():
    app = JustAPIApp()

    @app.get("/unauth")
    async def unauth(request):
        raise HTTPException(
            status_code=401,
            detail="no token",
            headers={"WWW-Authenticate": "Bearer"},
        )

    resp = JustAPITestClient(app).get("/unauth")
    assert resp["status"] == 401
    assert _header(resp, "www-authenticate") == "Bearer"
    assert json.loads(resp["body"])["detail"] == "no token"


def test_http_exception_default_detail():
    app = JustAPIApp()

    @app.get("/boom")
    async def boom(request):
        raise HTTPException(status_code=400)

    resp = JustAPITestClient(app).get("/boom")
    assert resp["status"] == 400
    # No detail -> JSON null (matches FastAPI).
    assert json.loads(resp["body"])["detail"] is None


def test_unhandled_exception_is_500():
    app = JustAPIApp()

    @app.get("/explode")
    async def explode(request):
        raise ValueError("kaboom")

    resp = JustAPITestClient(app).get("/explode")
    assert resp["status"] == 500
    # Secure default: internal error details must NOT leak to the client.
    assert "kaboom" not in resp["body"].decode()
    assert json.loads(resp["body"])["error"] == "internal server error"


def test_request_validation_error_422():
    app = JustAPIApp()

    @app.get("/items")
    async def items(request, q: str):
        return {"q": q}

    resp = JustAPITestClient(app).get("/items")  # missing required `q`
    assert resp["status"] == 422
    assert _header(resp, "content-type") == "application/json"
    detail = json.loads(resp["body"])["detail"]
    assert isinstance(detail, list) and len(detail) > 0


def test_exception_classes():
    assert issubclass(HTTPException, Exception)
    assert issubclass(WebSocketException, Exception)
    assert issubclass(RequestValidationError, Exception)
    exc = HTTPException(418, detail="teapot")
    assert exc.status_code == 418 and exc.detail == "teapot" and exc.headers is None
    wse = WebSocketException(code=1008, reason="policy")
    assert wse.code == 1008 and wse.reason == "policy"


class FakeWebSocket:
    def __init__(self):
        self.closed = None

    async def close(self, code=None, reason=None):
        self.closed = (code, reason)


def test_websocket_exception_wrapper_closes():
    @route_websocket("/ws")
    async def handler(ws):
        raise WebSocketException(code=1008, reason="policy violation")

    ws = FakeWebSocket()
    asyncio.run(handler(ws))
    assert ws.closed == (1008, "policy violation")


def test_websocket_exception_default_code():
    @route_websocket("/ws")
    async def handler(ws):
        raise WebSocketException()

    ws = FakeWebSocket()
    asyncio.run(handler(ws))
    assert ws.closed == (1000, None)
