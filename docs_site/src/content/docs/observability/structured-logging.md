---
title: Structured Logging
description: Configure structured JSON logging with tracing-subscriber for JustAPI — a high-performance FastAPI alternative built in Rust.
keywords: structured logging, JSON logging, tracing-subscriber, FastAPI alternative, Rust web framework
---

JustAPI uses `tracing` for structured logging, supporting both human-readable text and machine-parseable JSON formats.

## Default Logging

By default, JustAPI logs to stdout in a human-readable format:

```bash
RUST_LOG=info python app.py
```

## JSON Log Format

Enable JSON logging for production log aggregation:

```python
from justapi.logging import init_json_logging

init_json_logging()
```

JSON output:

```json
{"timestamp":"2026-07-24T15:00:00Z","level":"INFO","message":"Server started","addr":"127.0.0.1:8000"}
{"timestamp":"2026-07-24T15:00:01Z","level":"INFO","message":"Request completed","method":"GET","path":"/items/42","status":200,"duration_ms":1.2}
```

## Log Levels

Set via `RUST_LOG` environment variable:

| Level | Description |
|---|---|
| `error` | Only errors |
| `warn` | Warnings and errors |
| `info` | Normal operational messages (default) |
| `debug` | Detailed debugging |
| `trace` | Very detailed, high-frequency events |

```bash
RUST_LOG=warn python app.py     # Less verbose
RUST_LOG=debug python app.py    # More verbose
```

## Production Guidelines

- Set `RUST_LOG=info` in production
- Do **not** log request bodies (may contain PII/secrets)
- Use JSON format for log aggregation (ELK, Loki, Datadog, etc.)
- Configure log sampling for high-traffic endpoints

## See Also

- [Metrics & Monitoring](/observability/metrics-monitoring/) — Prometheus metrics
- [OpenTelemetry](/observability/opentelemetry/) — Distributed tracing
- [Health Checks](/observability/health-checks/) — Health endpoints
