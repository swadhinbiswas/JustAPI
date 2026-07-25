---
title: Background Task Patterns
description: Implement prioritized tasks, progress tracking, and error handling with JustAPI background tasks.
keywords: [JustAPI, background tasks, patterns, priority, progress, error handling]
---

## Basic Background Tasks

```python
from justapi import JustAPIApp, BackgroundTasks

app = JustAPIApp()

def process_data(task_id: str, data: dict):
    # Long-running processing
    import time
    time.sleep(5)
    # Save results
    print(f"Task {task_id} completed")

@app.post("/process")
def start_processing(data: dict, background_tasks: BackgroundTasks):
    task_id = "task-123"
    background_tasks.add_task(process_data, task_id, data)
    return {"task_id": task_id, "status": "started"}
```

## Progress Tracking

```python
import asyncio

task_progress = {}

def update_progress(task_id: str, progress: int, status: str = "running"):
    task_progress[task_id] = {"progress": progress, "status": status}

def long_task(task_id: str):
    for i in range(100):
        time.sleep(0.1)
        update_progress(task_id, i + 1)
    update_progress(task_id, 100, "completed")

@app.post("/tasks")
def create_task(background_tasks: BackgroundTasks):
    import uuid
    task_id = str(uuid.uuid4())
    update_progress(task_id, 0, "pending")
    background_tasks.add_task(long_task, task_id)
    return {"task_id": task_id}

@app.get("/tasks/{task_id}")
def get_task_progress(task_id: str):
    return task_progress.get(task_id, {"error": "task not found"})
```

## Error Handling in Tasks

```python
def safe_task(task_id: str):
    try:
        # Risky operation
        risky_operation()
        update_progress(task_id, 100, "completed")
    except Exception as e:
        update_progress(task_id, 0, f"failed: {str(e)}")

@app.post("/safe-tasks")
def create_safe_task(background_tasks: BackgroundTasks):
    task_id = "safe-task-1"
    background_tasks.add_task(safe_task, task_id)
    return {"task_id": task_id}
```

## See Also

- [Background Tasks](/tutorials/background-tasks/) — basic background tasks
- [Lifespan Events](/advanced/lifespan-events/) — startup/shutdown
- [Settings & Environment Variables](/advanced/settings/) — configuration
