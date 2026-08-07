import asyncio
import pytest
from justapi import JustAPIApp, TokenStreamResponse, JustAPITestClient

@pytest.mark.asyncio
async def test_sse_streaming():
    app = JustAPIApp()

    @app.get("/stream")
    async def stream():
        async def gen():
            for i in range(5):
                yield f"data: {i}\n\n"
                # tiny sleep to yield to event loop
                await asyncio.sleep(0.01)
        return TokenStreamResponse(gen())

    client = JustAPITestClient(app)
    response = client.get("/stream")
    assert response["status"] == 200
    
    body = bytes(response["body"]).decode("utf-8")
    print("Response body:", repr(body))
    for i in range(5):
        assert f"data: {i}\n\n" in body


def test_sse_native_rust_stream():
    """Rust-native SSE stream (ADR-088): events generated entirely in Rust,
    no Python handler — must work on the real server AND test client."""
    from justapi import JustAPIApp, JustAPITestClient

    app = JustAPIApp()
    app.sse_native("/events", count=3, interval_ms=0)

    r = JustAPITestClient(app).get("/events")
    assert r["status"] == 200
    body = bytes(r["body"]).decode()
    assert 'data: {"n":1}' in body
    assert 'data: {"n":3}' in body
    # content-type is event-stream
    assert r["headers"].get("content-type", "").startswith("text/event-stream")
