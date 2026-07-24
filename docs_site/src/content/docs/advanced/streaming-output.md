---
title: Streaming Output
description: Stream responses with Rust-side validation using @app.stream_json and StreamingResponse in JustAPI, the FastAPI alternative.
keywords: JustAPI, FastAPI alternative, streaming, stream_json, StreamingResponse, Rust validation
---

JustAPI supports two streaming approaches: the validated `@app.stream_json` and the flexible `StreamingResponse`.

## `@app.stream_json` (Validated Streaming)

Every yielded item is validated against a JSON Schema **in Rust** before being written to the socket. Invalid items abort the stream.

### NDJSON Mode

```python
from justapi import JustAPIApp

app = JustAPIApp()

SCHEMA = {
    "type": "object",
    "properties": {
        "step": {"type": "string"},
        "value": {"type": "number"},
    },
    "required": ["step", "value"],
}


@app.stream_json("/stream", schema=SCHEMA, mode="ndjson")
def stream(request):
    for i in range(10):
        yield {"step": f"item-{i}", "value": i}
```

Output:

```
{"step":"item-0","value":0}
{"step":"item-1","value":1}
...
```

### Array Mode

```python
@app.stream_json("/array", schema=SCHEMA, mode="array")
def stream_array(request):
    for i in range(10):
        yield {"step": f"item-{i}", "value": i}
```

Output:

```json
[{"step":"item-0","value":0},{"step":"item-1","value":1},...]
```

### Validation

If an item doesn't match the schema, the stream is **aborted immediately** and the client receives an error. This guarantees clients never see partial or invalid data.

## `StreamingResponse` (Flexible Streaming)

For custom streaming needs:

```python
import asyncio
from justapi.responses import StreamingResponse


@app.get("/events")
async def events(request):
    async def generate():
        for i in range(10):
            yield f"data: Event #{i}\n\n"
            await asyncio.sleep(1)

    return StreamingResponse(generate(), media_type="text/event-stream")
```

## SSE (Server-Sent Events)

The `text/event-stream` content type enables browser EventSource API:

```javascript
const source = new EventSource("/events");
source.onmessage = (event) => console.log(event.data);
```

## Large File Streaming

```python
import aiofiles
from justapi.responses import StreamingResponse


@app.get("/large-file")
async def download(request):
    async def file_chunks():
        async with aiofiles.open("data.csv", "rb") as f:
            while chunk := await f.read(65536):
                yield chunk

    return StreamingResponse(file_chunks(), media_type="text/csv")
```

## Performance

Streaming responses use Rust's `StreamBody` with zero-copy chunks. Overhead is minimal — measured at <0.05 ms per chunk.

## See Also

- [Agent System](/advanced/agent-system/) — Streaming with sessions and tools
- [WebSockets & SSE Tutorial](/tutorials/websockets-sse/) — Real-time communication
- [Response Classes API](/api-reference/responses/) — Response reference
