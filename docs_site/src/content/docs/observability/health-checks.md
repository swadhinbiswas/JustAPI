---
title: Health Checks
description: Liveness, readiness, and custom health check endpoints for JustAPI — a high-performance FastAPI alternative built in Rust.
keywords: health checks, liveness probe, readiness probe, FastAPI alternative, Rust web framework
---

## Built-in Endpoints

Every JustAPI application exposes three health endpoints by default:

| Endpoint | Type | Returns | Use |
|---|---|---|---|
| `GET /health` | Liveness | `200 {"status":"ok"}` | Process alive check |
| `GET /live` | Liveness | `200 {"status":"ok"}` | Lightweight alias |
| `GET /ready` | Readiness | `200` or `503` | Dependency health |

## Readiness Probe

`/ready` returns `503` if any registered health check fails:

```bash
curl http://localhost:8000/ready
# When healthy: {"status":"ok","checks":{"postgres":"healthy"}}
# When unhealthy: {"status":"unavailable","checks":{"postgres":"unhealthy"}}
```

## Custom Health Checks

```python
app.register_health_check("postgres", lambda: check_db_connection())
app.register_health_check("redis", lambda: check_redis_connection())
app.register_health_check("external_api", lambda: ping_external_api())
```

Each check function should return `True` (healthy) or raise an exception.

## Kubernetes Probe Configuration

```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 15

readinessProbe:
  httpGet:
    path: /ready
    port: 8080
  initialDelaySeconds: 10
  periodSeconds: 10
  failureThreshold: 3
```

## See Also

- [Metrics & Monitoring](/observability/metrics-monitoring/) — Prometheus metrics
- [OpenTelemetry](/observability/opentelemetry/) — Distributed tracing
- [Production Checklist](/deployment/production-checklist/) — Production setup
