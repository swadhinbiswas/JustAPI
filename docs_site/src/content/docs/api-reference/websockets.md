---
title: WebSockets API
description: "API reference for WebSocket support in JustAPI, the FastAPI alternative — handler registration and connection management."
keywords: [websockets, fastapi alternative, justapi, websocket handler, real-time, bidirectional]
---

## `@app.websocket()`

Register a WebSocket handler:

```python
@app.websocket(path="/ws")
async def handler(ws):
    await ws.accept()
    ...
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `path` | `str` | required | WebSocket endpoint path |

## `WebSocket` Object

| Method | Description |
|---|---|
| `await ws.accept()` | Accept the WebSocket upgrade |
| `await ws.receive_text()` | Receive a text message (`str`) |
| `await ws.receive_bytes()` | Receive a binary message (`bytes`) |
| `await ws.receive_json()` | Receive and parse JSON message |
| `await ws.send_text(data)` | Send a text message |
| `await ws.send_bytes(data)` | Send a binary message |
| `await ws.send_json(data)` | Send JSON message |
| `await ws.close()` | Close the connection |

## `WebSocketState`

| State | Description |
|---|---|
| `CONNECTING` | Initial state, waiting for upgrade |
| `OPEN` | Connection established, messages can be exchanged |
| `CLOSING` | Close frame sent/received |
| `CLOSED` | Connection closed |

## Example

```python
@app.websocket("/chat")
async def chat(ws):
    await ws.accept()
    try:
        while True:
            msg = await ws.receive_text()
            await ws.send_text(f"Echo: {msg}")
    except Exception:
        pass
    finally:
        await ws.close()
```

## See Also

- [WebSockets & SSE Tutorial](/tutorials/websockets-sse/) — Usage guide with broadcasting
- [JustAPIApp](/api-reference/justapiapp/) — App configuration
