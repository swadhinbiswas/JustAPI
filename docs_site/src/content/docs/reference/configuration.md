---
title: Configuration Reference
description: Environment variables, app settings, and server configuration options for JustAPI — a high-performance FastAPI alternative built in Rust.
keywords: configuration, environment variables, FastAPI alternative, Rust web framework, server settings
---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `RUST_LOG` | `info` | Log level (error, warn, info, debug, trace) |
| `OTEL_SERVICE_NAME` | `justapi` | Service name for OpenTelemetry |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | — | OTLP collector endpoint |
| `DATABASE_URL` | — | Database connection string |
| `REDIS_URL` | — | Redis connection string |
| `JWT_SECRET` | — | JWT signing secret |

## `JustAPIApp()` Constructor

```python
app = JustAPIApp(
    title="My API",           # OpenAPI title
    version="1.0.0",          # OpenAPI version
    description="...",        # OpenAPI description
)
```

## `app.run()` Parameters

```python
app.run(
    addr="127.0.0.1:8000",        # Bind address
    max_body_size=52428800,       # Max body size (50 MiB)
)
```

## Server CLI Flags

```bash
justapi serve --host 0.0.0.0 --port 8080 --workers 4 --timeout 30
```

| Flag | Env Variable | Default | Description |
|---|---|---|---|
| `--host` | `HOST` | `127.0.0.1` | Bind address |
| `--port` | `PORT` | `8000` | Bind port |
| `--workers` | — | CPU count | Worker processes |
| `--timeout` | — | `30` | Request timeout (seconds) |
| `--reload` | — | false | Hot-reload |
| `--unix` | — | — | Unix socket path |

## Feature Flags (Cargo)

```toml
# Cargo.toml features for justapi-core
[dependencies]
justapi-core = { features = [
    "tls",            # TLS support via rustls
    "ws",             # WebSocket support
    "compression",    # Response compression
    "opentelemetry",  # OpenTelemetry tracing
    "db",             # Database support
    "simd-json",      # SIMD-accelerated JSON
] }
```

## See Also

- [CLI Reference](/reference/cli/) — Command line options
- [Environment Variables](#) — Runtime configuration
- [Feature Flags](https://github.com/swadhinbiswas/JustAPI/blob/main/Cargo.toml)
