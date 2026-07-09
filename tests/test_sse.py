import asyncio
from justapi import JustAPIApp, TokenStreamResponse

app = JustAPIApp()

@app.get("/stream")
async def stream():
    async def gen():
        for i in range(5):
            yield f"data: {i}\n\n"
            await asyncio.sleep(0.1)
    return TokenStreamResponse(gen())
