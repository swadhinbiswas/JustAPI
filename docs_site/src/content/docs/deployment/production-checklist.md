---
title: Production Checklist
description: Essential settings and configuration for running JustAPI in production.
---

## 1. Build for Production

```bash
# Release build of the Rust extension
maturin develop --release --manifest-path crates/justapi-py/Cargo.toml
```

`Cargo.toml` sets `panic = "abort"` in release mode — panics safely abort the process (no UB across PyO3 FFI).

## 2. Process Supervision

Run under a supervisor that restarts the process:

```ini
# systemd unit
[Service]
ExecStart=/srv/app/venv/bin/python -m myapp
Restart=always
RestartSec=1
StartLimitIntervalSec=10
StartLimitBurst=5
```

For Kubernetes, set `restartPolicy: Always` with liveness and readiness probes.

## 3. TLS Termination

**Recommended:** Terminate TLS at an edge proxy (nginx, Caddy, cloud LB):

```nginx
server {
    listen 443 ssl;
    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        client_max_body_size 1m;
    }
}
```

## 4. Request Body Size

Set the smallest value your endpoints allow:

```python
app.run("0.0.0.0:8000", max_body_size=1024 * 1024)  # 1 MiB
```

Bodies over the limit receive **413 Payload Too Large**.

## 5. Security Headers

```python
app.enable_secure_headers(with_hsts=True)
app.add_cors(
    allow_origins=["https://yourdomain.com"],
    allow_methods=["GET", "POST"],
)
```

CORS is deny-by-default — no permissive `*` unless you configure it.

## 6. Health Checks

```python
app.register_health_check("postgres", lambda: db_ok())

# Endpoints:
# GET /health  → liveness probe (200 = OK)
# GET /ready   → readiness probe (200 = all deps healthy)
# GET /live    → lightweight liveness alias
```

## 7. Metrics

```bash
# Scrape /metrics with Prometheus
curl http://localhost:8000/metrics
```

Key signals: request count by route/status, p50/p95/p99 latency, error ratios.

## 8. Timeouts

```bash
justapi serve --timeout 30  # Request timeout in seconds
```

Keep proxy and JustAPI timeouts aligned to avoid spurious 504s.

## 9. Graceful Shutdown

`run()` installs SIGINT/SIGTERM handlers. On signal, it:
1. Stops accepting new connections
2. Drains in-flight requests
3. Calls plugin shutdown hooks
4. Exits cleanly

Ensure your supervisor sends **SIGTERM** (not SIGKILL) for clean drains.

## 10. Pre-Flight Checklist

- [ ] Release build (`maturin develop --release`)
- [ ] Supervisor with `Restart=always` / `restartPolicy: Always`
- [ ] TLS terminated at proxy (or native TLS configured)
- [ ] `max_body_size` set to the smallest value your endpoints allow
- [ ] `enable_secure_headers()` called; CORS explicitly configured
- [ ] `/live` + `/ready` wired to liveness/readiness probes
- [ ] `/metrics` scraped; alerts on error ratio + p99
- [ ] Request timeout set; proxy idle timeout ≥ JustAPI timeout
- [ ] Graceful shutdown verified (SIGTERM drains cleanly)
- [ ] No request bodies logged in production

## See Also

- [Docker](/deployment/docker/) — Container deployment
- [Kubernetes / Helm](/deployment/kubernetes-helm/) — K8s deployment
- [Performance Tuning](/advanced/performance-tuning/) — Optimize your app
