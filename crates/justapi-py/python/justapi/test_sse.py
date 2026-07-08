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
