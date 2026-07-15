import asyncio
import json
import socket
import threading

import pytest
import websockets

from justapi import JustAPIApp, WebSocket, WebSocketState, WebSocketDisconnect


def _free_port():
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def test_websocket_state_enum():
    assert WebSocketState.CONNECTING == 0
    assert WebSocketState.CONNECTED == 1
    assert WebSocketState.DISCONNECTED == 2
    assert WebSocketState.RESPONSE == 3


def test_websocket_disconnect_exception():
    exc = WebSocketDisconnect(code=1001, reason="bye")
    assert exc.code == 1001
    assert exc.reason == "bye"
    assert isinstance(exc, Exception)


def _build_app():
    app = JustAPIApp()

    @app.get("/items/{item_id}", name="item-detail")
    def item(request, item_id: int):
        return {"item_id": item_id}

    @app.websocket("/ws/echo")
    async def echo(ws: WebSocket):
        await ws.accept()
        scope = {
            "query": dict(ws.query_params),
            "cookies": dict(ws.cookies),
            "headers": {k: v for k, v in ws.headers.items()},
            "client": ws.client,
            "url": str(ws.url),
            "app_state": ws.application_state,
            "client_state": ws.client_state,
            "subprotocol": ws.subprotocol,
            "url_for": ws.url_for("item-detail", item_id=7),
        }
        await ws.send_json(scope)
        try:
            async for msg in ws.iter_text():
                await ws.send_text("echo:" + msg)
        except WebSocketDisconnect:
            pass
        finally:
            await ws.close()

    return app


@pytest.mark.asyncio
async def test_websocket_scope_and_json():
    port = _free_port()
    app = _build_app()
    t = threading.Thread(target=app.run, args=(f"127.0.0.1:{port}",), daemon=True)
    t.start()
    await asyncio.sleep(0.5)

    uri = f"ws://127.0.0.1:{port}/ws/echo?foo=bar&baz=qux"
    conn = await websockets.connect(
        uri, additional_headers={"x-custom": "hi", "cookie": "session=abc123"}
    )
    try:
        scope = json.loads(await conn.recv())
        assert scope["query"] == {"foo": "bar", "baz": "qux"}
        assert scope["cookies"] == {"session": "abc123"}
        assert scope["headers"]["x-custom"] == "hi"
        assert scope["client"] is not None and isinstance(scope["client"], (list, tuple))
        assert "/ws/echo?foo=bar&baz=qux" in scope["url"]
        assert scope["app_state"] == WebSocketState.CONNECTED
        assert scope["client_state"] == WebSocketState.CONNECTED
        assert scope["url_for"] == "/items/7"

        await conn.send("hello")
        assert await conn.recv() == "echo:hello"
        await conn.send("world")
        assert await conn.recv() == "echo:world"
    finally:
        try:
            await conn.close()
        except websockets.exceptions.ConnectionClosedOK:
            pass

    t.join(timeout=1.0)
