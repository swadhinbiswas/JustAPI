---
title: WebSockets & Server-Sent Events
description: Build real-time applications with JustAPI WebSockets and SSE streaming — a FastAPI alternative powered by Rust.
keywords: WebSockets, Server-Sent Events, SSE, real-time, JustAPI, FastAPI alternative, streaming
---

JustAPI supports both WebSockets (full-duplex) and Server-Sent Events (server-to-client streaming) for real-time communication.

## WebSockets

### Echo Server

```python
from justapi import JustAPIApp

app = JustAPIApp()


@app.websocket("/ws")
async def echo(ws):
    await ws.accept()
    try:
        while True:
            data = await ws.receive_text()
            await ws.send_text(f"Echo: {data}")
    except Exception:
        print("Client disconnected")
```

### Test with a WebSocket Client

```bash
# Using websocat
websocat ws://127.0.0.1:8000/ws
> Hello
< Echo: Hello
```

Or from JavaScript:

```javascript
const ws = new WebSocket("ws://localhost:8000/ws");
ws.onmessage = (event) => console.log(event.data);
ws.send("Hello");
```

### WebSocket API Reference

| Method | Description |
|---|---|
| `await ws.accept()` | Accept the WebSocket upgrade |
| `await ws.receive_text()` | Receive a text message |
| `await ws.receive_bytes()` | Receive a binary message |
| `await ws.receive_json()` | Receive and parse a JSON message |
| `await ws.send_text(data)` | Send a text message |
| `await ws.send_bytes(data)` | Send a binary message |
| `await ws.send_json(data)` | Send a JSON message |
| `await ws.close()` | Close the connection |

### Broadcasting to Multiple Clients

```python
import asyncio

connected = set()


@app.websocket("/chat")
async def chat(ws):
    await ws.accept()
    connected.add(ws)
    try:
        while True:
            data = await ws.receive_text()
            # Broadcast to all connected clients
            for client in connected:
                if client != ws:
                    await client.send_text(data)
    finally:
        connected.discard(ws)
```

## Server-Sent Events (SSE)

SSE streams events from server to client over a single HTTP connection:

```python
import asyncio
from justapi.responses import StreamingResponse


@app.get("/events")
async def event_stream(request):
    async def generate():
        for i in range(10):
            yield f"data: Event #{i}\n\n"
            await asyncio.sleep(1)

    return StreamingResponse(generate(), media_type="text/event-stream")
```

```bash
curl -N http://127.0.0.1:8000/events
# Output:
# data: Event #0
# data: Event #1
# ...
```

### SSE with JSON Data

```python
import json
import asyncio
import random
from justapi.responses import StreamingResponse


@app.get("/stock-prices")
async def stock_prices(request):
    async def generate():
        while True:
            price = {"symbol": "JUST", "price": round(100 + random.random() * 10, 2)}
            yield f"data: {json.dumps(price)}\n\n"
            await asyncio.sleep(0.5)

    return StreamingResponse(generate(), media_type="text/event-stream")
```

## Choosing Between WebSockets and SSE

| Feature | WebSocket | SSE |
|---|---|---|
| Direction | Bidirectional | Server → Client only |
| Protocol | `ws://` / `wss://` | Standard HTTP |
| Message format | Text or binary | Text-only (usually JSON) |
| Auto-reconnect | Manual | Built-in (EventSource API) |
| Browser support | All modern browsers | All modern browsers |
| Use case | Chat, gaming, collaboration | Live updates, notifications, feeds |

## Next Steps

- [Background Tasks](/tutorials/background-tasks/) — Async tasks after response
- [Streaming Output](/advanced/streaming-output/) — Validated streaming with Rust
- [Agent System](/advanced/agent-system/) — Sessions and MCP tools
