---
title: Scheduler API
description: "API reference for the task scheduler in JustAPI, the FastAPI alternative — Rust-native cron-based periodic task scheduler."
keywords: [scheduler, fastapi alternative, justapi, cron, periodic tasks, rust native]
---

## Scheduler

JustAPI's scheduler manages periodic tasks using cron expressions and fixed
intervals. It runs entirely in Rust with UTC-based timing, and jobs are
dispatched onto the same Rust background-task worker pool as
`BackgroundTasks` — so they run with the GIL released.

There are two ways to use it: the standalone `Scheduler` class, or the
convenience methods on `JustAPIApp`.

## Standalone `Scheduler`

```python
from justapi import Scheduler

scheduler = Scheduler()
scheduler.schedule("*/5 * * * *", my_task)  # cron expression
scheduler.every(30, poll_upstream)          # fixed interval
scheduler.start()
```

| Method | Description |
|---|---|
| `schedule(cron_expr, func, *args, **kwargs)` | Schedule a job with a standard 5-field cron expression |
| `every(seconds, func, *args, **kwargs)` | Schedule a job at a fixed interval |
| `remove(job)` | Remove a scheduled job |
| `jobs` | List registered jobs |
| `stats` | Job run statistics |
| `start()` | Start the scheduler (jobs run for the process lifetime) |
| `stop()` | Stop the scheduler |

### `schedule()` — cron jobs

```python
# Run every hour at minute 0
scheduler.schedule("0 * * * *", my_task)

# With arguments
scheduler.schedule("0 0 * * *", daily_report, "team@example.com")
```

| Parameter | Type | Description |
|---|---|---|
| `cron_expr` | `str` | Standard 5-field cron expression (min hour day month weekday), evaluated in UTC |
| `func` | callable | Function to execute |
| `*args` / `**kwargs` | — | Passed through to `func` |

### `every()` — interval jobs

```python
# Run every 30 seconds (first fire one interval after registration)
scheduler.every(30, my_task)
```

| Parameter | Type | Description |
|---|---|---|
| `seconds` | int | Interval in seconds |
| `func` | callable | Function to execute |
| `*args` / `**kwargs` | — | Passed through to `func` |

> **Note:** jobs are in-memory (not persisted) in the current version. The
> scheduler runs for the lifetime of the process.

## Integration with App

The `JustAPIApp` methods `schedule()` and `every()` register jobs directly and
start the scheduler automatically when the server starts:

```python
from justapi import JustAPIApp

app = JustAPIApp()

def cleanup_expired_sessions():
    app.db.execute("DELETE FROM sessions WHERE expires_at < ?", ["now"])

app.schedule("0 */6 * * *", cleanup_expired_sessions)  # Every 6 hours
app.every(30, poll_upstream)                           # Every 30 seconds

app.run("127.0.0.1:8000")
```

## See Also

- [Background Tasks](/api-reference/background-tasks/) — Per-request background tasks
- [JustAPIApp](/api-reference/justapiapp/) — App configuration
