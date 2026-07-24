---
title: Background Tasks
description: Execute tasks after sending the response using BackgroundTasks.
---

Background tasks let you run code after the HTTP response has been sent, without blocking the client. This is useful for operations like sending emails, processing uploads, or triggering webhooks.

## Basic Usage

```python
from justapi import JustAPIApp, BackgroundTasks

app = JustAPIApp()


def write_log(message: str):
    with open("requests.log", "a") as f:
        f.write(f"{message}\n")


@app.post("/submit")
def submit(request, tasks: BackgroundTasks):
    tasks.add_task(write_log, f"New submission at {request.path}")
    return {"message": "Request accepted"}
```

The response is sent immediately, and `write_log` runs in the background.

## Multiple Background Tasks

```python
def send_email(to: str, subject: str):
    ...

def update_analytics(endpoint: str):
    ...

def notify_slack(channel: str, message: str):
    ...


@app.post("/order")
def create_order(request, tasks: BackgroundTasks):
    tasks.add_task(send_email, "user@example.com", "Order confirmed")
    tasks.add_task(update_analytics, "/order")
    tasks.add_task(notify_slack, "#orders", "New order placed")
    return {"message": "Order created"}
```

## Tasks with Return Values

Background tasks can't return values to the client (the response is already sent). Instead, store results in a database or queue:

```python
def process_file_async(file_path: str, db):
    try:
        result = expensive_processing(file_path)
        db.execute(
            "UPDATE files SET status = 'done', result = $1 WHERE path = $2",
            [result, file_path],
        )
    except Exception as e:
        db.execute(
            "UPDATE files SET status = 'failed', error = $1 WHERE path = $2",
            [str(e), file_path],
        )


@app.post("/upload")
def upload(request, tasks: BackgroundTasks):
    data = request.json()
    tasks.add_task(process_file_async, data["file_path"], app.db)
    return {"message": "Processing started", "status_url": "/status/file_123"}
```

## Tasks and the Event Loop

Background tasks run on the Tokio async runtime. They don't block the main request handler:

```python
import asyncio
from justapi import BackgroundTasks


async def send_welcome_email(email: str):
    await asyncio.sleep(1)  # Simulate async email sending
    print(f"Welcome email sent to {email}")


@app.post("/signup")
async def signup(request, tasks: BackgroundTasks):
    tasks.add_task(send_welcome_email, "user@example.com")
    return {"message": "Signed up successfully"}
```

## When to Use Background Tasks

| Use Case | Example |
|---|---|
| Email notifications | Welcome emails, password resets |
| Webhook delivery | Notify external services |
| Log aggregation | Write access logs to external service |
| Analytics | Track page views, events |
| File processing | Resize images after upload |
| Cache warming | Rebuild cache after data change |
| Webhook triggers | Notify external systems |

## How It Works Internally

1. `BackgroundTasks.add_task()` registers a callable with arguments
2. The handler executes and returns the response immediately
3. After the response is sent, background tasks execute in order
4. Tasks run on the Tokio runtime with the GIL managed by PyO3
5. If a task raises an exception, it's logged but doesn't affect the response

## Next Steps

- [Scheduler](/api-reference/scheduler/) — Cron-based periodic tasks
- [WebSockets & SSE](/tutorials/websockets-sse/) — Real-time client notifications
- [Streaming Output](/advanced/streaming-output/) — Stream task progress to clients
