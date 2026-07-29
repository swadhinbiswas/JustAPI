"""Integration tests for the @app.websocket() and @app.sse() decorators."""

import multiprocessing
import time

import pytest
import requests

from justapi import JustAPIApp

WEBSOCKETS_AVAILABLE = True
try:
    import websockets
except ImportError:
    WEBSOCKETS_AVAILABLE = False


PORT = 8091


def run_server():
    app = JustAPIApp()

    @app.websocket("/ws")
    async def ws_echo(ws):
        await ws.accept()
        msg = await ws.receive_text()
        await ws.send_text("echo:" + msg)
        await ws.close()

    @app.sse("/stream")
    async def sse_stream(req):
        for i in range(3):
            yield f"data: {i}\n\n"

    @app.get("/hello")
    def hello(req):
        return {"message": "hello"}

    app.run(f"127.0.0.1:{PORT}")


@pytest.fixture(scope="module")
def server():
    p = multiprocessing.Process(target=run_server)
    p.start()
    # Wait for server to be ready with retry
    for _ in range(20):
        try:
            requests.get(f"http://127.0.0.1:{PORT}/hello", timeout=1)
            break
        except (requests.ConnectionError, requests.Timeout):
            time.sleep(0.5)
    try:
        yield
    finally:
        p.terminate()
        p.join(timeout=5)


def test_sse_stream(server):
    with requests.get(f"http://127.0.0.1:{PORT}/stream", stream=True) as r:
        assert r.status_code == 200
        assert r.headers.get("content-type", "").startswith("text/event-stream")
        data = b""
        for chunk in r.iter_content(chunk_size=1024):
            data += chunk
            if b"data: 2" in data:
                break
        assert b"data: 0" in data
        assert b"data: 1" in data
        assert b"data: 2" in data


def test_http_still_works(server):
    r = requests.get(f"http://127.0.0.1:{PORT}/hello")
    assert r.status_code == 200
    assert r.json() == {"message": "hello"}


@pytest.mark.skipif(not WEBSOCKETS_AVAILABLE, reason="websockets not installed")
def test_websocket_echo(server):
    async def client():
        uri = f"ws://127.0.0.1:{PORT}/ws"
        async with websockets.connect(uri) as ws:
            await ws.send("ping")
            reply = await ws.recv()
            assert reply == "echo:ping"

    import asyncio

    asyncio.run(client())
