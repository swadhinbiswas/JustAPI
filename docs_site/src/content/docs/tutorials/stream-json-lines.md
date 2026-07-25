---
title: Stream JSON Lines
description: Stream validated JSON responses line-by-line in JustAPI with stream_json.
keywords: [JustAPI, stream JSON, SSE, streaming, NDJSON, validated streaming]
---

## Basic Streaming

Use `stream_json` to send validated JSON lines:

```python
from justapi import JustAPIApp, Schema
from typing import AsyncGenerator

class Token(Schema):
    text: str
    index: int

app = JustAPIApp()

@app.stream_json("/stream", schema=Token, mode="ndjson")
async def stream_tokens():
    for i, word in enumerate(["Hello", "world", "from", "JustAPI"]):
        yield Token(text=word, index=i)
```

Each line is validated against the schema before sending.

## Mode

| Mode | Format | Content-Type |
|------|--------|-------------|
| `"ndjson"` | One JSON object per line | `application/x-ndjson` |
| `"sse"` | Server-Sent Events format | `text/event-stream` |

## With Dependencies

```python
@app.stream_json(
    "/stream",
    schema=Token,
    dependencies=[Depends(get_current_user)],
)
async def stream_tokens(user: dict):
    for i, word in enumerate(["Hello", "world"]):
        yield Token(text=word, index=i)
```

## See Also

- [Streaming Output](/advanced/streaming-output/) — advanced streaming
- [Server-Sent Events](/tutorials/websockets-sse/) — SSE basics
- [Validated Streaming](/advanced/streaming-output/) — schema validation
