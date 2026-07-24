---
title: Error Codes
description: JustAPI error code catalog and troubleshooting guide.
---

## CLI Error Codes

| Code | Error | Cause | Fix |
|---|---|---|---|
| `E001` | Invalid address | Address format is invalid | Use `host:port` format (e.g., `127.0.0.1:8000`) |
| `E002` | Port in use | Address already bound | Use a different port or stop the other process |
| `E003` | Config file not found | Configuration file missing | Create the config file or specify the correct path |
| `E004` | Database connection failed | Cannot connect to database | Verify DATABASE_URL and database server status |
| `E005` | Migration failed | Migration SQL error | Check migration SQL syntax and database state |
| `E006` | Worker start failed | Cannot start worker process | Check system resources and ulimits |
| `E007` | Hot-reload not available | File watcher initialization failed | Ensure `notify` library is installed |

## HTTP Status Codes

| Status | Description | Common Cause |
|---|---|---|
| 400 | Bad Request | Malformed request syntax |
| 401 | Unauthorized | Missing or invalid authentication |
| 403 | Forbidden | Insufficient permissions |
| 404 | Not Found | Route not registered |
| 413 | Payload Too Large | Body exceeds `max_body_size` |
| 422 | Unprocessable Entity | Request validation failure |
| 429 | Too Many Requests | Rate limit exceeded |
| 500 | Internal Server Error | Unhandled exception in handler |
| 503 | Service Unavailable | Database pool saturated |
| 504 | Gateway Timeout | Request exceeded timeout |

## Troubleshooting

### "ModuleNotFoundError: No module named '_justapi'"

The Rust extension wasn't built. Run:

```bash
maturin develop --release --manifest-path crates/justapi-py/Cargo.toml
```

### "SQLITE_CANTOPEN"

Database path resolution issue. Ensure the path is correct:

```python
app.set_database("sqlite:///app.db")    # Relative path (3 slashes)
app.set_database("sqlite:////tmp/db")  # Absolute path (4 slashes)
```

### "database pool saturated"

All connections are in use. Increase pool size or reduce concurrent load:

```python
app.set_database("...", max_connections=20, request_acquire_timeout=3.0)
```

## See Also

- [CLI Reference](/reference/cli/) — Command line documentation
- [Configuration Reference](/reference/configuration/) — Environment variables
- [Production Checklist](/deployment/production-checklist/) — Troubleshooting in production
