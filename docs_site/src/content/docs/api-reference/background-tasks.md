---
title: Background Tasks API
description: "API reference for background tasks in JustAPI, the FastAPI alternative — post-response background task execution."
keywords: [background tasks, fastapi alternative, justapi, backgroundtasks, async tasks, post-response]
---

## `BackgroundTasks` Object

```python
from justapi import BackgroundTasks
```

| Method | Description |
|---|---|
| `tasks.add_task(func, *args, **kwargs)` | Register a function to run after the response |

### `add_task()`

| Parameter | Type | Description |
|---|---|---|
| `func` | callable | Function to execute in the background |
| `*args` | any | Positional arguments for the function |
| `**kwargs` | any | Keyword arguments for the function |

Tasks are executed in registration order after the response is sent. If a task raises an exception, it's logged but subsequent tasks still execute.

## Usage

```python
@app.post("/submit")
def submit(request, tasks: BackgroundTasks):
    tasks.add_task(send_email, "user@example.com", subject="Welcome")
    return {"message": "Submitted"}
```

## Async Tasks

```python
import asyncio

async def process_async(data: dict):
    await asyncio.sleep(1)
    print(f"Processed: {data}")

@app.post("/process")
async def process(request, tasks: BackgroundTasks):
    tasks.add_task(process_async, {"id": 1})
    return {"status": "processing"}
```

## See Also

- [Background Tasks Tutorial](/tutorials/background-tasks/) — Usage patterns
- [Scheduler API](/api-reference/scheduler/) — Cron-based periodic tasks
- [JustAPIApp](/api-reference/justapiapp/) — App configuration
