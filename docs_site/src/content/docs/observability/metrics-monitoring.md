---
title: Metrics & Monitoring
description: Export Prometheus metrics and monitor your JustAPI application — a high-performance FastAPI alternative built in Rust.
keywords: Prometheus metrics, monitoring, FastAPI alternative, Rust web framework, observability
---

## Metrics Endpoint

Every JustAPI application exposes Prometheus-compatible metrics at `/metrics`:

```bash
curl http://localhost:8000/metrics
```

## Available Metrics

| Metric | Type | Description |
|---|---|---|
| `requests_total` | Counter | Total requests by route and status |
| `request_duration_seconds` | Histogram | Request latency (p50, p95, p99, p999) |
| `inbound_bytes_total` | Counter | Total bytes received |
| `outbound_bytes_total` | Counter | Total bytes sent |
| `connections_active` | Gauge | Currently active connections |
| `errors_total` | Counter | Error count by type |

## Prometheus Scrape Configuration

```yaml
scrape_configs:
  - job_name: 'justapi'
    static_configs:
      - targets: ['localhost:8000']
    metrics_path: /metrics
    scrape_interval: 15s
```

## Grafana Dashboard

Key panels to create:

1. **Request Rate** — `rate(requests_total[1m])` by route
2. **Latency** — `histogram_quantile(0.99, rate(request_duration_seconds_bucket[5m]))`
3. **Error Ratio** — `rate(errors_total[5m]) / rate(requests_total[5m])`
4. **Active Connections** — `connections_active`

## Alerting Rules

```yaml
groups:
  - name: justapi
    rules:
      - alert: HighErrorRate
        expr: rate(errors_total[5m]) / rate(requests_total[5m]) > 0.05
        for: 5m
        labels: { severity: critical }

      - alert: HighLatency
        expr: histogram_quantile(0.99, rate(request_duration_seconds_bucket[5m])) > 1
        for: 5m
        labels: { severity: warning }
```

## Built-in Alerting

JustAPI supports webhook-based alerting:

```python
app.enable_alerting(
    slack_webhook="https://hooks.slack.com/services/...",
    pagerduty_key="...",
)
```

## See Also

- [OpenTelemetry](/observability/opentelemetry/) — Distributed tracing
- [Structured Logging](/observability/structured-logging/) — Log configuration
- [Health Checks](/observability/health-checks/) — Health endpoints
