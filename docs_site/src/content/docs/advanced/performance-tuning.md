---
title: Performance Tuning
description: Optimize your JustAPI application for maximum throughput and minimum latency.
---

## Worker Configuration

### Multi-Worker Mode

```bash
justapi serve --host 0.0.0.0 --port 8080 --workers 4
```

Each worker is a separate OS process. Workers share no state — use Redis/Postgres for shared data.

### Auto-Scaling Workers

```bash
justapi serve --scale --min-workers 2 --max-workers 8 \
  --scale-high 1000 --scale-cooldown 30
```

| Flag | Default | Description |
|---|---|---|
| `--workers` | CPU count | Fixed number of workers |
| `--scale` | off | Enable load-based auto-scaling |
| `--min-workers` | 2 | Minimum workers |
| `--max-workers` | CPU count | Maximum workers |
| `--scale-low` | 100 | RPS threshold to scale down |
| `--scale-high` | 1000 | RPS threshold to scale up |
| `--scale-cooldown` | 30 | Seconds between scaling events |

## Connection Pool Tuning

```python
app.set_database(
    "postgres://localhost/mydb",
    max_connections=20,               # Pool size
    request_acquire_timeout=1.0,      # Timeout for pool saturation
)
```

- Match pool size to expected concurrency
- Set `request_acquire_timeout` low (1-3s) for fast failure under load
- Use connection pooling behind the app for higher concurrency

## Body Size Limits

```python
app.run("0.0.0.0:8000", max_body_size=1024 * 1024)  # 1 MiB
```

Tight limits prevent memory exhaustion under load. Default is 50 MiB.

## Request Timeouts

Set timeouts to bound worst-case latency:

```bash
justapi serve --timeout 30  # 30 second request timeout
```

Requests exceeding the timeout return **504 Gateway Timeout**.

## TLS Overhead

TLS adds approximately **10.7%** overhead vs. plain HTTP. For maximum performance:

1. Terminate TLS at the edge proxy (nginx, Caddy, cloud LB)
2. Run JustAPI on a private loopback or Unix socket
3. Pass `X-Forwarded-For` and `X-Forwarded-Proto` from the proxy

## Compression Trade-offs

| Algorithm | Compression Ratio | Speed |
|---|---|---|
| None | 1.0x | Fastest |
| gzip (level 1) | ~3x | Fast |
| gzip (level 9) | ~5x | Slow |
| brotli | ~4x | Medium |
| zstd | ~5x | Fast |

Enable compression only when bandwidth is the bottleneck:

```rust
// In Cargo.toml features
justapi-core = { features = ["compression"] }
```

## Route-Lookup Cache

JustAPI automatically caches route lookups for high-traffic paths. No configuration needed — the cache is always active.

## Native Fast Path

Maximum throughput comes from the native fast path:

```python
@app.post("/fast", body_schema=Schema, native=True)
def fast_handler(request):
    return {"status": "ok"}  # 724k+ RPS
```

See [Native Fast Path](/advanced/native-fast-path/) for details.

## K8s Overhead

When running in Kubernetes, expect approximately **8.4%** throughput degradation due to networking overhead. Mitigate by:

1. Using host networking mode
2. Setting resource requests/limits appropriately
3. Tuning kube-proxy to use `iptables` mode

## Benchmark Methodology

```bash
# Warm up: 5 seconds
oha -z 5s http://localhost:8080/hello

# Benchmark: 30 seconds, 100 concurrent connections
oha -z 30s -c 100 http://localhost:8080/hello

# Results: p50, p95, p99 latency, RPS, peak RSS
```

Run minimum 3 runs per workload to establish distribution.

## See Also

- [Native Fast Path](/advanced/native-fast-path/) — 724k+ RPS execution
- [Production Checklist](/deployment/production-checklist/) — Production deployment settings
- [Benchmarking Guide](/contributing/benchmarking-guide/) — Adding benchmarks
