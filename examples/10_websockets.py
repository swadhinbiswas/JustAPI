import asyncio
from justapi import JustAPIApp

app = JustAPIApp()

@app.websocket("/ws")
async def websocket_endpoint(ws):
    """
    A simple WebSocket echo server.
    """
    await ws.accept()
    try:
        while True:
            data = await ws.receive_text()
            await ws.send_text(f"Message text was: {data}")
    except Exception as e:
        print("Client disconnected:", e)

if __name__ == "__main__":
    app.run("127.0.0.1:8000")
