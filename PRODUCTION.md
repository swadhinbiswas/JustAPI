# PRODUCTION.md — Deploying justapi in production

This guide covers the operational, security, and observability settings needed
to run a justapi service in production. It assumes you have already built the
extension (`maturin develop` / `maturin build`) and have a working `JustAPIApp`.

> Rust-first: all networking, protocol parsing, routing, middleware, scheduling,
> and body limiting live in `justapi-core` (Rust). Python only provides
> application handlers and glue. Tuning below is therefore largely Rust-driven
> and exposed through the Python `JustAPIApp` API.

---

## 1. Build for production

```bash
# Release build of the Rust extension (optimized, panic=abort)
VIRTUAL_ENV=.venv .venv/bin/maturin develop --release --manifest-path crates/justapi-py/Cargo.toml
```

`Cargo.toml` sets `panic = "abort"` in `[profile.release]`. On an unexpected
panic the process aborts (no unwinding across the PyO3/GIL FFI, no UB). Run
under a supervisor that restarts the process (systemd, Kubernetes,
`supervisord`, `docker restart=always`).

---

## 2. Process supervision & crash-fast model

- **Do not** wrap request handling in `catch_unwind`. It is unsound at the GIL
  FFI boundary and costs a large throughput regression. (See `DECISIONS.md`
  ADR-053.)
- Treat the server as **crash-fast + restart**. A supervisor restarts it within
  milliseconds. Design handlers to be idempotent so a restart is safe.
- systemd unit sketch:

  ```ini
  [Service]
  ExecStart=/srv/app/venv/bin/python -m myapp
  Restart=always
  RestartSec=1
  # Cap restart storms
  StartLimitIntervalSec=10
  StartLimitBurst=5
  ```

- Kubernetes: set `restartPolicy: Always`, a `livenessProbe` against `/live`,
  and a `readinessProbe` against `/ready` (see §5).

---

## 3. TLS termination

justapi speaks plain HTTP/1.1 + HTTP/2 at the socket. Terminate TLS at an
edge proxy (recommended) **or** enable native TLS:

- **Edge proxy (recommended):** put `justapi` behind `nginx`, `Caddy`, `Envoy`,
  or a cloud LB. The proxy handles TLS, HTTP/2, and connection reuse; justapi
  binds to a private loopback/Unix socket. This keeps certificates out of the
  Rust process and lets you use standard cert tooling (ACME, secret stores).
- **Native TLS:** `Server::with_tls(config)` (Rust `tls` feature) is available
  if you must terminate in-process. Prefer the proxy pattern for rotation and
  OCSP/stapling.

Pass the real client address and scheme from the proxy via `X-Forwarded-For`
/ `X-Forwarded-Proto` handling in your middleware so logs and rate-limits see
the true peer.

---

## 4. Request body size — DoS hardening

Unbounded request bodies are a memory-exhaustion DoS vector. Every read path is
capped by a configurable limit (ADR-055):

```python
app.run("0.0.0.0:8000", max_body_size=1024 * 1024)   # 1 MiB cap
```

- Default: `50 * 1024 * 1024` (50 MiB).
- Bodies over the limit are rejected with **`413 Payload Too Large`** and
  `{"detail":"payload too large"}` — they are never buffered into memory.
- **Tune per deployment, not globally:** set a tight cap for JSON/API endpoints
  and a larger one only where big uploads are expected. Run separate services
  (or routes behind different listeners) if one size does not fit all.
- The Rust constant `DEFAULT_MAX_BODY_SIZE` is the fallback for the built-in
  `serve()` default-routes path.

---

## 5. Health, readiness & liveness

- `GET /health` — liveness. Returns `200 {"status":"ok"}` when the process is
  up. Use for Kubernetes `livenessProbe`.
- `GET /ready` — readiness. Returns `200` only when all registered health
  checks pass (DB pool, downstream deps). Returns `503` otherwise. Use for
  `readinessProbe` so traffic is withheld until dependencies are healthy.
- `GET /live` — lightweight liveness alias.
- Register custom checks:

  ```python
  app.register_health_check("postgres", lambda: db_ok())
  ```

- `GET /metrics` — Prometheus exposition in text format (real counters, not a
  stub). Scrape it with Prometheus; alert on `request` error rates, p99 latency,
  and `connection` saturation.

### Transactions

Use `with app.db.begin() as tx:` for multi-step flow that must be atomic — the
block commits on success, rolls back on any exception, and holds one connection
for the duration (keep it short):

```python
with app.db.begin() as tx:
    row = tx.query("UPDATE stock SET qty = qty - ? WHERE sku = ? AND qty >= ? RETURNING price_cents",
                   [qty, sku, qty])
    if not row:
        raise HTTPException(409, "insufficient stock")   # rolls back
    tx.execute("INSERT INTO orders (...) VALUES (...)", [...])
```

Rules that keep it fast and deadlock-free:

- Do the **write first** (`UPDATE ... WHERE ... RETURNING`) so conditional logic
  is atomic and SQLite takes the write lock immediately (avoids the
  read-to-write upgrade busy-error).
- For multi-row updates, touch rows in a **consistent order** (e.g. sort by
  primary key) to avoid cross-transaction deadlocks.
- An interactive transaction holds a pooled connection; a leaked one rolls back
  on GC but still reduces pool capacity until then.

### Database outages (verified behavior)

A database outage is **not** a crash and **not** a 500:

| Situation | Behavior |
|---|---|
| DB down at process start | Server **starts normally**; `/live` 200, `/ready` 503, DB requests 503. Logs one warning. |
| DB goes down while running | `/ready` 503; DB-backed requests return **503 + `Retry-After: 5`** after the acquire window (`request_acquire_timeout`, 3 s default) — no hang, no 500. |
| DB comes back | The pool reconnects **lazily on the next request**; `/ready` returns 200 and traffic resumes — **no restart required**. |
| Query still running when the acquire window expires | `DatabaseUnavailable` is raised; the request fails fast instead of queueing behind a dead server. |

Handlers can degrade gracefully:

```python
from justapi import DatabaseUnavailable

@app.get("/report")
def report():
    try:
        return {"rows": app.db.query("SELECT ...")}
    except DatabaseUnavailable:
        return {"cached": True}   # serve a cached/static view
```

Kubernetes users: rely on `readinessProbe: /ready` rather than a startup
probe that kills the pod — the process is healthy, its dependency is not.

---

## 6. Security headers (opt-in)

Secure-by-default means **no** injected headers until you ask. Enable them
explicitly:

```python
app.enable_secure_headers()
```

This adds `Strict-Transport-Security`, `X-Content-Type-Options`,
`X-Frame-Options`, `Content-Security-Policy`, and `Referrer-Policy` defaults.
CORS is also secure-by-default (no permissive `*` unless you configure it).

---

## 7. Observability

- **Tracing:** justapi emits `tracing` spans per request (`handler.execute`,
  with `http.status_code`). Pipe `tracing-subscriber` (env-filter / OTel) via
  your Python bootstrap to get distributed traces.
- **Metrics:** scrape `/metrics`. Key signals: request count by route/status,
  p50/p95/p99 latency, inbound/outbound bytes, open connections, and error
  ratios.
- **Logs:** structured logs via `tracing`; avoid logging request bodies in
  production (they may contain PII/secrets). The body cap (§4) bounds memory
  even if you do sample payloads.
- **Request IDs:** call `app.enable_request_id()` (default header
  `x-request-id`, or pass e.g. `X-Correlation-ID`). An incoming id is honoured
  (1-128 printable ASCII), otherwise a 128-bit hex id is generated. The id is
  visible to handlers as a request header, echoed on the response (including
  401/429/504s and CORS preflights), and appears in the structured access log
  as `http.request_id` — the join key between gateway logs and app logs:

  ```python
  app.enable_request_id()          # x-request-id
  app.enable_request_id("X-Correlation-ID")  # reuse your gateway's header
  ```

  ```json
  {"message":"request completed","http.method":"GET","http.path":"/orders",
   "http.request_id":"77491bde080d2b7b8f49aeaf87d78bfc","http.status_code":200}
  ```

  If OTel tracing is enabled, correlate `http.request_id` with the trace id.

---

## 8. Connection & timeout tuning

- Per-request timeout: requests exceeding the configured timeout return `504
  Gateway Timeout` (the handler is cancelled). Set it to bound worst-case tail
  latency.
- Max concurrent connections is semaphore-bounded; tune for your worker count
  and memory budget.
- Behind a proxy, keep proxy and justapi timeouts aligned (proxy idle timeout ≥
  justapi timeout) to avoid spurious 504s.

---

## 9. Graceful shutdown

`run()` installs `SIGINT`/`SIGTERM` handlers. On signal it stops accepting new
connections and drains in-flight requests before exiting, so a supervisor
restart loses no committed work. Ensure your supervisor sends `SIGTERM` (not
`SIGKILL`) for clean drains.

---

## 10. Minimal production bootstrap

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/healthz")
def healthz(request):
    return {"status": "ok"}

app.register_health_check("postgres", lambda: db_ok())
app.enable_secure_headers()

if __name__ == "__main__":
    # 1 MiB body cap, behind an nginx/LB doing TLS
    app.run("127.0.0.1:8000", max_body_size=1024 * 1024)
```

Front it with:

```nginx
server {
    listen 443 ssl;
    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        client_max_body_size 1m;   # match justapi's cap
    }
}
```

---

## 11. Pre-flight checklist

- [ ] Release build (`maturin develop --release`).
- [ ] Supervisor with `Restart=always` / `restartPolicy: Always`.
- [ ] TLS terminated at proxy (or native TLS configured).
- [ ] `max_body_size` set to the smallest value your endpoints allow.
- [ ] `enable_secure_headers()` called; CORS explicitly configured.
- [ ] `/live` + `/ready` wired to liveness/readiness probes.
- [ ] `/metrics` scraped; alerts on error ratio + p99.
- [ ] Request timeout set; proxy idle timeout ≥ justapi timeout.
- [ ] Graceful shutdown verified (SIGTERM drains cleanly).
- [ ] No request bodies logged in production.
- [ ] `enable_request_id()` enabled; gateway forwards/propagates the header.
- [ ] `/metrics` alert thresholds set for 5xx ratio and p99 (e.g. p99 > 2x
      the pre-deploy baseline for 10 min).

---

## 12. Production-readiness evidence (2026-10-30)

The claims below are backed by recorded runs, not estimates:

| Area | Evidence |
|---|---|
| Tests (GIL) | 317 Python tests (framework + 2 production examples), 312 core lib tests, 151 inference tests |
| Tests (free-threaded 3.14t) | 317 Python tests with `PYTHON_GIL=0` |
| Lint/supply chain | `cargo fmt --check`, `clippy -D warnings`, `cargo deny` (0 ignored advisories), Trivy + CycloneDX SBOM |
| Latency gate | p99 A/B vs main is a **hard CI gate** (≤5% and <0.2 ms absolute) since 2026-10-23 — §13 |
| Multi-worker | 4 prefork workers: 3.20–3.73× RPS, p99 down ~2/3 (BENCHMARKS.md) |
| Graceful shutdown | Worker SIGTERM drains in-flight requests and is respawned; supervisor SIGTERM stops all workers cleanly |
| Database correctness | SQLite + Postgres 16 (same app): parameter types pinned per value, WAL/busy_timeout, background tasks; **interactive transactions** (commit/rollback, multi-item atomicity) |
| Inventory/invariant proof | Order platform: 12 concurrent buyers / 5 units → exactly 5 orders (4 workers, SQLite **and** Postgres); idempotency storm → one order; zero negative stock; 1,127 transactional writes/s |
| Deployment | Docker two-stage image (non-root, healthcheck, `/data` volume) built + smoke-tested on every PR |
| Observability | Prometheus `/metrics`, JSON access logs with request IDs, `/health` + `/ready`, OTLP tracing |
| Recovery | crash-fast + supervisor restart model (§2); server-side panics abort without UB (§2); DB outages → 503 + `Retry-After`, lazy reconnect without restart |
| Supply chain | Actions pinned to full commit SHAs (enforced by a CI job), least-privilege `permissions:` per workflow/job, OIDC publishing, Dependabot for pins, `cargo deny` green |

### Benchmark gate policy

- p99 regression vs `main` >5% **and** ≥0.2 ms absolute fails CI
  (`p99-gate.yml`). The absolute floor absorbs shared-runner jitter on
  sub-millisecond baselines; real regressions on the ~2 ms Python path move
  well past it.
- Throughput numbers are trend-only across entries; compare like-for-like
  builds within one session (see BENCHMARKS.md methodology notes).
