---
title: System Routes
description: Built-in introspection routes in JustAPI — help, tools, OpenAPI, health checks.
keywords: [JustAPI, system routes, health check, help, introspection, tools]
---

## Built-in Routes

JustAPI registers these routes automatically:

| Route | Method | Auth | Description |
|-------|--------|------|-------------|
| `/health` | GET | No | Health check (always returns 200) |
| `/live` | GET | No | Liveness probe |
| `/ready` | GET | No | Readiness probe (checks registered health checks) |
| `/metrics` | GET | Yes | Prometheus metrics |
| `/openapi.json` | GET | Yes | OpenAPI 3.1 schema |
| `/docs` | GET | Yes | Swagger UI |
| `/redoc` | GET | Yes | ReDoc |
| `/scalar` | GET | Yes | Scalar API Reference |

## Enable System Routes

```python
app = JustAPIApp(admin_token="my-secret-token")
app.enable_system_routes()
```

This mounts additional introspection routes:

| Route | Description |
|-------|-------------|
| `/_system/help` | Rich route descriptors (AI-friendly) |
| `/_system/help/{name}` | Detailed help for one route |
| `/_system/tools` | List registered MCP tools |
| `/_system/tools/call` | Invoke a tool by name |

## Register Health Checks

```python
def check_db():
    db = app.db
    if db.health():
        return True
    raise Exception("Database unreachable")

def check_redis():
    # Check Redis connection
    return True

app.register_health_check("database", check_db)
app.register_health_check("redis", check_redis)
```

`GET /ready` returns 200 only if all health checks pass.

## Admin Token Protection

Routes like `/metrics`, `/openapi.json`, `/docs` are protected by `admin_token`:

```python
app = JustAPIApp(admin_token="my-secret-token")
```

Include the token in requests:

```bash
curl -H "Authorization: Bearer my-secret-token" http://localhost:8000/metrics
```

## See Also

- [Health Checks](/observability/health-checks/) — monitoring
- [Metrics & Monitoring](/observability/metrics-monitoring/) — Prometheus
- [Configuration Reference](/reference/configuration/) — all options
