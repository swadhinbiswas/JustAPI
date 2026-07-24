---
title: Scheduler API
description: Reference for the Rust-native cron-based task scheduler.
---

## `PyScheduler` Object

The scheduler manages periodic tasks using cron expressions. It runs entirely in Rust with UTC-based timing.

```python
from justapi import PyScheduler

scheduler = PyScheduler()
```

| Method | Description |
|---|---|
| `scheduler.cron(expression, callback)` | Schedule task with cron expression |
| `scheduler.interval(seconds, callback)` | Schedule task at fixed interval |
| `scheduler.once(delay_seconds, callback)` | Schedule one-time delayed task |

### `scheduler.cron()`

```python
# Run every hour at minute 0
scheduler.cron("0 * * * *", my_task)
```

| Parameter | Type | Description |
|---|---|---|
| `expression` | `str` | Standard 5-field cron expression (min hour day month weekday) |
| `callback` | callable | Function to execute |

### `scheduler.interval()`

```python
# Run every 30 seconds
scheduler.interval(30, my_task)
```

| Parameter | Type | Description |
|---|---|---|
| `seconds` | `int` | Interval in seconds |
| `callback` | callable | Function to execute |

### `scheduler.once()`

```python
# Run once after 60 seconds
scheduler.once(60, my_task)
```

| Parameter | Type | Description |
|---|---|---|
| `delay_seconds` | `int` | Delay before execution |
| `callback` | callable | Function to execute |

## Integration with App

```python
from justapi import JustAPIApp, PyScheduler

app = JustAPIApp()
scheduler = PyScheduler()

def cleanup_expired_sessions():
    app.db.execute("DELETE FROM sessions WHERE expires_at < NOW()")

scheduler.cron("0 */6 * * *", cleanup_expired_sessions)  # Every 6 hours
```

## See Also

- [Background Tasks](/api-reference/background-tasks/) — Per-request background tasks
- [JustAPIApp](/api-reference/justapiapp/) — App configuration
