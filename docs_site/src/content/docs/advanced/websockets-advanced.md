---
title: WebSockets Advanced
description: Advanced WebSocket patterns — connection management, broadcasting, and testing in JustAPI.
keywords: [JustAPI, WebSocket, advanced, broadcasting, connection management]
---

## WebSocket with Connection Manager

```python
from justapi import JustAPIApp
from justapi import WebSocket, WebSocketDisconnect

app = JustAPIApp()

class ConnectionManager:
    def __init__(self):
        self.connections: list[WebSocket] = []

    async def connect(self, websocket: WebSocket):
        await websocket.accept()
        self.connections.append(websocket)

    def disconnect(self, websocket: WebSocket):
        self.connections.remove(websocket)

    async def broadcast(self, message: str):
        for conn in self.connections:
            await conn.send_text(message)

manager = ConnectionManager()

@app.websocket("/ws")
async def websocket_endpoint(websocket: WebSocket):
    await manager.connect(websocket)
    try:
        while True:
            data = await websocket.receive_text()
            await manager.broadcast(f"Echo: {data}")
    except WebSocketDisconnect:
        manager.disconnect(websocket)
```

## Private WebSocket Rooms

```python
from collections import defaultdict

rooms: dict[str, list[WebSocket]] = defaultdict(list)

@app.websocket("/ws/{room}")
async def room_websocket(websocket: WebSocket, room: str):
    await websocket.accept()
    rooms[room].append(websocket)
    try:
        while True:
            data = await websocket.receive_text()
            for conn in rooms[room]:
                if conn != websocket:
                    await conn.send_text(data)
    except WebSocketDisconnect:
        rooms[room].remove(websocket)
```

## See Also

- [WebSockets & SSE](/tutorials/websockets-sse/) — basic WebSocket usage
- [Streaming Output](/advanced/streaming-output/) — SSE streaming
- [Resilience Patterns](/advanced/resilience-patterns/) — connection management
