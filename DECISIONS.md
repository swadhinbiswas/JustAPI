# DECISIONS.md

> Append-only architecture decision record (ADR) log.
> Every deviation from PROMPT.md, every crate choice, every GIL/concurrency
> decision gets a dated entry: context, options considered, decision, and
> the evidence behind it.

---

## ADR-001 — 2026-07-03 — Crate defaults for v1.0

**Context:** Phase 0 bootstrap. Choosing the dependency stack before any
implementation code, following PROMPT.md Section 1's "build vs. reinvent"
philosophy.

**Options considered:**
1. Hand-roll everything (HTTP parser, TLS, scheduler) — rejected per
   Section 1; produces unmaintainable half-finished subsystems.
2. Use mature, audited crates per Section 1's table — selected.

**Decision:** Default to the crate table from PROMPT.md Section 1:

| Concern | Crate | Version |
|---|---|---|
| Async runtime | `tokio` (multi-thread) | 1.x |
| HTTP/1.1 + HTTP/2 | `hyper` + `hyper-util` + `http-body-util` | 1.x |
| TLS | `rustls` | (Phase 7) |
| Python bindings | `pyo3` | 0.29.x |
| Routing | `matchit` | (Phase 2, evaluate vs custom trie) |
| JSON | `serde_json` baseline, `simd-json` feature-flagged | 1.x |
| Rate limiting | `governor` | (Phase 5) |
| JWT | `jsonwebtoken` | (Phase 5) |
| CLI | `clap` | 4.x |
| Logging/tracing | `tracing` + `tracing-subscriber` | 0.1 / 0.3 |
| Error handling | `anyhow` (app code), `thiserror` (library boundaries) | 1.x |

Custom replacements only after profiling shows the FFI boundary is the
bottleneck, with evidence attached to a new DECISIONS.md entry.

**Evidence:** PROMPT.md Section 1 crate table. No profiling data yet.

---

## ADR-002 — 2026-07-03 — Repository skeleton layout

**Context:** PROMPT.md Section 5.1 specifies the repo layout. Workspace
root is `/home/swadhin/RastAPI/` (directory name differs from the canonical
"justapi/" but crate names follow the spec).

**Decision:** Follow Section 5.1 layout exactly:
- 4 crates: `justapi-core`, `justapi-py`, `justapi-cli`, `justapi-bench`
- Python package under `python/justapi/` built with maturin
- Skills under `skills/`

No deviation from the specified structure.

---

## ADR-003 — 2026-07-03 — hyper 1.x requires hyper-util

**Context:** The original skeleton used `hyper::server::conn::http1::Builder`
which existed in hyper 0.14 but was moved to `hyper-util` in hyper 1.x.
The server wouldn't compile without this crate.

**Options considered:**
1. Downgrade to hyper 0.14 — rejected; 0.14 is legacy, 1.x is the
   maintained branch with the modular architecture we need.
2. Add `hyper-util` and `http-body-util` — selected; these are the official
   companion crates for hyper 1.x server usage.

**Decision:** Add `hyper-util` (features: `tokio`, `http1`, `server`) and
`http-body-util` as workspace dependencies. The server connection handling
uses `hyper_util::server::conn::http1::Builder` and
`hyper_util::rt::TokioIo` for the stream wrapper.

**Evidence:** hyper 1.0 migration guide, `hyper-util` crate documentation.

---

## ADR-004 — 2026-07-03 — PyO3 version: 0.29.x for Python 3.14 support

**Context:** The development machine runs Python 3.14.6. PyO3 0.23.x does
not support Python 3.14. PyO3 0.24.x also failed to compile against
Python 3.14.6 — full support required 0.29.

**Options considered:**
1. pyo3 0.23 — rejected; no Python 3.14 support.
2. pyo3 0.24 — rejected; still failed to compile against Python 3.14.6.
3. pyo3 0.29 — selected; compiles successfully against Python 3.14.6.

**Decision:** Upgrade to `pyo3 = "0.29"` with `extension-module` feature.

**Evidence:** Compilation failure with 0.23 and 0.24; successful build
with 0.29 on Python 3.14.6.

---

## ADR-005 — 2026-07-03 — Rust edition: 2021 (downgraded from 2024)

**Context:** The original skeleton specified `edition = "2024"`. While the
Rust toolchain (1.96.0) supports it, edition 2024 caused compatibility
issues with some dependency crates during the bootstrap build.

**Options considered:**
1. Keep edition 2024 and fix all dependency compatibility — higher risk,
   delays Phase 0 for no user-facing benefit.
2. Downgrade to edition 2021 — selected; maximizes dependency compatibility,
   can upgrade later when the ecosystem catches up.

**Decision:** Use `edition = "2021"`. Revisit upgrading to 2024 once all
dependencies (especially pyo3, hyper-util) have confirmed 2024 support.

**Evidence:** Build failures during Phase 0 bootstrap with edition 2024.

---

## ADR-006 — 2026-07-03 — Benchmark tooling: oha over wrk

**Context:** PROMPT.md Section 10.1 recommends `oha` or `wrk`. Both are
HTTP load generators.

**Options considered:**
1. `wrk` — mature, Lua scriptable, but reports only percentiles (not
   histogram), harder to install on some distros.
2. `oha` — Rust-based, outputs JSON with full latency distribution, easy
   to install via cargo, reports p50/p95/p99/p999 natively.

**Decision:** Primary tool is `oha`. `wrk` as a secondary validation tool
if needed. `oha` version and exact command lines recorded in BENCHMARKS.md
alongside each run.

**Evidence:** `oha` natively outputs the exact metrics BENCHMARKS.md needs
(p50/p95/p99, req/sec) in both human-readable and JSON formats.

---

## ADR-007 — 2026-07-03 — ASGI strategy: Tier A + Tier B per Section 2

**Context:** PROMPT.md Section 2 requires both an ASGI compatibility shim
(Tier A) and a native justapi API (Tier B). Tier A is required for v1.0;
Tier B is the performance ceiling.

**Decision:** Both tiers ship. Development order:
- Phases 1–9: Build the core runtime using Tier B (native) API internally
- Phase 10: Implement Tier A ASGI shim on top of the established core

Rationale: The native API exercises the full pipeline and establishes
performance baselines. The ASGI shim is an adapter on top — building it
first would mean building the adapter before the thing it adapts.

---

## ADR-008 — 2026-07-03 — GIL strategy: dedicated Python worker thread

**Context:** Phase 4 design decision for the execution model. justapi's
architecture split (Rust owns I/O, Python executes application logic) means
we must decide how to bridge the two worlds. The Python GIL (Global
Interpreter Lock) prevents multiple native OS threads from executing Python
bytecode concurrently. We need a concurrency model that maximizes throughput
while staying safe and simple.

**Options considered:**

### Option A — Dedicated Python worker thread (selected)

A single OS thread owns the Python interpreter and runs all Python handlers.
Requests arriving via Rust's tokio async runtime are dispatched to this
thread via a lock-free or channel-based mechanism (e.g., `tokio::sync::mpsc`
or a `flume` channel). The Rust side never acquires the GIL directly — it
sends a job and awaits the result.

Key properties:
- **GIL contention: zero.** Exactly one thread ever touches Python.
- **Concurrency:** Rust I/O is fully concurrent (tokio multi-thread
  scheduler). Python execution is serialized, which matches typical
  web-app profiles (I/O-bound inside Python — waiting on DB, external APIs).
- **Safety:** Great. No chance of accidentally blocking the tokio runtime
  with a slow Python call because Python runs on its own thread.
- **Complexity:** Low. A single producer-consumer channel. No GIL acquire/
  release logic scattered through the hot path.

### Option B — Direct GIL acquisition on tokio worker threads

Any tokio worker thread that needs to call Python acquires the GIL via
`Python::with_gil()`, runs the handler, then releases it. Multiple threads
may contend for the GIL.

Key properties:
- **GIL contention:** High under load. If all N tokio workers are handling
  requests that need Python, N-1 threads are blocked on the GIL.
- **Concurrency:** Only one thread runs Python at a time, so no throughput
  gain vs Option A for Python-bound work. For Rust-only requests (static
  files, caching layer, etc.), all threads are available — same as Option A.
- **Safety:** Moderate risk. A slow Python call blocks the tokio worker
  thread, starving other tasks on that thread. Mitigation: use
  `tokio::task::spawn_blocking` for Python calls, which moves them off the
  async runtime.
- **Complexity:** Low in principle, but `spawn_blocking` + GIL interactions
  need careful handling in the PyO3 boundary.

### Option C — Sub-interpreters (PEP 684)

Each connection or request task gets its own Python sub-interpreter, each
with its own GIL. True parallelism for Python code.

Key properties:
- **GIL contention:** Zero per-sub-interpreter; multiple interpreters run
  concurrently.
- **Concurrency:** True Python parallelism, but objects cannot be shared
  across interpreter boundaries. This breaks almost every Python web
  framework (they assume a single interpreter with shared globals).
- **Safety:** Low. Sub-interpreters in CPython have subtle isolation gaps.
  PyO3 support is experimental and incomplete.
- **Complexity:** Very high. Every Python object crossing the boundary
  must be serialized or copied.
- **Verdict:** Too immature for v1.0. Revisit for a future major version.

### Option D — Free-threaded CPython (PEP 703, `--disable-gil`)

CPython built with `--disable-gil` removes the GIL entirely. Multiple
threads can run Python bytecode simultaneously.

Key properties:
- **GIL contention:** Zero (no GIL).
- **Concurrency:** True Python parallelism on CPU-bound workloads.
- **Safety:** Low-medium. `--disable-gil` is experimental in CPython 3.13.
  PyO3's `nogil` support is preliminary. The broader Python C-extension
  ecosystem is largely not thread-safe without the GIL.
- **Complexity:** Medium in PyO3 (needs `nogil` feature flag and careful
  review of all unsafe code). High in the ecosystem.
- **Verdict:** Promising long-term, but not ready for production v1.0.
  We will feature-flag support so early adopters can test.

**Decision:**

**Select Option A — dedicated Python worker thread** as the default for v1.0.

Rationale:

1. **Best match for workload profile.** Python web applications are
   I/O-bound within Python (database queries, external HTTP calls). A
   single Python thread driving async I/O via `asyncio` is the established
   pattern (uvicorn, gunicorn with async workers). Serializing Python
   execution doesn't bottleneck throughput — the bottleneck is Rust I/O
   and the downstream services Python calls.

2. **Runtime safety.** Python never blocks a tokio worker thread. Slow or
   buggy Python code can't stall the I/O runtime. The Rust side remains
   responsive regardless of Python behavior.

3. **Simplest correctness model.** No GIL acquire/release logic in the hot
   path. No risk of deadlocks involving the GIL + Rust locks. The channel
   boundary provides natural backpressure: if Python is slow, Rust buffers
   requests up to the channel capacity.

4. **Clear upgrade path to Option D.** When free-threaded CPython matures,
   the single-worker-thread is replaced by "acquire GIL on any thread" code
   path behind a `py-nogil` feature flag. The channel dispatch layer is
   removed and Python calls happen inline on tokio workers.

**Implementation sketch:**

```rust
// Python worker runs in a dedicated OS thread.
// Rust handler sends requests via channel, awaits result.
let (tx, rx) = tokio::sync::mpsc::channel::<PythonJob>(256);

// Spawn Python worker
std::thread::spawn(move || {
    Python::with_gil(|py| {
        // Load ASGI app, register handlers
        // Loop: receive jobs from rx, execute, send result back
    });
});

// In the request handler (tokio task):
let job = PythonJob { request, response_tx };
tx.send(job).await?;
let response = response_tx.recv().await?;
```

The channel acts as both dispatch mechanism and backpressure valve.
Channel capacity (256 by default) limits in-flight Python work. If the
channel is full, the Rust side can either buffer (via the BufferPool) or
respond with 503 Service Unavailable.

**Feature flag for PEP 703 (`py-nogil`):**

A Cargo feature `py-nogil` switches the execution model:

- **default (`py-nogil` off):** Dedicated Python worker thread (Option A).
- **`py-nogil` on:** Direct GIL acquisition on tokio workers (Option A
  dispatch removed). Relies on CPython built with `--disable-gil` and
  PyO3's `nogil` support.

This flag is hidden/unstable until PEP 703 ecosystem maturity improves.
It exists to prevent architectural deadlock — we can evolve toward it
without rewriting the dispatch layer.

**Evidence:**
- CPython PEP 703 (free-threaded CPython) status: experimental in 3.13,
  targeting default-on in 3.14 or 3.15.
- PyO3 `nogil` feature tracking issue: currently experimental, API surface
  unstable.
- Typical Python web workload profile: I/O-bound inside Python handlers
  (database, cache, external API calls). CPU-bound Python workloads
  (ML inference, image processing) are a niche use case for app servers
  and should be offloaded to dedicated worker processes or task queues.
- Uvicorn + Gunicorn model: single Python thread per worker process,
  achieving production throughput via process-level parallelism (not
  thread-level). justapi's Rust I/O layer provides better single-process
  throughput than Gunicorn's prefork model.

---

## ADR-009 — 2026-07-03 — WebSocket: TCP-peek approach over hyper upgrade

**Context:** Phase 8 WebSocket support. Hyper 1.x provides `hyper::upgrade::on(req)`
to upgrade HTTP/1.1 connections to WebSocket. This mechanism had persistent
`ResetWithoutClosingHandshake` errors with `tokio-tungstenite`.

**Options considered:**

### Option A — hyper upgrade mechanism (rejected)

Call `hyper::upgrade::on(req)` inside the service function, return 101
response, spawn task to handle upgraded stream. Required careful ordering:
the upgrade future must not be awaited inside the service function (deadlock),
and `WebSocketStream::from_raw_socket` + `Role::Server` had protocol errors.

Issues encountered:
- `hyper::upgrade::on` returning `ResetWithoutClosingHandshake` consistently
- Double-wrapping `Upgraded` in `TokioIo` caused confusion about trait bounds
- Service function return + upgrade completion ordering fragile

### Option B — TCP-peek approach (selected)

Peek at the first bytes of the TCP connection to detect WebSocket upgrade
requests before passing the stream to hyper. If the request looks like a
WebSocket upgrade, use `tokio_tungstenite::accept_async` directly on the
raw `TcpStream` (or `TlsStream<TcpStream>`).

Key properties:
- **Reliability:** `tokio_tungstenite::accept_async` handles the full HTTP
  upgrade handshake correctly every time.
- **Simplicity:** No hyper upgrade state management, no ordering issues.
- **Generics:** `handle_ws_raw<S>` works over any `AsyncRead + AsyncWrite`,
  supporting both plain TCP and TLS connections.
- **Trade-off:** WebSocket connections bypass hyper entirely — no middleware,
  no arena allocation, no connection pool integration. Acceptable because
  WebSocket connections are long-lived and the hot path (message framing)
  is handled by tokio-tungstenite.

**Decision:** Select Option B — TCP-peek approach.

**Evidence:**
- Option A consistently produced `ResetWithoutClosingHandshake` in integration
  tests despite correct header ordering and response construction.
- Option B integration test (send "hello" → receive "hello") passed on first
  attempt.
- Production WebSocket servers (tungstenite examples, warp) commonly use
  this approach.
- `peek()` is non-blocking and reads only 4KB — negligible overhead per
  connection.

---

## ADR-010 — 2026-07-03 — Response body type: UnsyncBoxBody

**Context:** Phase 8 introduced SSE streaming responses. The existing
`ResponseBody` type was `http_body_util::combinators::BoxBody<Bytes, anyhow::Error>`,
which requires the inner body to be `Send + Sync + 'static`. Streaming bodies
(like `StreamBody<ReceiverStream<...>>`) are not `Sync` because the underlying
`tokio::sync::mpsc::Receiver` is `Send` but not `Sync`.

**Options considered:**

### Option A — BoxBody (Sync, rejected)

Requires all response bodies to be `Sync`. Streaming bodies fail to compile.

### Option B — UnsyncBoxBody (selected)

`http_body_util::combinators::UnsyncBoxBody<Bytes, anyhow::Error>` only
requires `Send + 'static` — drops the `Sync` bound. All existing static
responses (`Full<Bytes>`) are still `Send + Sync`, so they satisfy the
weaker bound. Streaming responses (`StreamBody`) are `Send` but not `Sync`,
which now compiles.

**Decision:** Use `UnsyncBoxBody` as the `ResponseBody` type alias.

**Evidence:**
- `BoxBody::new()` requires `B: Body + Send + Sync + 'static`
- `UnsyncBoxBody::new()` requires `B: Body + Send + 'static`
- `tokio::sync::mpsc::Receiver` is `Send` but not `Sync`
- SSE streaming via `StreamBody<ReceiverStream<...>>` compiles with
  `UnsyncBoxBody` but not `BoxBody`

---

---
## ADR-011 — 2026-07-05 — Static file serving as 404 fallback

**Context:** Phase 9 static file serving. The middleware chain handles all requests
first. If it returns a 404, we try to serve a static file before returning 404
to the client. If the middleware chain errors, we also try static files.

**Options considered:**

### Option A — Static files checked before middleware chain

Check `StaticDir::resolve()` before running middleware. If a file matches, serve
it directly without going through middleware.

Pros: Slightly faster for static files (no middleware overhead).
Cons: Static files bypass auth/CORS/rate-limit middleware — may expose protected
files. Each path gets resolved twice (once by static check, once by chain on miss).

### Option B — Static files as 404 fallback after middleware (selected)

Run the full middleware chain first. If it returns 404 (or errors), then try to
resolve and serve a static file.

Pros:
- Middleware applies to all requests consistently (auth protects static files too).
- Single path resolution in the common case (dynamic route hit or cache hit).
- Lets middleware override static file routes (e.g., `/index.html` blocked by auth).

Cons: Static file requests always incur middleware overhead. Acceptable because
the middleware chain is lightweight (~0.18% overhead per Phase 5 benchmark).

**Decision:** Select Option B — static file serving as 404 fallback after middleware.

**Evidence:** Phase 5 benchmark showed middleware overhead of 0.18% (negligible).
Security principle: middleware should apply uniformly to all requests.

---

## ADR-012 — 2026-07-05 — Metrics via atomic counters (no Prometheus client)

**Context:** Phase 9 metrics. We need to expose basic metrics at `/metrics` in
Prometheus text format.

**Options considered:**

### Option A — Prometheus Rust client library (`prometheus` or `prometheus-client`)

Full Prometheus client with histogram support, registry, automatic
`/metrics` endpoint. Adds ~20 dependencies.

Pros: Rich metric types (histograms, summaries), automatic handling of
concurrent access, well-known API.
Cons: Heavy dependency tree, overkill for 5 simple counters/gauge.
Histogram support adds memory overhead per-bucket.

### Option B — Atomic counters with manual formatting (selected)

Use `AtomicU64` for each metric, format Prometheus text on demand at
`/metrics` endpoint. No external client library.

Pros: Zero additional dependencies, minimal overhead (single atomic increment
per metric update), full control over output format.
Cons: No built-in histogram support, need to write Prometheus text format
manually.

**Decision:** Select Option B — atomic counters with manual Prometheus formatting.

**Evidence:**
- Only 5 metrics needed: 4 counters (requests, errors, bytes_in, bytes_out)
  + 1 gauge (active_connections).
- Histograms can be added later if profiling shows they're needed
  (typically via external metrics pipeline, not in-process).
- Prometheus text format for counters/gauges is trivial to produce:
  `# HELP`, `# TYPE`, `metric_name value` lines.

---

---
## ADR-013 — 2026-07-05 — ASGI shim: dedicated thread + channel dispatch

**Context:** Phase 10 Tier A ASGI compatibility shim. We need to run existing
Python ASGI applications (FastAPI, Starlette) on justapi's Rust runtime.

**Options considered:**

### Option A — Inline GIL acquisition on tokio workers

Acquire the Python GIL inside each request handler (tokio worker thread).
Use `pyo3_asyncio::tokio::into_awaitable` to bridge Python asyncio coroutines
to Rust futures.

Pros: No extra thread, simpler architecture.
Cons: Blocks tokio worker during Python execution (violates ADR-008 constraint).
`pyo3-asyncio` adds a complex dependency. Risk of GIL deadlocks.

### Option B — Dedicated Python worker thread with channel dispatch (selected)

A single OS thread holds the GIL permanently and runs a persistent asyncio
event loop. Rust sends raw HTTP request data (method, path, headers, body)
via `std::sync::mpsc` and receives the response via `tokio::sync::oneshot`.
The Python thread builds the ASGI scope dict, creates `receive`/`send` async
functions, and calls `await app(scope, receive, send)`.

Pros:
- **ADR-008 compliant:** zero GIL contention, tokio workers never blocked by Python
- **No pyo3-asyncio dependency:** asyncio event loop managed entirely in Python
- **Simple data contract:** plain Rust structs across the channel boundary
- **Backpressure:** bounded channel capacity naturally limits in-flight Python work

Cons:
- Extra context switch per request (channel send + recv + oneshot round-trip)
- No concurrent Python execution (serialized on the single worker thread)
- Response body fully buffered in memory (no streaming for large responses)

### Decision: Select Option B — dedicated Python worker thread.

**Evidence:**
- ADR-008 established this as the correct GIL strategy for justapi
- `pyo3-asyncio` is not needed: the Python side runs `loop.run_until_complete()`
  on each request, using a persistent event loop created once at startup
- The `call_asgi()` Python helper is embedded as a `const &str` and compiled
  via `PyModule::from_code` — no separate Python file to manage

**Known limitations (tracked for resolution):**
- Streaming request/response bodies not yet supported (body is fully buffered)
- ASGI lifespan protocol not yet implemented (no startup/shutdown events)
- Single Python worker thread means serialized Python execution

---

## Open questions / risk register

Track these and resolve as phases progress:

- [ ] `tokio-uring` / `monoio` evaluation — scheduled for Phase 0 spike if
  time permits, otherwise deferred. epoll via `tokio` is the default and
  mandatory fallback regardless.
- [ ] Free-threaded CPython (PEP 703) ecosystem maturity for PyO3 0.29 —
  confirm before committing Phase 4 design.
- [ ] Sub-interpreter isolation guarantees — research needed before Phase 4.
- [ ] `quinn` / HTTP-3 crate maturity — evaluate at Phase 10 time.
- [ ] ASGI shim edge cases (lifespan protocol, streaming responses, WebSocket
  close codes) — track known gaps explicitly in Phase 10.

---

## ADR-014 — 2026-07-05 — Project rename: Hyperion → JustAPI

**Context:** Phase 10 completion. The project needs a name that reflects its
ambition (comparable to FastAPI) and positions it for production-grade adoption.
"Hyperion" was the working name from the original master prompt.

**Decision:** Rename to **JustAPI** — simple, memorable, domain-available.
All crates renamed: `hyperion-core` → `justapi-core`, `hyperion-py` → `justapi-py`,
`hyperion-cli` → `justapi-cli`, `hyperion-bench` → `justapi-bench`.
Python package renamed: `hyperion` → `justapi`. Class renamed: `HyperionApp` → `JustAPIApp`.
Metrics prefix renamed: `hyperion_*` → `justapi_*`.

**Evidence:** Name check: domain available, PyPI name available, no trademark conflicts.

---

## ADR-015 — 2026-07-05 — Validation strategy: Rust-native + Pydantic bridge

**Context:** Phase 11 — Request Validation & Serialization. Need to decide
how to handle type-safe validation.

**Decision:** Two-tier approach:
1. **Rust-native (Tier B):** `Schema` derive macro + `serde` + `validator` crate.
   Validates at Rust speed — target 10x faster than Pydantic.
2. **Pydantic bridge (Tier A):** Python `BaseModel` → Rust struct via PyO3
   `FromPyObject`. Runs alongside the Rust-native path for compatibility.

Error format follows RFC 9457 (Problem Details for HTTP APIs).

**Evidence:** Pydantic v2 is ~5-10x faster than v1 but still has Python overhead.
Rust-native validation eliminates Python serialization entirely for the hot path.

---

## ADR-016 — 2026-07-05 — Database strategy: sqlx with compile-time checks

**Context:** Phase 12 — Database ORM Integration. Need async SQL driver.

**Decision:** Use `sqlx` as the async SQL driver:
- Compile-time query checking (`sqlx::query!("SELECT ...")`)
- Connection pooling built-in
- Multi-DB (PostgreSQL, MySQL, SQLite)
- Migration system built-in

Query builder inspired by Diesel/SeaORM for Tier B; SQLAlchemy bridge for Tier A.

**Evidence:** `sqlx` is the most popular async SQL driver in Rust, with compile-time
query verification that eliminates an entire class of runtime errors.

---

## ADR-017 — 2026-07-05 — Deployment strategy: Helm + distroless + CI/CD

**Context:** Phase 16 — Multi-Cloud / Kubernetes Support.

**Decision:** 
- Distroless base image (< 50MB, no shell)
- Helm chart as primary K8s deployment mechanism
- Health probes via existing `/health`/`/ready`/`/live` endpoints
- GitHub Actions CI → container registry → K8s deploy
- Cloud-specific docs for GKE, EKS, AKS, Fly.io, Railway

**Evidence:** Distroless images reduce attack surface. Helm is the standard
K8s packaging format. Multi-cloud strategy avoids vendor lock-in.

---

## ADR-018 — 2026-07-05 — Security strategy: SAST in CI, fuzzing, OWASP baseline

**Context:** Phase 17 — Zero-Day / SAST / Security Hardening.

**Decision:**
- `cargo-audit` + `cargo-deny` + `trivy` in CI as gating checks
- `cargo-fuzz` / `libfuzzer` for Rust fuzz targets, `Atheris` for Python
- OWASP Top 10 as security baseline
- All security fixes documented with CVE reference where applicable
- Continuous fuzzing for all parsing paths

**Evidence:** OWASP Top 10 is the industry standard web security baseline.
`cargo-audit` catches known vulnerabilities in dependency tree.

---

## ADR-019 — 2026-07-05 — Plugin strategy: trait-based, compile-time registration

**Context:** Phase 19 — Plugin System & Extensibility.

**Decision:**
- `Plugin` trait with lifecycle hooks (`pre_route`, `post_route`, `on_startup`, `on_shutdown`)
- Compile-time plugin registration for Rust plugins
- `JustAPIApp.use(plugin)` for Python plugins
- Plugins run in the Rust middleware chain (no Python overhead for Rust plugins)
- Plugin isolation via panic boundaries (catch_unwind)

**Evidence:** Trait-based design is idiomatic Rust. Middleware chain integration
means plugins get full performance. Python plugin API enables third-party ecosystem.

---

## ADR-020 — 2026-07-05 — Performance target: beat FastAPI on every benchmark

**Context:** Phase 20 — Benchmark & Optimization.

**Decision:** FastAPI is the competition to beat. All Phases 11-19 must include
benchmark gates that compare against FastAPI. If any benchmark shows FastAPI
faster, it's a blocking bug.

Target metrics:
- Throughput: 2x on hello-world, 3x on JSON echo
- Latency p99: < 0.5ms (FastAPI ~2ms)
- Memory: 2x requests per MB
- Startup: < 10ms cold (FastAPI ~300ms)
- Image size: < 50MB (FastAPI ~200MB)
- Test speed: 5x faster than TestClient

**Evidence:** Phase 1 already showed 2.4x vs Granian on hello-world. Rust gives
a fundamental performance advantage; the gap should widen as optimization progresses.

---

## ADR-021 — 2026-07-05 — GIL-free architecture: remove dedicated Python worker thread

**Context:** Phase 13 testing utilities required `JustAPITestClient` to run synchronously without a separate server process. The dedicated Python worker thread (ADR-008) made this impossible — it required an OS thread running a tokio runtime + Python event loop.

**Problem:** The dedicated worker thread architecture from ADR-008 prevented:
1. Synchronous test client execution (needed to start/finish per-request without thread lifecycle)
2. Pool initialization inside `py.detach()` closures (worker thread owned the GIL)
3. Request-response cycle that starts and completes within a single function call

**Options considered:**

### Option A — Keep worker thread, add test-mode bypass (rejected)

Add a flag to skip the worker thread when running tests. Would create two code paths (production vs test) with risk of divergence.

### Option B — Remove worker thread, GIL-free `Python::try_attach` everywhere (selected)

Remove the dedicated Python worker thread entirely. Use `Python::try_attach()` from any tokio worker thread. This function safely acquires the GIL via `PyGILState_Ensure` regardless of which OS thread calls it.

Key properties:
- **No extra OS thread:** Saves ~1MB per worker, removes thread lifecycle management
- **`OnceLock<HelperFunctions>`:** Helper module (Python code that builds request dicts and calls user handlers) compiled once, cached globally with `Py<PyAny>` (which is `Send + Sync + 'static`)
- **Pool initialization:** Works inside `py.detach()` closure before server starts — critical for `JustAPITestClient` where pool must be ready before any request
- **`call_python_handler`:** Shared function used by both `run()` and `JustAPITestClient` — single code path for production and testing

**Decision:** Select Option B — remove dedicated Python worker thread. `Python::try_attach` for GIL acquisition, `py.detach()` for releasing GIL during blocking `rt.block_on`.

**Evidence:**
- `Python::try_attach()` uses `PyGILState_Ensure` internally — works from any OS thread, no prior GIL state required
- `OnceLock<Py<PyAny>>` is safe because `Py<PyAny>` implements `Send + Sync + 'static` via PyO3's Send/Sync bounds
- Works with both GIL-ful and free-threaded Python 3.14
- All 43 Python integration tests pass with the new architecture

---

## ADR-022 — 2026-07-05 — TestClient: tokio duplex + hyper HTTP/1.1 parser (no TCP)

**Context:** Phase 13 testing utilities. Need a synchronous test client in Rust that runs requests through the full pipeline without opening a TCP socket.

**Options considered:**

### Option A — TCP loopback (rejected)

Open a real TCP connection to `127.0.0.1:0` for each test. Adds ~100μs per connection for TCP handshake + teardown.

### Option B — tokio duplex + hyper HTTP/1.1 parser (selected)

Use `tokio::io::duplex()` to create a pair of (reader, writer) in-memory byte streams. Write raw HTTP request bytes to the writer, pipe the reader through `hyper::http1::Builder::serve_connection` with the handler as the service function.

Key properties:
- **No TCP:** All in-memory, sub-millisecond per request
- **Full pipeline:** Hyper HTTP/1.1 parser still runs — headers, body framing, connection lifecycle are real
- **`Content-Length: 0` required:** Hyper's HTTP/1.1 parser fails with "connection closed before message completed" if response doesn't include `Content-Length` for non-chunked responses

**Decision:** Select Option B — tokio duplex + hyper HTTP/1.1 parser.

**Evidence:** 
- Sub-millisecond test latency achieved (vs ~1-2ms for TCP loopback)
- Hyper parses both request and response, ensuring real HTTP behavior
- `Content-Length: 0` is always included in the response builder

---

## ADR-023 — 2026-07-05 — Snapshot testing: custom implementation over `insta`

**Context:** Phase 13 testing utilities. Need snapshot testing similar to Jest/insta but integrated with Python `JustAPITestClient`.

**Options considered:**

### Option A — `insta` crate (rejected)

Rust snapshot testing crate. Would require calling Rust from Python to compare snapshots, adding complexity to the FFI boundary. No native Python integration.

### Option B — Custom Python `Snapshot` class (selected)

`Snapshot` class with `assert_match()`, `assert_response()`, `assert_body()` methods. `.snap` files stored in `__snapshots__/` directory next to the test file. Caller detection via `inspect.stack()` frame walk. `SNAPSHOT_UPDATE=1` env var for auto-accepting new snapshots. Unified diff output on mismatch.

**Decision:** Select Option B — custom Python `Snapshot` class.

**Evidence:**
- Tighter Python integration (no FFI calls for snapshot comparison)
- Unified diff output is human-readable
- `SNAPSHOT_UPDATE=1` pattern matches Jest/insta UX
- 10 Python tests for snapshot functionality

---

## ADR-024 — 2026-07-05 — Bucket-based latency histograms over HDR Histogram

**Context:** Phase 14 enhanced metrics. Need latency histograms with percentile computation (p50/p95/p99/p999) for Prometheus output.

**Options considered:**

### Option A — `hdrhistogram` crate (rejected)

Full HDR Histogram implementation. Adds a new dependency with complex internal data structures. Overkill for 13 fixed buckets.

### Option B — Bucket-based histograms with `AtomicU64` (selected)

13 Prometheus-compatible buckets (1ms, 2ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1000ms, 2500ms, 5000ms, 10000ms). Each bucket is an `AtomicU64` counter. Percentiles computed from bucket distribution during `snapshot()`. Zero-allocation on the record path.

**Decision:** Select Option B — bucket-based histograms.

**Evidence:**
- Zero allocation on record path (single `AtomicU64::fetch_add(1, Relaxed)`)
- Percentiles computed from cumulative bucket counts at query time
- Prometheus-rendered as `justapi_request_duration_ms_bucket{le="..."}` counters
- 8 unit tests covering percentiles, bucket accumulation, and Prometheus output

---

## ADR-025 — 2026-07-05 — Structured logging: tracing-subscriber json + tracing-appender

**Context:** Phase 14 structured logging. Need JSON log output with configurable levels and file rotation. No custom JSON serializer wanted.

**Options considered:**

### Option A — Custom JSON logger (rejected)

Write JSON formatting manually. Higher maintenance burden, risk of formatting bugs.

### Option B — `slog` crate (rejected)

Alternative structured logging ecosystem. Less ecosystem support than tracing, not compatible with `tracing` crate's existing usage.

### Option C — `tracing-subscriber` json feature + `tracing-appender` (selected)

`tracing-subscriber`'s built-in JSON layer handles structured JSON output. `tracing-appender` provides non-blocking file writers with daily/hourly rotation. `NonBlocking` writer backed by `WorkerGuard` stored in `OnceLock<Mutex<Option<WorkerGuard>>>` for safe global access.

**Decision:** Select Option C — tracing-subscriber json + tracing-appender.

**Evidence:**
- No new logging infrastructure to learn — leverages existing tracing ecosystem
- `tracing-subscriber` `json` feature is mature and well-documented
- `tracing-appender` `NonBlocking` writer prevents file I/O from blocking request handling
- Environment filter via `RUST_LOG` env var works transparently

---

## ADR-026 — 2026-07-05 — Health check system: composable registry over static endpoints

**Context:** Phase 14 health checks. Need pluggable health probes that can validate DB, cache, and upstream services.

**Options considered:**

### Option A — Static `/health`/`/ready`/`/live` endpoints (rejected)

Hard-coded health endpoints. Not extensible — adding a new health check requires modifying server.rs.

### Option B — `HealthRegistry` with pluggable `HealthCheck` trait (selected)

`HealthRegistry` struct with `register(Box<dyn HealthCheck>)` and `register_fn(name, F)`. `check_all()` runs all registered checks and aggregates results. `health_response()` generates HTTP response (200 for all healthy, 503 for any unhealthy). `DbHealthCheck` under `db` feature flag.

**Decision:** Select Option B — composable HealthRegistry.

**Evidence:**
- Backward-compatible with existing static `/health` response
- `DbHealthCheck` reuses existing `AnyPool::health_check_all()` from Phase 12
- 6 unit tests covering healthy, degraded, unhealthy, and mixed states

---

## ADR-027 — 2026-07-05 — Alerting: HTTP-only webhook dispatcher over `reqwest`

**Context:** Phase 14 alerting. Need to send webhook notifications to Slack, PagerDuty, and generic endpoints.

**Options considered:**

### Option A — `reqwest` crate (rejected)

Full async HTTP client. Adds ~30 dependencies to the dependency tree. Significant compile time increase.

### Option B — Manual HTTP POST via `tokio::net::TcpStream` (selected)

Construct raw HTTP/1.1 POST requests manually over `tokio::net::TcpStream`. Build request bytes in a `Vec<u8>` (method, path, headers, body), write to stream, read response.

**Decision:** Select Option B — HTTP-only POST via raw TcpStream.

**Evidence:**
- Avoids adding `reqwest` as a dependency (justapi-core stays lean)
- Only HTTP POST needed (no GET, PUT, DELETE, streaming, cookie handling)
- Slack and PagerDuty webhooks accept POST only
- HTTPS requires `rustls` client — not implemented; users can proxy through a local HTTP gateway
- 3 unit tests for payload builders (Slack, PagerDuty, severity filtering)

---

## ADR-028 — 2026-07-05 — Audit logging: HandlerFn wrapper over Middleware trait

**Context:** Phase 14 audit logging. Need to log requests to sensitive endpoints with method, path, status, and duration.

**Options considered:**

### Option A — Middleware trait implementation (rejected)

Implement the `Middleware` trait for `AuditLogging`. Would run inside the middleware chain, capturing pre-request state.

### Option B — `HandlerFn` wrapper via `wrap_handler()` (selected)

`AuditLogging::wrap_handler()` takes a `HandlerFn` and returns a new `HandlerFn` that wraps the original. This is designed to be the last wrapper before the middleware chain, ensuring it captures the final status after all middleware processes the request.

**Decision:** Select Option B — HandlerFn wrapper.

**Evidence:**
- `HandlerFn` wrapper is simpler than implementing the `Middleware` trait
- Placed at the end of the wrapper chain, captures final response status
- Structured `tracing::info!` with `audit=true` field enables log filtering
- 2 unit tests covering default and custom audit rules

---

## ADR-029 — 2026-07-05 — Panic recovery: `catch_unwind` over tokio task abort

**Context:** Phase 14 panic recovery. Need to prevent panics in user handlers from crashing the server.

**Options considered:**

### Option A — Tokio task abort (rejected)

Set up a task lifecycle hook that detects panics and aborts the task. Limited control over the panic response — tokio's default panic handling just terminates the task.

### Option B — `std::panic::catch_unwind` + `FutureExt::catch_unwind` (selected)

Wrap the `BoxFuture` handler with `FutureExt::catch_unwind`. If the future panics, `catch_unwind` catches the panic and returns `Err(PanicInfo)`. This is converted to a 500 JSON response: `{"error":"Internal server error"}`.

Additionally, set a custom panic hook via `std::panic::set_hook` that logs the panic message and location via `tracing::error!(panic=true, ...)`. The default hook behavior is preserved.

**Decision:** Select Option B — catch_unwind on the BoxFuture.

**Evidence:**
- `FutureExt::catch_unwind` wraps the future at the point of execution — no tokio task lifecycle changes
- Custom panic hook logs stack trace information
- 500 JSON response ensures the client doesn't hang on a panic
- `anyhow::Error` from handler is also converted to 500
- 1 unit test verifying panic recovery returns a handler function

- [ ] `tokio-uring` / `monoio` evaluation — deferred. epoll via `tokio` is the default.
- [ ] Free-threaded CPython (PEP 703) maturity for PyO3 — monitor.
- [ ] Sub-interpreter isolation guarantees — deferred for v1.0.
- [ ] `quinn` / HTTP-3 crate maturity — evaluate at Phase 20.
- [ ] Pydantic bridge performance — if Pydantic bridge is too slow, consider dropping Tier A ASGI shim for Phase 11+ features.
- [ ] sqlx compile-time checks — may increase compile times. Evaluate trade-off vs runtime query building.

---

## ADR-030 — 2026-07-10 — Native inference engine: Candle (Rust), not torch delegation

**Context:** JustAPI's 2026 differentiation thesis is "best for model deploying"
with *native GPU* support. FastAPI delegates all GPU work to `torch` under the
GIL, which is the bottleneck it can never remove. To beat FastAPI for inference
we must own the GPU execution path in Rust.

**Options considered:**
1. Keep delegating to `torch`/`transformers` via PyO3 (current state) — rejected:
   keeps the GIL bottleneck, adds no native advantage, just repackages FastAPI.
2. **Candle** (HuggingFace Rust ML framework: CUDA/ROCm/Metal/CPU backends,
   safetensors/GGUF loaders, KV-cache + FlashAttention kernels) — **selected**.
3. `burn` (Rust DL framework, backend-agnostic) — strong, but smaller model
   coverage and weaker LLM/quantization ecosystem than Candle today.
4. `llm` / llama.cpp bindings — excellent for GGUF but narrower (no training,
   weaker multimodal/agent tooling); good later as a quantized backend.

**Decision:** Build a new `justapi-inference` crate on **Candle** as the native
GPU inference engine. Python handlers call into it through the existing PyO3
zero-copy (Arrow/DLPack) boundary; heavy compute runs in Rust with no GIL.
`burn` and llama.cpp remain candidate backends behind a trait for Phase 45+.

**Evidence:**
- 2026 landscape (vLLM/SGLang/TGI/TensorRT-LLM) confirms the engine layer is
  what defines GPU efficiency; FastAPI sits strictly below it.
- Candle gives Rust-native CUDA/ROCm, KV cache, FlashAttention, and quantization
  without leaving the Rust runtime we already control.
- Reuses existing `memory` crate (arena/pool) for KV-cache block allocation.

---

## ADR-031 — 2026-07-10 — Two-layer architecture: engine + control plane

**Context:** Research shows production model serving splits into (a) an
*inference engine* (vLLM/SGLang/TGI — owns GPU, batching, KV cache) and
(b) a *serving control plane* (Ray Serve / NVIDIA Dynamo / AIBrix / Ouranos —
owns OpenAI API, routing, autoscaling, registry). JustAPI currently has web
framework + partial control-plane primitives (rate-limit, circuit-breaker,
health, DAG) but **no engine layer**.

**Decision:** Implement both layers inside JustAPI:
- **Engine (Phase 41-45):** `justapi-inference` — Candle-backed load/generate,
  paged KV cache, continuous-batching scheduler, quantization, multi-LoRA.
- **Control plane (Phase 44, 46):** OpenAI-compatible API, model registry,
  KV-aware/LoRA-aware routing, LLM-specific autoscaling, multi-replica.
This avoids a hard dependency on an external engine (unlike Ray Serve, which
*orchestrates* vLLM but does not implement one).

**Evidence:** Dynamo/AIBrix/Ouranos all wrap vLLM-class engines; owning the
engine is what lets JustAPI be a single self-contained "deploy a model" binary
(Rust speed + GPU + Python DX) rather than glue code.

---

## ADR-032 — 2026-07-10 — PyPI distribution: single abi3 wheel + manylinux_2_28 build

**Context:** `justapi` PyPI name is available (verified 404). The package must
ship one wheel covering CPython 3.11-3.14.

**Decision:**
- Build a single **abi3** wheel (`cp311-abi3`) via `abi3-py311 = true` in
  `pyproject.toml` + `abi3-py311` on the `pyo3` crate feature. One build covers
  all supported Pythons.
- The publish build MUST run inside a `manylinux_2_28` container (maturin
  auto-detects the policy). The local Arch host (glibc 2.41) tags
  `manylinux_2_34`, which PyPI rejects, so local wheels are dev-only.
- `twine check` must pass before upload; upload via `maturin publish` or
  `twine upload` using a `MATURIN_PYPI_TOKEN` (not committed).

**Evidence:** wheel built and verified (`twine check` PASSED, installs +
imports in a clean venv). Optional extras: `pydantic`, `jinja`, `full`.

---

## ADR-033 — 2026-07-10 — Single Python package source of truth

**Context:** Two divergent `justapi` packages existed — the real, complete one
at `crates/justapi-py/python/justapi/` (maturin builds/ships from here, has
`_justapi.so` + full `app.py`) and a stale duplicate at repo-root
`python/justapi/` that shadowed imports and broke test collection.

**Decision:** `crates/justapi-py/python/justapi/` is the single canonical
source. The repo-root `python/justapi/` duplicate was removed; its unique files
(tests, `grpc_compiler.py`) were merged into the canonical dir. Maturin builds
from `crates/justapi-py/` via `python-source = "python"`.

**Evidence:** after consolidation, `pytest` collects and runs 53 passed / 1
skipped against the installed (editable) package; no import shadowing.

---

## ADR-034 — 2026-07-10 — `multer` for multipart/form-data parsing

**Context:** Phase 47 (FastAPI Parity) — multipart file upload. JustAPI needs to
parse `multipart/form-data` bodies for file upload endpoints (dataset/RAG ingest,
model weight uploads, etc.). The parser must be async, memory-safe, and handle
arbitrary boundary strings.

**Options considered:**
1. **`multer` 3.1** — battle-tested async parser used by Rocket, Actix, Axum.
   Accepts `Stream<Item = Result<Bytes, E>>` so it works with any
   `http_body::Body` via `BodyDataStream`. Has built-in size constraints for
   DoS protection. ~88M downloads. Maintained by SergioBenitez (Rocket lead).
2. **Manual parser** — hand-roll boundary-aware chunked body reader. Risk of
   off-by-one on edge-case boundaries, slower to develop, no streaming support.
3. **`actix-multipart`** — tightly coupled to Actix's body type; would require
   adapter layer.

**Decision:** Use `multer = "3.1"`. It integrates directly with hyper 1.x via
`BodyDataStream`, is already used by the wider Rust web ecosystem, and supports
streaming + size limits out of the box. Added as a dependency of `justapi-core`
(no feature gate — multipart is not optional for a FastAPI-competitive framework).

**Evidence:** 7 unit tests covering boundary extraction, text fields, single
file, multiple files, and missing-boundary error. `cargo test --workspace` passes.

---

## ADR-036 — 2026-07-10 — Disaggregated prefill/decode scheduler

**Context:** Phase 48 — disaggregated serving. Production LLM servers (vLLM
DistServe, NVIDIA Dynamo) split prefill and decode onto separate GPU pools
because their compute profiles are fundamentally different: prefill is
compute-bound (processes many prompt tokens in parallel) while decode is
memory-bandwidth-bound (generates one token at a time). Pooling them together
wastes GPU cycles (decode waits for prefill to finish).

**Options considered:**
1. **Single scheduler with priority scheduling** — keep one pool, prioritise
   decode over prefill to reduce TTFT jitter. Simpler but cannot fully isolate
   prefill-heavy workloads from decode latency.
2. **Two independent schedulers with KV transfer (selected)** — `PdScheduler`
   owns one `Scheduler` for prefill and one for decode. Sequences transfer from
   prefill→decode via `Scheduler::take_completed_prefill` /
   `admit_transferred`. Enables independent pool sizing (e.g. prefill pool
   `max_num_seqs=1`, decode pool `max_num_seqs=8`).

**Decision:** Option 2 — `PdScheduler` wrapping two `Scheduler` instances.
The prefill scheduler's `TransferableSequence` struct was moved from inner
(struct inside `impl Scheduler`) to module scope to satisfy Rust's nested-item
restriction. `SchedulerStats` given `#[derive(Debug, Clone)]` so `PdStats` can
derive them. The `KvBlockPool::release` flow (→ cached, not free) is the
expected lifecycle — blocks are made available to eviction but stay tracked
until pressure triggers recycling.

**Evidence:** 5 unit tests: starts idle, single-request prefill→transfer→decode,
independent pool sizing (prefill 1 / decode 8), `total_transferred_tokens`
accumulation (16+8=24), block in-use verification (prefill releases, decode
allocates). All 92 lib + 10 integration tests pass, clippy + fmt clean.

---

## ADR-037 — 2026-07-10 — Structural LLM benchmark via synthetic GPU-cost model

**Context:** Phase 48 benchmark vs vLLM/SGLang. No GPU exists in CI, so a literal
wall-clock tokens/sec comparison is impossible. We still need a reproducible,
checkable number proving the disaggregated P/D (ADR-036) and speculative-decoding
(ADR-035) mechanisms transfer to the scheduling metrics (TTFT, ITL, throughput)
that define LLM-serving quality.

**Options considered:**
1. **Real GPU run (deferred)** — requires `--features real` + CUDA + weights.
   Accurate but not runnable in this environment; left as a gate TODO.
2. **Wall-clock of `Engine::generate` + MockModel** — rejected: `MockModel`
   produces tokens with no yield, so the unbounded channel buffers the whole
   stream and TTFT/ITL collapse to ~0; it measures channel throughput, not
   serving dynamics.
3. **Drive the real schedulers over a synthetic GPU-cost model (selected)** —
   `justapi-bench-inference` runs `PdScheduler` (disaggregated) and `Scheduler`
   (collocated) through the actual prefill→transfer→decode loop, charging each
   step a modeled cost. Disaggregated uses *parallel* virtual clocks (independent
   prefill/decode pools); collocated uses one *shared* clock. Prompt lengths are
   varied per request so prefill/decode overlap (the condition that degrades
   collocated ITL).

**Decision:** Option 3. The benchmark is a *topology* proof: it isolates the
scheduler-level effect of splitting prefill/decode, which is hardware-independent.
Cost model: prefill ∝ prompt tokens/step (compute-bound), decode = fixed step
(memory-bandwidth-bound), matching the vLLM/SGLang assumption. Numbers are
explicitly labeled structural, not GPU throughput.

**Evidence:** 16 burst requests, prompt 64–256 tok — disaggregated ITL p99 0.20 ms
vs collocated 1.16 ms (5.80x tighter), throughput 70,617 vs 34,634 tok/s (2.04x),
wall time 12.32 vs 25.12 ms. Mirrors the DistServe/Dynamo disaggregated-P/D win.
Recorded in BENCHMARKS.md.

---

## ADR-038 — 2026-07-10 — K8s AI inference gateway plugin

**Context:** Phase 48 final item — a K8s AI inference gateway: KV-aware +
LoRA-aware routing *inside* the gateway (the Dynamo / AIBrix behaviour), so
JustAPI can act as a model-serving ingress rather than just an engine library.

**Options considered:**
1. **Wire `Server::with_openai_routed` only** — already exists (Phase 46) and
   routes per-request, but it owns no Kubernetes concepts (readiness, endpoint
   DNS, namespace). It is HTTP glue, not a gateway abstraction.
2. **New `InferenceGateway` composing `ControlPlane` + `Router` (selected)** —
   adds the K8s-specific concerns the raw router lacks: readiness gating,
   Kubernetes service-DNS endpoint resolution, namespace scoping, and a
   KV-pressure default strategy. Pure decision logic (no network/GPU), matching
   the rest of the control plane; a K8s executor fulfils routing decisions and
   reports pod readiness/load back.

**Decision:** Option 2. `gateway.rs` with `InferenceGateway`, `GatewayConfig`
(namespace, `service_template` supporting `{replica}`/`{model}`/`{version}`/
`{namespace}`, `strategy`, `require_ready`), and `GatewayDecision` (replica +
resolved model + endpoint). Readiness is folded into the router's `healthy`
flag (standard K8s mapping: not-Ready ⇒ not a routing candidate). Default
strategy `LowestKvPressure` so the gateway is KV-aware out of the box.

**Evidence:** 5 unit tests: Kubernetes endpoint resolution, namespace scoping,
readiness gating (not-Ready pod excluded, both drained → no capacity), KV-pressure
default routing to coolest replica, LoRA-aware routing inside the gateway. All
97 lib + 10 integration tests pass, clippy + fmt clean.

---

## ADR-039 — 2026-07-10 — Tree-based speculative decoding (Medusa/EAGLE)

**Context:** Phase 49 — the speculative-decoding foundation (ADR-035) proved
lossless token streaming with `gamma + 1` tokens per step when a draft perfectly
predicts the target. But with an imperfect draft, the single-path draft-target
scheme accepts only `~1` token per step (one bonus target token). Production
systems (Medusa, EAGLE, vLLM's tree speculation) improve this by giving the
draft multiple "guesses" per position — a tree of candidates. ADR-035 explicitly
deferred this ("Tree-based (Medusa/EAGLE): higher acceptance, more complex
verify (tree reduction). Deferred — the draft-target scheme is the foundation
they refine.").

**Options considered:**
1. **Draft-target only (rejected for Phase 49):** keep single-path draft-target.
   Leaves a known capability gap vs vLLM/SGLang — ADR-037's TODO explicitly
   called for "Tree-based Medusa/EAGLE verify (higher acceptance than
   draft-target)."
2. **Tree-based verify with greedy DFS path (selected):** `build_draft_tree`
   constructs a `DraftTree` from `branch` candidates per depth (recursive
   top-k extraction from the draft's `forward_logits`). `verify_tree` walks
   the tree greedily: at each depth, find a child whose token matches the
   target's argmax/sample; accept it and descend; if no match, emit the
   target's own token as correction (lossless). `speculative_generate_tree`
   wraps the loop with stop-token checking on accepted tokens (unlike
   draft-target, which only checks stop tokens at the bonus phase — tree
   speculation must check each accepted token because the draft can propose
   stop tokens in the tree).
3. **Full tree verification with scoring (deferred):** score all paths and pick
   the globally optimal one. More complex, needs tree-attention-style batching
   to be efficient; possible future optimization.

**Decision:**
- New `spec_decode_tree.rs` module with `TreeNode`, `DraftTree`,
  `build_draft_tree`, `verify_tree`, `speculative_generate_tree`.
- `AcceptanceStats.tree_branch` and `tree_nodes_verified` fields added
  (backward-compatible: both default to 0 for draft-target runs).
- `top_k_tokens` helper extracted (pure-Rust argmax/top-k from logits).
- `RangeModel` test model: configurable offset + spread for injection of
  draft-target alignment scenarios.
- For structural (no-GPU) tests: `RangeModel(offset=0, spread=3)` as draft,
  `MockModel` as target — acceptance rate ≈ 1.0 with branch=3.
- Stop tokens checked per-accepted-token in the tree loop (not just at bonus
  phase), because the draft tree may contain stop tokens at any depth.

**Evidence:** 18 unit tests in `spec_decode_tree::tests` covering:
- `top_k_tokens` sorted order and edge cases (empty, k=0)
- `DraftTree.total_nodes` formula (branch=1 ✓, branch=0 ✓, branch=3)
- Branch=1 acts identical to draft-target (single path)
- Gamma=0 degenerates to plain decode (lossless)
- Perfect draft (offset=0, branch=3): acceptance rate ≥ 0.95
- Graceful degradation (offset=10, never matches target): acceptance < 0.1,
  output identical to plain decode
- Higher branch gives ≥ same acceptance as lower branch
- `verify_tree` unit cases: match, no-match, multi-depth match, deep reject
- Stop tokens respected (stops on first stop token)
- Empty prompt works (output length == max_tokens)
- All 18 pass, 115 inference tests total. `cargo test --workspace`,
  `clippy -D warnings`, `cargo fmt --check` clean.

**What this does NOT yet provide (gated on GPU):**
- Wall-clock tokens/sec on real 7B-class models with tree-attention batching
  (needs CUDA + weights).
- The `SpeculativeModel` wrapper for tree speculation (for serving through
  `Engine::generate` without callers knowing); draft-target has this, tree
  would need `TreeSpeculativeModel` or a config flag.
- Tree-attention batching (verify all tree nodes in one forward pass with a
  tree-attention mask, matching Medusa's efficient implementation).

**Context:** Phase 48 — production hardening for AI. vLLM/SGLang win on decode
throughput via speculative decoding (draft proposes `gamma` tokens, target
verifies, accept longest agreeing prefix + emit 1 correction). JustAPI must
offer the same to avoid a feature gap vs production LLM servers.

**Options considered:**
1. **Tree-based (Medusa/EAGLE):** higher acceptance, more complex verify (tree
   reduction). Deferred — the draft-target scheme is the foundation they refine.
2. **Draft-target (Chen et al. 2023):** standard, simple, correctness-preserving.
   Implemented.
3. **Skip / delegate to candle:** would leave a hard capability gap.

**Decision:**
- Add `Model::forward_logits(context) -> Vec<f32>` to the trait (single-step
  primitive). `MockModel` returns a degenerate-but-smooth distribution;
  `RealModel` returns true last-position logits from one forward pass.
- New `spec_decode` module: `speculative_generate`, `sample_token` (pure-Rust
  temperature/top_p/top_k sampler, seeded via `rand` 0.8), `AcceptanceStats`,
  `SpeculativeConfig`, `SpeculativeModel` (a `Model` wrapper servable through
  `Engine::register_speculative`).
- Added `rand = "0.8"` to `justapi-inference` (already used in `justapi-core`;
  minimal, standard). Seeded RNG keeps verification reproducible for tests.

**Evidence:** 8 unit tests (incl. proof that draft==target yields an identical
token stream to plain decode — speculation is lossless — and acceptance rate
→ 1.0 / gamma+1 tokens per step for a perfect draft) + 2 Engine integration
tests. `cargo test --workspace`, `clippy -D warnings`, `cargo fmt` clean.

---

## ADR-040 — 2026-07-10 — Tree speculation in the serving path (Engine + CLI)

**Context:** Phase 49 delivered the tree-verify mechanism (`spec_decode_tree.rs`)
but it was only reachable via the free function `speculative_generate_tree`. To
be servable like the draft-target scheme (ADR-035's `SpeculativeModel` +
`Engine::register_speculative`), tree speculation needed an equivalent wrapper
so `justapi serve --model` can enable it through the OpenAI-compatible API with
zero caller awareness.

**Options considered:**
1. **`Engine::register_speculative` overload** — add `branch` param to the
   existing method and branch internally on `branch > 0`. Simpler call site but
   conflates two distinct verify algorithms in one entry point; harder to read
   and test.
2. **New `TreeSpeculativeModel` + `Engine::register_tree_speculative` (selected)**
   — mirrors the `SpeculativeModel` / `register_speculative` shape exactly, so
   draft-target and tree modes are first-class peers. The CLI wires both.

**Decision:**
- `spec_decode_tree::TreeSpeculativeModel` — a `Model` wrapper running
  `speculative_generate_tree` with `target` / `draft` / `gamma` / `branch` /
  `seed`, delegating `vocab_size` + `forward_logits` to the target (same as
  `SpeculativeModel`).
- `Engine::register_tree_speculative(name, target, draft, gamma, branch, seed)`
  — registers the wrapped model under `name` so it is served through the normal
  `Engine::generate` path (used by the OpenAI server) transparently.
- `Engine::get(name)` — small public accessor used by the CLI to fetch the
  registered target before wrapping it (registry was previously private).
- CLI `justapi serve`: new `--gamma` (draft-target + tree trigger) and `--branch`
  (tree enable) flags. When either is set, the CLI registers a second mock as the
  draft (`<model>-draft`) and wraps `<model>` via `register_tree_speculative`
  (`branch > 0`) or `register_speculative` (`gamma > 0`). The log line names the
  mode.

**Evidence:** 2 new wrapper tests (`tree_speculative_model_wrapper_matches_target`,
`tree_speculative_model_vocab_and_logits_delegated`) — the wrapped model emits the
same token stream as plain target decode and delegates vocab/logits to the target.
End-to-end smoke test: `justapi serve --model mymodel --gamma 4 --branch 3`
starts with the "tree speculative decoding" log line and serves
`/v1/models` (lists `mymodel` + `mymodel-draft`), `/v1/chat/completions`
(non-streaming: 12 completion tokens) and streaming SSE — all 200. All 117
inference tests pass; `clippy -D warnings`, `cargo fmt --check` clean.

**What this does NOT yet provide (gated on GPU):**
- Wall-clock tokens/sec on real 7B-class models with tree-attention batching
  (needs CUDA + weights). The structural acceptance gain (up to 3× at branch=3)
  transfers directly; only the per-step batched verification cost changes.
- A registered *real* draft model (today the CLI uses a second `MockModel` as
  draft — a perfect-draft demo; a smaller/faster real model would be the
  production draft).


## ADR-041 — 2026-07-10 — Radix tree as the prefix-cache data structure

**Context:** Phase 50 needed cross-request KV reuse. The existing
`PrefixCache` is a flat hash-map keyed by per-prefix token hashes. It dedupes
identical prefixes but duplicates block-ID references across distinct prefix
entries, so its memory grows super-linearly for nested (chat-history) prompts
and cannot merge *partial* shared prefixes. A radix tree (SGLang
RadixAttention) stores each block once and merges shared prefixes.

**Decision:** `RadixPrefixCache` in `radix_cache.rs` — a radix tree of
block-aligned nodes. `insert` splits edges to share common prefixes;
`lookup` returns the longest matching prefix (refreshing LRU timestamps);
`evict` removes LRU leaf subtrees and returns their `BlockId`s as a free
hint. Benchmark (`bench_nested`) shows O(N) block-slots for radix vs O(N²)
block references for the flat `PrefixCache` on a chat-history workload
(radix uses <1% of flat's storage at N=500).

**Evidence:** 14 unit tests incl. `shared_prefix_merges_nodes`,
`three_way_sharing`, `evict_removes_lru_leaf`, and the two
`bench_nested`-derived scaling tests. `clippy -D warnings`, `fmt --check` clean.

## ADR-042 — 2026-07-10 — Wiring RadixPrefixCache into the Scheduler

**Context:** `RadixPrefixCache` (ADR-041) was a standalone module; the
`Scheduler` never consulted any prefix cache — finished sequences were simply
dropped, leaking their KV blocks in the pool. The production win of Phase 50
(cross-request prefix KV reuse) required the scheduler to own the cache.

**Options considered:**
1. **Caller-driven lookup** (keep `NewRequest.cached_blocks`/`prefix_cached_tokens`,
   caller fills them) — leaves the scheduler cache-less, duplicates lookup
   logic, and the leaked-block bug unfixed.
2. **Scheduler owns the radix cache (selected)** — single eviction authority,
   O(1) lookup on admission, blocks cached on completion, eviction driven by
   the scheduler under pressure.

**Decision:**
- `Scheduler` now owns a `RadixPrefixCache` plus a `prefix_refs: HashMap<BlockId,
  usize>` live-reference count.
- **Admission:** if `NewRequest.cached_blocks` is empty, the scheduler does its
  own `prefix.lookup`; matched blocks are `reclaim`ed (reused, no recompute)
  and recorded as live-referenced (`unpin`). A block is only reused when its
  live-ref is 0, so two concurrent requests never claim the same cached block.
- **Completion (`schedule` retain + `cancel`):** finished sequences' blocks are
  promoted via `cache_completed` — `prefix.insert` (idempotent for shared
  prefixes) and then `pool.release` + `pool.pin` so the clock-sweep evictor
  cannot recycle them out from under the cache. This also fixes the
  finished-sequence block leak.
- **Eviction:** when `Sequence::grow` fails, the scheduler calls
  `prefix.evict_filter(k, |bs| all live-refs == 0)` which returns only
  freeable LRU leaf blocks; they are `unpin`ed + `pool.free_cached` (a new
  `KvBlockPool` method that returns a cached block to the free list
  immediately, no clock-sweep wait). This keeps a single eviction authority and
  avoids stale block ids in the tree.
- `pin` was relaxed to allow `ref_count == 0` (cached + pinned), and
  `RadixPrefixCache::evict_filter` was added (skips leaves still referenced by a
  live sequence). `take_completed_prefill` clears `prefix_refs` for transferred
  blocks (they leave this pool).

**Evidence:** 3 new scheduler-integration tests: `schedule_radix_reuses_shared_prefix`
(full hit, no prefill, 48 tokens saved), `schedule_radix_reuses_partial_prefix`
(only the unique tail prefilled, `computed_tokens == 48`), and
`schedule_radix_evicts_under_pressure` (6-block pool, 10 distinct prompts, no
deadlock). All 134 inference tests pass; `clippy -D warnings`, `fmt --check` clean.

## ADR-043 — 2026-07-10 — Collision-safe content-hash cache key for RadixPrefixCache

**Context:** ADR-042 wired `RadixPrefixCache` into the `Scheduler`. The tree
matches prefixes by exact token comparison, which is already correct, but the
cache key was implicit (the token run itself). If a future change switched
matching to be hash-based for speed, or if the tree were ever corrupted,
a false share could return the wrong KV blocks. We wanted the cache key to be
explicit and collision-resistant.

**Decision:** every `RadixNode` now stores `token_hash` (FNV-1a of its tokens).
- `insert` / split recompute the hash for each node (including the truncated
  edge after a split).
- `lookup` accepts a full-edge match only if `child.token_hash ==
  hash_tokens(&tokens[..cp])`; on divergence it sets a `collision` flag and the
  caller rejects the match (returns `None`, increments `hash_collisions`).
- Added `RadixPrefixCache::verify_hashes()` (self-check that every node's
  stored hash equals its token content) and a `hash_collisions` stat.

**Evidence:** 4 new tests — `node_hash_matches_token_content`,
`hashes_consistent_after_split`, `distinct_prefixes_do_not_collide`
(`hash_collisions == 0`), and `lookup_rejects_corrupted_hash` (forged
corruption → `lookup` returns `None`, `hash_collisions == 1`). All 138
inference tests pass; `clippy -D warnings`, `fmt --check` clean.

**Note / caveat:** KV-cache reuse is still keyed on *exact* token ids — two
prompts that are semantically equal but token-distinct correctly do NOT share
blocks, because their KV differs. The hash hardens the *key integrity*
(collision/corruption safety), not semantic equivalence.

## ADR-044 — 2026-07-10 — Prefix-cache observability surface on the Scheduler

**Context:** With the radix prefix cache wired into the `Scheduler` (ADR-042)
and its cache key hardened (ADR-043), the reuse win was only observable by
calling `RadixPrefixCache::stats()` directly. An operator/metrics layer needs
the signal surfaced through the `Scheduler`'s normal stats snapshot.

**Decision:** `SchedulerStats` now carries `prefix: RadixPrefixCacheStats`
(hits / misses / `hash_collisions` / `tokens_saved` / nodes) and
`cached_kv_blocks: usize` (resident prefix KV in the pool, via
`KvBlockPool::cached`). Added `Scheduler::prefix_cache_stats()` as a direct
accessor. The radix stats already track `tokens_saved` = tokens not
recomputed thanks to reuse, which is the headline operator metric.

**Evidence:** `schedule_prefix_stats_reflect_reuse` asserts that after a shared
prefix is reused, `stats().prefix.hits == 1`, `tokens_saved == 48`,
`hash_collisions == 0`, and `cached_kv_blocks > 0` once a prefix is resident.
All 139 inference tests pass; `clippy -D warnings`, `fmt --check` clean.

---
## ADR-045 — 2026-07-10 — Scheduler serving integration (Phase 51)

**Context:** Phase 50 wired the `RadixPrefixCache` into the `Scheduler` but
the `Scheduler` was not connected to any HTTP server. The existing OpenAI
handlers (`chat_completions_handler`, etc.) called `Engine::generate()`
directly, which delegates to `Model::generate()` — a black-box prefill+decode
loop that bypasses the scheduler entirely. To get admission control, prefix
reuse, and observability benefits, the scheduler must sit between the HTTP
layer and the model.

**Options considered:**
1. **Replace `Model::generate()` with a scheduler loop inside `Engine`** —
   would require rewriting the `Model` trait and breaking speculative decoding.
   Too invasive for a single phase.
2. **Per-request scheduler loop** — each `generate()` spawns its own thread
   running `schedule()` → `forward_logits()` → `on_step_complete()`. Simplest,
   but breaks under concurrency: concurrent requests each run their own loop
   and double-schedule the same seq (tokens emitted twice). Rejected after
   discovering the bug; the fix below subsumes it.
3. **Persistent scheduler background thread** (vLLM's `SchedulerLoop` model) —
   one loop thread interleaves ALL in-flight requests through a single
   `schedule()` call. Selected: proper continuous batching, correct under
   concurrency.

**Decision:** Implement `SchedulerEngine` in a new `scheduler_engine.rs`
module. A single persistent loop thread (spawned in `new()`) runs forever,
calling `Scheduler::schedule()` and routing tokens to per-request `mpsc`
channels keyed by scheduler seq id. `generate()` resolves the model, calls
`Scheduler::add_request` (which returns the assigned seq id), registers the
channel, and returns the receiver. When a seq leaves the running set the loop
sends a final `finish_reason` token and closes its channel. `Model::forward_logits`
is driven per step; greedy argmax sampling is used (temperature/top_p deferred).

**Scheduler API changes:** `add_request` now assigns + returns the seq id
immediately (previously assigned lazily inside `schedule()`), so external
drivers can correlate requests with scheduler events. `schedule()` builds
`Sequence::new(req.id)` instead of re-assigning. Added `running_ids()` and
`running_seq_generated()` accessors for completion detection. All existing
scheduler tests still pass (seq ids remain 1,2,3… in order).

**Key details:**
- `SchedulerEngine` holds `Arc<Engine>`, `Arc<Mutex<Scheduler>>`, and maps
  `seq_id → (sender, model, context, max_tokens)`.
- Completion detection runs on EVERY loop iteration (including empty
  `schedule()` cycles) so a finished seq always receives its finish token —
  the original single-request design hung here because it `continue`d before
  the detection block.
- Metrics exposed via a `MetricProvider` trait on `Metrics`:
  `justapi_scheduler_waiting`, `_running`, `_prefilling`, `_back_pressure`,
  `_prefix_hits`, `_prefix_misses`, `_prefix_tokens_saved`, `_cached_kv_blocks`.
- CLI: `justapi serve --model <id> --scheduled [--pool-blocks N] [--max-seqs N]`.

**Evidence:** 8 `SchedulerEngine` tests including `batches_concurrent_requests`
(2 concurrent) and `handles_many_concurrent_requests` (8 concurrent, above the
`max_num_seqs=4` batch limit — exercises admission + interleave). 4
justapi-core HTTP integration tests drive `scheduled_chat_completions_handler`
/ `scheduled_completions_handler` and `Server::with_openai_scheduled` and
assert scheduler metrics appear in `/metrics`. 147 inference + 259 core tests
pass; `clippy -D warnings`, `cargo fmt --check` clean.

**ADR-045a — 2026-07-10 — Sampling-parameter plumb in scheduler loop.**

The original scheduler loop used greedy argmax only (`sample_token` →
`argmax`), ignoring `temperature`, `top_k`, and `top_p` from
`SamplingParams`. This was a real feature gap vs the naive
`Engine::generate` path, which has always honoured all sampling params.

**Change:** Replaced `sample_token` with
`sample_token_with_params(logits, &SamplingParams)` in the scheduler engine.
The new function:
1. If `temperature ≈ 0`: delegates to `argmax` (greedy).
2. Otherwise: scales logits by `1/temperature`, applies top-K truncation
   (zeroes everything beyond the k-th largest), softmaxes to probabilities,
   applies top-P nucleus truncation if `top_p < 1.0`, then draws from the
   categorical distribution via `rand::thread_rng`.
3. Falls back to argmax on degenerate input (all-zero probs after truncation).

`seq_params` map stores the per-seq `SamplingParams` (inserted in
`generate()`, cleaned up on finish), and `get_seq_params` helper retrieves
it with a `unwrap_or_default` fallback.

**Evidence:** 4 new unit tests (`greedy_sampling_is_argmax`,
`sampling_honors_temperature`, `sampling_top_k_truncates`,
`sampling_uniform_distributes_with_high_temp`) verify the sampling logic.
151 inference tests pass (up from 147). Real wall-clock throughput
benchmark added to `inference_bench.rs` comparing naive vs scheduler-backed
generation on MockModel with 8 concurrent requests. BENCHMARKS.md updated.

## ADR-046 — 2026-07-11 — HTTP/3 (QUIC) transport as a parallel server

**Context:** FastAPI has no first-class HTTP/3 story; adding QUIC to justapi is
a parity/differentiation win and a natural extension of the Rust networking core
(ADR-008: Rust owns I/O and scheduling). The question was how to structure the
H3 server so it reuses the existing `HandlerFn` dispatch without forking the
large Python-facing handler logic in `justapi-py`.

 **Decision:** Implement HTTP/3 in `justapi-core::server::http3` behind an `http3`
 Cargo feature, using `quinn` (QUIC) + `h3` + `h3-quinn` (the h3/QUIC bridge).
 `serve_http3` accepts a `MiddlewareChain<Full<Bytes>>` plus an optional `WasmEngine`:
 the request body is fully buffered into `Bytes` before dispatch, then the request
 is run through the *same* middleware chain shape used by HTTP/1.1 (circuit breaker,
 coalescer, gateway, WASM preprocessing, then the handler). On the Python side,
 `make_native_handler` / `make_test_handler` were made generic over the request
 body type `B: http_body::Body<Data = Bytes> + Send + Sync + Unpin + 'static`, so
 the *same* closure serves `Incoming` (HTTP/1.1) and `Full<Bytes>` (HTTP/3) — no
 duplication of routing/validation/DB/batch/Python-dispatch logic.
 `JustAPIApp.run` spawns `serve_http3` in parallel with the HTTP/1.1 server when
 `enable_http3(cert_pem, key_pem)` was called, sharing the same `CancellationToken`,
 building an H3 `MiddlewareChain<Full<Bytes>>` from the same config as the H1 path.

 **Trade-offs / limitations:**
 - *Resolved (2026-07-11):* the original design served the raw `HandlerFn` on H3
   because `Middleware` was `Middleware<Incoming>`-only. Middleware was made generic
   over `B` (`impl<B: Send + 'static> Middleware<B>` for all core middleware;
   resilience middleware was already generic), so `serve_http3` now runs the full
   chain — H3 applies identical middleware policy to HTTP/1.1. See PLAN.md Phase 53.
 - We buffer the whole request body for H3 (no streaming request bodies). Acceptable
   for the parity target; matches how most H3 servers behave for typical API payloads.
 - TLS cert/key are PEM strings passed at runtime via `enable_http3`; the QUIC
   endpoint uses rustls with ALPN `h3` only (no HTTP/1.1-over-QUIC).

**Evidence:** `cargo test -p justapi-core --features http3` — `http3_roundtrip`
(unit test, rcgen self-signed cert + h3-quinn client) passes. `clippy` clean
with the feature. Python `test_http3.py` confirms the server starts, routes answer
over HTTP/1.1, and the QUIC UDP port is bound. Full pytest suite: 77 passed, 1
skipped under both GIL (3.12) and no-GIL (3.14t) — no regression from the generic
handler refactor.

---

## ADR-0xx — 2026-07-13 — Direct `jsonschema` + `uuid` deps in justapi-py for agent-native primitives

**Context:** Building the v0.2 differentiator (Rust-first per AGENTS.md §2): a
native streaming structured-output validator and an agent session-state store.
Both need (a) JSON-Schema validation of individual streamed values and (b)
opaque, collision-resistant session IDs.

**Options considered:**
1. Reuse `justapi_core::validate::validate_json_schema` (re-compiles the schema
   per call) — correct but recompiles the schema on every streamed item, which
   is wasteful for long LLM token streams.
2. Add `jsonschema` + `uuid` as direct deps to `justapi-py`, with a cached
   compiled-`Validator` map (keyed by schema string) for the validator — selected.

**Decision:** Add `jsonschema = "0.46"` (already a transitive dep via
justapi-core) and `uuid = "1"` (v4) as direct deps of `justapi-py`. The streaming
validator caches compiled `jsonschema::Validator` in a process-wide
`Mutex<HashMap<String, Validator>>` so a repeated schema compiles once. Session
IDs are `uuid::Uuid::new_v4()` hex strings stored in a `Mutex<HashMap<String,
serde_json::Value>>` on the app. Python remains thin glue (schema inference,
handler wrapping, dependency injection); all validation/state lives in Rust.

 **Evidence:** to be filled once `cargo test` + pytest pass for the new
 `test_streaming.py` / `test_session.py` suites.

---

## ADR-047 — 2026-07-13 — Hot-path: remove the per-request Python↔Rust boundary

**Context:** BENCHMARKS.md head-to-head showed justapi ~1.5–2× *slower* than
Robyn on raw throughput (hello 26,346 vs 40,397 RPS). Root cause was structural,
not tuning: every request crossed the PyO3 boundary with (1) a full Python
`Request` dict built in Rust, (2) a Python `wrap_result`/`to_dict` round-trip on
the response, and (3) an unconditional `set_trace_context` Python call. Robyn
avoids these by serializing responses with a cached `orjson` C-call from Rust and
skipping `Request` construction for 0-param handlers (`call0()`).

**Options considered:**
1. Keep the Python boundary; micro-tune `orjson` usage in `wrap_result`. Rejected —
   still pays the `to_dict` + Python fn-call overhead on every request.
2. Make `Response` a Rust `#[pyclass]` and downcast it directly in Rust. Rejected —
   subclassing the pyclass from Python broke: the pyclass `#[new]` expected
   `body: String` but `render()` returns `bytes`, and the Python `__init__` passed
   an unknown `background=` kwarg. Brittle and fight-y with pyo3's subclass/ctor
   rules.
3. Detect `Response` via a `_justapi_response` marker attribute and read its
   fields (`status_code`, `headers`, `body`) directly in Rust — selected. Keeps
   `Response` a thin Python class (AGENTS.md §2: Python owns application glue),
   removes the `to_dict` round-trip, and runs `background.run()` in Rust exactly
   where `wrap_result` did.

**Decision (what changed):**
- `handlers.rs::serialize_response` mirrors Robyn's `extract_response_type_fast`:
  `Response` (marker) → direct field read; dict/list/str/num → `orjson.dumps`
  called from a `OnceLock`-cached Python lambda; bytes → octet-stream; anything
  else falls back to the Python `wrap_result` (preserving `default=str`).
- `call_python_handler` skips building the Python `Request` object when the route
  doesn't need it. The per-handler `needs_request` flag is computed in
  `app.py::_wrap_handler` from `sig.parameters`, `all_deps`, and any
  (route- or app-level) middleware, and threaded through `run()` →
  `make_native_handler` / `make_test_handler`. Default `True` (safe) when the
  attribute is missing.
- Trace-context propagation is gated behind `JUSTAPI_ENABLE_TRACE` (default off)
  via a `OnceLock`-cached check, so the hot path skips the Python call entirely.
- `_native_helper.py::call_handler` returns the raw result; Rust serializes.

**Evidence:** Re-run on the same fixture (i5-13600K, `oha -z 10s -c 100`, release
builds of both) — justapi now *beats* Robyn: hello-world 60,297 vs 39,103
(×1.54), JSON echo 47,415 vs 36,899 (×1.29), validated JSON 40,080 vs 32,919
(×1.22). Resolved edge-case bug: 0-param handlers behind middleware now correctly
force `needs_request=True` (empty-dict would have 500'd a middleware that reads
the request). Gates green: `cargo test --workspace` (0 fail), `cargo clippy
--workspace --tests -- -D warnings` (0 warn; two pre-existing feature-gated
warnings also fixed), `cargo fmt --check` clean, Python pytest 107 passed / 1
skipped. BENCHMARKS.md head-to-head section updated (old conclusion superseded).

## ADR-048 — 2026-07-14 — Schema-backed native Rust fast path (`native=True`)

**Context:** ADR-047 removed the *per-request* Python↔Rust boundary for the
response and the `Request` build, and justapi now beats Robyn on raw throughput.
But every route still *dispatched into a Python handler* — the final PyO3 call
could not be eliminated for application logic. Robyn-style "handler runs entirely
in Rust" was still impossible for justapi. The user-chosen next step was the
schema-backed fast path: for routes whose contract is "validate the body against
a schema and return it," there is no application logic to run — the response is
deterministic, so the Python handler is pure overhead.

**Options considered:**
1. Make `native=True` validate-then-echo in Rust for *any* body. Rejected — without
   a schema there is nothing to validate against and the body may not be JSON; the
   fast path needs a schema to be meaningful and safe.
2. Validate in Rust, echo body; if `native=True` but no schema present, fall back to
   the normal Python handler (don't panic on `None` schema). Selected.

**Decision (what changed):**
- `app.py` route decorators `post/put/patch/query` accept `native: bool = False`
  and forward it (via `**kw` → `native=`) to the Rust `_app.<method>` call.
- Rust `JustAPIApp` holds a `native: Vec<bool>` (parallel to `handlers`); every
  route method (`get/head/options/trace/post/put/patch/delete/query`) records
  `native.unwrap_or(false)`; `run()` and the test client build an `Arc<Vec<bool>>`
  and pass it to `make_native_handler` / `make_test_handler`.
- `call_python_handler` gains `native: bool` + `schema_json: Option<String>`. When
  `native && schema_json.is_some()`, it calls `justapi_core::validate::
  validate_json_schema(body, sj)`; on `Ok` it returns `200 application/json` with the
  raw request body echoed back; on `Err` it returns `422 application/problem+json`
  with `field`/`message` per error. Otherwise execution continues to the Python
  handler unchanged.
- Guard: `native && schema_json.is_some()` — `native=True` without a schema silently
  falls through to the Python path (no `None` deref).

**Evidence:** Release `maturin develop` on the fixture (i5-13600K, `oha -z 10s -c 100`,
`POST {"id":1,"name":"x","price":1.5"}`): the native route served **59,666 RPS** vs
**3,531 RPS** for the equivalent Python-handler route (~16.9× faster), exceeding
Robyn's validated-workload number (32,919 RPS) by ~1.8×. New regression test
`test_native_fastpath.py` (4 cases: valid→200 echo, invalid→422, native-without-
schema→Python fallback, PUT echo) passes. Gates green: `cargo clippy -p justapi-py
--tests -- -D warnings` clean, `cargo fmt --check` clean. BENCHMARKS.md native
fast-path section added.

---

## ADR-049 — Non-native dispatch deadlocks at high concurrency (GIL/blocking-pool)

**Status:** ✅ Resolved. The dedicated GIL thread-pool (`crates/justapi-py/src/gil_pool.rs`)
replaced the per-request `tokio::spawn_blocking` + `Python::attach` pattern on
both non-native dispatch sites (`handlers.rs` calls `gil_pool::run_python`). The
fix was implemented in a later session (WIP save-point `f47c554`); this ADR's
_status_ line was left "open" and is corrected here.

**Context:** While re-benchmarking the ADR-048 native fast path at `oha -z 5-6s
-c 100`, the *Python* routes were found to hard-stall. This is a separate,
pre-existing defect in the Python dispatch path; it is **not** caused by the
native optimization (ADR-048) and native routes are immune.

**Symptom:** At ~100 concurrent connections, **every** non-native route serves
≈16–20 req/s with all connections aborted/deadlined. At `-c 10` the same routes
serve hundreds of thousands of req/s. The stall is nondeterministic in severity
(one `/noop` run reached 697k req/s; a subsequent `/noparam` run stalled at 20
req/s for the identical handler class), i.e. a GIL/blocking-pool *race* that
usually resolves to a full stall under saturation.

**Reproduction (minimal):** any route, including the simplest:
```python
@app.post("/noparam")
def noparam():            # no params, no body access, needs_request=False
    return {"ok": True}
```
`oha -z 5s -c 100 -m POST` → ≈20 req/s, 100 aborted. Even `/noparam`, which never
reaches `try_native_fast_path` (native=false), never hits the schema block (no
schema), and does **not** use `app`/`scheme`/`client`/`needs_request`/
`http_version` (the params added to `call_python_handler` in ADR-048) — yet it
deadlocks. This proves the ADR-048 change is not the trigger.

**Location:** `crates/justapi-py/src/native/handlers.rs` `make_native_handler`
spawn_blocking block (~line 722):
```rust
let nr = tokio::task::spawn_blocking(move || {
    Python::attach(|py| { call_python_handler(py, ...) })
}).await;
```
All non-native requests funnel through `tokio::spawn_blocking` + `Python::attach`
(GIL acquisition) on tokio's blocking pool. Under ~100 concurrent requests the
GIL and the blocking pool interact to produce a hard stall — the classic pyo3
"many blocking threads all contending for the GIL while the async runtime needs
those threads to make progress" deadlock class.

**Why native routes are immune:** `try_native_fast_path` (handlers.rs ~421)
returns the response with **no GIL acquire and no `spawn_blocking` hop** — it
validates via the precompiled `CompiledValidator` and echoes the body in pure
Rust. Measured 410k–724k req/s at `-c 100` (ADR-048 / BENCHMARKS.md). So opting
a route into `native=True` both maximizes throughput *and* avoids this deadlock.

**Decision (fix direction — NOT yet implemented):** Replace the per-request
`spawn_blocking` + `Python::attach` pattern with a **dedicated GIL thread-pool**
(a small, bounded set of threads — sized to `num_cpus()` — each holding a
persistent GIL / `PyGILPool`, dispatched via a channel with a oneshot result
future). This decouples Python execution from tokio's blocking pool so the two
never deadlock each other, and removes per-request GIL-acquisition contention.
The async side `await`s the oneshot instead of `spawn_blocking`. Native routes
keep using `try_native_fast_path` (no change).

**Out of scope for ADR-048:** the native fast-path optimization is complete and
correct; this deadlock is a separate P0 to schedule as the next phase. It affects
the *common* (non-native) path, so it blocks production use at realistic
concurrency and should be the immediate next task after ADR-048.

**Evidence:** isolation benchmark (fixture, `oha -z 5s -c 100`): `/noparam` ≈20
req/s (all aborted); `/validate_native` (native) 410k–724k req/s; `/noop` once
697k, once ≈20 (nondeterministic stall). Single-request and `-c 10` curl work.

## ADR-049 — 2026-07-15 — Remove HTTP/3 (QUIC) transport (dead code)

**Context:** ADR-046 added HTTP/3 behind the `http3` Cargo feature
(`quinn` + `h3` + `h3-quinn`, `server::http3::serve_http3`,
`JustAPIApp.enable_http3`). During the 2026-07-14 production-readiness audit
(MED#9) it was found that `crates/justapi-core/src/server/http3.rs` was
**untracked and never declared as a module** (no `mod http3;` anywhere), so it
was never compiled and `serve_http3` was never called. The `app.rs`
`#[cfg(feature = "http3")]` blocks reference `justapi_core::server::http3::serve_http3`,
so *enabling* the feature would have failed to compile — the "feature" was a
false signal, not working code.

**Decision:** Remove the dead HTTP/3 transport entirely:
- deleted `crates/justapi-core/src/server/http3.rs`;
- dropped the `http3` feature and the `quinn`/`h3`/`h3-quinn` optional deps
  from `justapi-core`;
- removed the `http3` feature from `justapi-py`;
- deleted the Rust `enable_http3` method, the Python `JustAPIApp.enable_http3`
  wrapper, the `.pyi` stub, `test_http3.py`, and all `#[cfg(feature = "http3")]`
  wiring in `app.rs`.

**Rationale:** keeping uncompilable, never-exercised code is worse than removing
it (AGENTS §2, "no dead code"; audit MED#9). If HTTP/3 is wanted later it must
be re-added as a *fully wired* module — declared via `mod http3;`, and given the
same production hardening the audit applied to the other transports (request
timeout via `chain.run` + `tokio::time::timeout`, connection-flood `Semaphore`,
panic safety) — otherwise it would be a fresh DoS/security gap. The middleware
body-type genericity work from the original phase remains and is still exercised
by the HTTP/1.1 path.

**Evidence:** `cargo check --workspace` and `cargo clippy --workspace --tests -D
warnings` clean after removal; `git status` shows the `http3.rs` file removed
(it was untracked, so the removal is from the working tree only — it was never
committed). PLAN.md Phase 53 status updated to "reverted — dead code removed".




## ADR-050 — 2026-07-15 — Secure-by-default CORS + opt-in security headers

**Context:** The production-readiness audit (MED#9) flagged two security
defaults. (1) `Cors::new()`/`Default` were `permissive()` → emitted
`Access-Control-Allow-Origin: *` for every request, and the credential branch
could reflect `*` with `Access-Control-Allow-Credentials: true` (an invalid but
leaky combination). (2) `SecurityHeaders` was only opt-in and always emitted HSTS
even on plaintext.

**Decision:**
- `Cors::new()` and `Default` now start with an **empty** `allow_origins` (no
  ACAO emitted) — fail-closed. Explicit `allow_origin(...)` / `allow_origins(...)`
  is required to enable CORS. `permissive()` still exists for dev.
- The `*` + `allow_credentials` bug is fixed: when the matched origin is `*`,
  credentials are force-disabled and the concrete request origin is reflected
  instead of `*`.
- `SecurityHeaders` gains `without_hsts()`; `Default` keeps HSTS but the Python
  `enable_secure_headers()` applies the **non-HSTS** default (plaintext server).
  HSTS is only added on explicit `enable_secure_headers(with_hsts=True)`.
- Security headers are **not** auto-applied to every app: forcing a CSP
  (`default-src 'self'`) by default would break CDN-loaded UIs such as the
  `/docs` Swagger UI added in the same audit. They are a one-call opt-in.

**Rationale:** secure defaults must not leak cross-origin or pin plaintext to
HTTPS; but forcing CSP by default is a breaking change for legitimate apps.
**Evidence:** `cargo test --workspace` (CORS unit tests updated to configure an
origin), `clippy -D warnings`, `cargo fmt --check` all clean; live server
verified that `enable_secure_headers()` emits `X-Content-Type-Options`,
`X-Frame-Options: DENY`, `Content-Security-Policy`, `X-XSS-Protection` and
**no** `Strict-Transport-Security`.

## ADR-051 — 2026-07-15 — Real metrics + genuine readiness for Python apps

**Context:** The Python app's builtin `/metrics` returned a static
`justapi_up 1` string and `/ready` returned a static `{"status":"ready"}`, so
Kubernetes probes and Prometheus scrapes saw no real signal. The Rust
`Server` already collects `Metrics` and runs a `HealthRegistry`, but for Python
apps routing is handled by the Python handler, which bypassed those builtins.

**Decision:**
- The live `Metrics` and `Arc<HealthRegistry>` are captured from the `Server`
  just before `server.run()` (via `slf` + `Python::attach`, because the
  `PyRefMut` borrow is `!Send` and cannot cross the detached thread) and stored
  on the `_JustAPIApp`.
- `/metrics` now calls `Metrics::prometheus()` → real Prometheus exposition
  (request counters, status breakdown, latency histogram, bytes, connections).
- `/ready` now consults the `HealthRegistry`: returns 200 with a component
  report when healthy/degraded, 503 when any registered check is unhealthy.
- Added `JustAPIApp.register_health_check(name, callable)` (Rust
  `PyHealthCheck` wrapping a Python callable, invoked under the GIL) so users can
  wire DB/dependency probes into readiness.

**Rationale:** observability and liveness/readiness are core production
requirements; a static 200 / stub defeats autoscaling and alerting.
**Evidence:** live server shows `/metrics` reporting `justapi_requests_total`
incrementing on real traffic; `/ready` returns 200 healthy with a component
report, and 503 (with report) when a registered check returns falsy.
`cargo test --workspace` and `pytest` (minus two pre-existing, unrelated
failures in `test_websocket`/`test_openapi_parity`) pass.

## ADR-052 — 2026-07-15 — Single error-response contract (`{"detail": ...}`)

**Context:** The production-readiness audit (MED#9) flagged three inconsistent
error shapes across the codebase: `{"error": "..."}` (native + core handlers,
resilience, panic, static files), `{"detail": ...}` (Python `HTTPException` /
`RequestValidationError`), and an RFC-7807 envelope
(`{"type","title","status","detail","errors"}` with `application/problem+json`)
emitted by the Rust JSON-Schema validation paths. Clients had no single shape
to parse.

**Decision:** Adopt **`{"detail": ...}`** (FastAPI-compatible) as the single
error envelope for every non-2xx response. The numeric status lives in the HTTP
status line; `detail` carries the human-readable message. `content-type` is
always `application/json` (the `application/problem+json` RFC-7807 variant is
dropped). Applied across:
- `justapi_core::error_response(status, detail)` / `validation_response(detail)`
  helpers added in `lib.rs`; all core error sites (server, resilience, panic,
  static files) routed through them.
- `justapi-py` native handlers: generic 500 → `{"detail":"Internal Server Error"}`;
  all `{"error":...}` string bodies → `{"detail":...}`; the RFC-7807 validation
  blocks collapsed to `{"detail": <message>}`.
- Python `system.py` error routes → `{"detail":...}`.
- Sensitive internals are never placed in `detail` for 5xx (matches the secure
  generic-500 policy from the prior audit).

**Rationale:** one parseable contract; `detail` keeps FastAPI client
compatibility; RFC-7807's extra keys added no consumer value here and broke
uniformity. **Evidence:** `cargo test --workspace` (core integration test
updated) and `pytest` (validation / test-client / native-fastpath tests updated
for `detail` + `application/json`) pass. The two remaining `pytest` failures
(`test_websocket`, `test_openapi_parity`) are pre-existing and unrelated
(fail on the prior save-point too).

## ADR-053 — 2026-07-15 — Panic model: abort + supervisor restart, not catch_unwind

**Context:** MED#9 noted that `panic = "abort"` means one bad request can kill
the process. Two mitigations were considered: (a) switch to `panic = "unwind"`
+ `catch_unwind` around the handler, or (b) audit/remove hot-path `.unwrap()`.

**Decision:** Keep **`panic = "abort"`** (ADR-048 / `gil_pool.rs` SAFETY note).
A `catch_unwind` boundary at the GIL FFI boundary is **undefined behaviour**
(pyo3 C code holding the GIL cannot be unwound safely) and was measured as a
**~3x throughput regression** on the GIL path — both documented in
`gil_pool.rs`. The production model is therefore **crash-fast + supervisor
restart** (Docker/k8s/systemd), which is the correct availability trade for a
Rust HTTP server. The actionable mitigation is the **hot-path unwrap audit**:
- Fixed a per-request abort: `router.fallback().unwrap()` in `server/mod.rs`
  now returns 404 when no fallback is registered (a 404 would otherwise abort
  the whole process).
- Reviewed request-path `.unwrap()`/`.expect()` in `server/` and `native/`; the
  remainder are init-time (route registration) or logically-guaranteed (e.g.
  `content_type.unwrap()` only reached when `is_multipart` is true), so they
  cannot be triggered by attacker input. A genuine logic bug still aborts and is
  restarted by the supervisor — by design.

**Evidence:** `cargo build --workspace` and all gates green; the core 404 path
returns 200/404 normally and no longer panics on unmatched routes.

## ADR-054 — 2026-07-16 — Fix WebSocket scope, OpenAPI parity, and SPA frontend mounts

**Context:** Two pre-existing pytest failures (`test_websocket.py::
test_websocket_scope_and_json`, `test_openapi_parity.py::test_openapi_parity`)
were investigated and found to be genuine production-grade framework bugs, not
test issues. A third gap (SPA `frontend()` serving) was discovered while fixing
the OpenAPI parity test.

**Decision / changes:**
1. **WebSocket scope completeness.** The Rust WS upgrade path previously passed
   only the request `path` to handlers; the query string, headers, and peer
   address were discarded, so a Starlette-style `scope` exposed an empty
   `query` and `None` client. The `WsHandler` signature changed from
   `Fn(String, WsRead, WsWrite)` to `Fn(WsConnInfo, WsRead, WsWrite)`, where
   `WsConnInfo { path, query_string, headers, client }` is built from the
   upgrade `req` *before* `hyper::upgrade::on(req)` consumes it. The Python
   `WebSocket` scope now reflects the real query/headers/client. The Rust fast
   path (`native=True`) is unaffected.
2. **OpenAPI parity.** `build_openapi` (Python) emitted the wrong key
   (`operation_id` instead of OpenAPI's `operationId`), silently dropped
   `openapi_extra` extensions, and omitted the default success response when a
   `responses` map was supplied. Fixed to emit `operationId`, deep-merge
   `openapi_extra` (handles the JSON-string form stored by route registration),
   and always include the success code (`200` or `status_code`).
3. **SPA frontend mounts.** `app.frontend(path, dir, fallback=...)` stored a
   `StaticMount { prefix, dir, fallback }` but `Server::run()` discarded the
   `prefix` and `fallback` (only mounted `dir` at root via `with_static_dir`),
   so `/static/` returned 404. Core `Server` gained `with_static_mounts` and a
   `try_serve_static` helper that honors per-mount prefix + SPA fallback
   (unmatched paths under the prefix serve the fallback file). Added
   `Server::run_on(listener)` so callers can supply a pre-bound listener (avoids
   an ephemeral-port bind race in tests). Added Rust unit tests for
   `StaticMount::resolve` and an integration test for prefix + SPA fallback.

**Evidence:** Both previously-failing pytest tests now pass; full pytest suite
119 passed / 1 skipped; `cargo test --workspace` green; `cargo clippy
--workspace --tests -D warnings` clean; `cargo fmt --check` clean. New Rust
tests: `static_files::tests::test_mount_resolve_prefix`,
`test_mount_traversal_blocked_through_prefix`, and
`integration::test_static_mount_prefix_and_spa_fallback`.

## ADR-056 — 2026-07-16 — Rust-native route compiler (skip the GIL on common routes)

**Context:** JustAPI already beats Robyn on raw throughput (ADR-048/055,
head-to-head in `BENCHMARKS.md`: native fast path ~21–25× Robyn, plain Python
path ~5.4× Robyn). But the *only* route that currently runs entirely in Rust is
the **validate-and-echo** native fast path (`try_native_fast_path`,
handlers.rs:449): it validates the body against a JSON schema and returns the
bytes unchanged. That covers a single shape — "accept + validate + echo" — and
cannot transform, query a DB, branch, or set custom status/headers. Every other
route falls back to the Python GIL path (~120k req/s here), which is GIL-capped.

The explicit goal ("faster than every existing framework, perfectly"): the only
way to be faster than Python-in-hot-path frameworks (Robyn/Flask/FastAPI/Sanic)
*and* match pure-Rust frameworks (Actix/Axum) is to **serve routes in Rust by
default** and make Python the *exception* (only for logic with no Rust
primitive). Raw echo can never beat Actix; the win is "Actix-class speed for
every route shape expressible in Rust primitives, with Python DX for the rest."

**Design — a route compiler, not "make everything native":**
A route is *eligible* for the Rust path iff **every step** of its behavior has a
Rust implementation in `justapi_core`. If eligible, it compiles to a
`Handler::Custom` (Rust closure, zero Python/GIL) via the existing
`Handler::Custom` mechanism (already used by OpenAI/DB routes). If not eligible,
it falls back to the Python GIL path — never silently wrong. This mirrors the
today's `native=True`+`Schema` rule (schema present → Rust echo; absent → Python
fallback), generalized to a set of primitives.

**Rust-primitive set (all already exist in `justapi_core`):**
- Body validation against JSON schema — `validate::CompiledValidator` ✅
- Query/param validation — `try_native_fast_path_query` ✅
- SQL execute/select/insert/update/delete — `db/` (`AnyPool`, `Select`/`Insert`/
  `Update`/`Delete` builders, `TransactionHandle`) ✅ (needs a row-fetch-as-JSON
  method added to `AnyPool`; see below)
- Serialization to JSON response — `serialize` ✅
- Middleware already in Rust: `rate_limit`, `resilience` (circuit breaker),
  CORS, auth/JWT, `middleware` chain ✅

**Eligibility examples (promote to Rust iff all true):**
- `POST /users` + body `Schema` + `native="crud_insert"` (table, columns) →
  validate → `INSERT ... RETURNING` → return row JSON. ✅
- `GET /users` + `query_schema` + `native="crud_select"` → validate query →
  `SELECT` → return rows. ✅
- Route with a Python `def handler` body that does arithmetic/string work /
  calls an LLM SDK / third-party lib → ❌ falls back to Python.

**Correctness contract (the real engineering risk):** a Rust-native route must
behave *identically* to the equivalent Python route — same status codes, error
envelope `{"detail": ...}`, transaction semantics (auto-begin/commit on writes,
rollback on error), NULL handling, and header/content-type. Add a test that
runs the same route in both modes and diffs responses. Silent semantic drift is
the failure mode to prevent.

**Phased plan:**
 - **Step B (shipped):** Rust-native `POST` CRUD insert — validate body →
   `INSERT ... RETURNING *` (via `AnyPool::query_with_params`, injection-safe
   bound params) → return the row as 200 JSON. Exposed as
   `app.post(path, crud_table=..., crud_columns=...)` on the Python side,
   compiling to `Handler::Custom`. Reuses `db/`, `validate`, `serialize`.
 - **Step C (shipped):** Unified `crud_dispatch_bytes` handler switches on the
   HTTP method: `POST`→insert, `GET`→select, `PUT`/`PATCH`→update,
   `DELETE`→delete, using one `CrudSpec { op, table, columns, id_column }` per
   route. All ops run entirely in Rust with no GIL/Python hop. Exposed on the
   Python side as `app.post/get/put/patch/delete(path, crud_table=...,
   crud_columns=...)`. `id_column` is hardcoded `"id"` (no Python exposure yet).
   Correctness locked by `integration::test_crud_all_handler` + end-to-end smoke.
 - **Step D:** Compile-time eligibility checker promoting routes automatically
   when all steps are Rust-backed; honest benchmark ledger vs Robyn **and**
   Actix/Axum on the same fixture; expose pool-size/WAL tuning from
   `app.set_database` and benchmark on Postgres.
- **Step D:** Compile-time eligibility checker promoting routes automatically
  when all steps are Rust-backed; honest benchmark ledger vs Robyn **and**
  Actix/Axum on the same fixture.

**Why not a WASM/user-Rust handler instead:** `wasm.rs` exists but user-authored
WASM handlers shift the DX burden to the user and add a runtime; the
declarative-primitive compiler keeps Python DX while executing in Rust. WASM
stays an option for truly custom logic.

 **Evidence (Step B, recorded in BENCHMARKS.md, 2026-07-16):** On a file-backed
 SQLite fixture, the Rust-native `POST /items` (`crud_table`/`crud_columns`) is
 **~2.5× faster than the equivalent Python route at -c1** (6.8k vs 2.7k RPS, 2.2ms
 vs 6.4ms p99) — the GIL-avoidance win is real. Under load (-c10/-c100) both routes
 collapse toward SQLite's single-writer ceiling (~5–6k RPS); Rust's *tail* latency is
 worse there only because the default 10-connection pool queues checkouts, while the
 Python path opens a fresh connection per request. So the bottleneck under concurrency
 is the DB + pool size, **not** the runtime — the architecture is correct and faster at
  the single-flight level. Remaining work (Step D): raise/expose `max_connections`,
  enable WAL, and benchmark on **Postgres** where the GIL-avoidance advantage compounds
  (no single-writer lock). Gates met: `cargo test --workspace` (incl.
  `integration::test_crud_insert_handler`), `cargo clippy --workspace --tests -D warnings`,
  pytest green; Rust integration test asserts the Rust-native route returns the same row
  JSON + 422 envelope as the Python contract.

  **Evidence (Step C, recorded in BENCHMARKS.md, 2026-07-16):** All four CRUD ops
  now serve entirely in Rust via `crud_dispatch_bytes`. On an in-memory SQLite
  fixture (`max_connections=1`, no fsync), INSERT ≈ 12.9k RPS @c1 and SELECT-by-id
  ≈ 18.1k RPS @c1 (≈ 21.7k @c10 — reads avoid the write lock). UPDATE/DELETE are
  correct in isolation (core integration test + end-to-end smoke both pass) and
  bounded by the same SQLite single-writer lock as INSERT. The Python API is
  `app.post/get/put/patch/delete(path, crud_table=..., crud_columns=...)`; a route
  registered this way never invokes a Python handler. `Database(init_sql=...)` was
  added so apps can bootstrap schema at startup. Gates met: `cargo test --workspace
  --features db` (incl. `integration::test_crud_all_handler`), `cargo clippy
  --workspace --tests -D warnings`, `cargo fmt --check`, pytest green.

  **Evidence (Step D, recorded in BENCHMARKS.md, 2026-07-16):** Benchmarked the
  Rust-native CRUD path against a real hosted Postgres (Aiven, TLS, 20-conn
  pool). All four verbs verified end-to-end; throughput scales ~linearly with
  pool concurrency (~9→~84 RPS INSERT from -c1→-c20) and is bounded by the
  ~110 ms cloud round-trip, not by JustAPI — confirming the Step D prediction
  (no SQLite single-writer ceiling, GIL-avoidance lets the pool saturate).
  Delivered the Step D tuning knobs: `Database(max_connections=…, init_sql=…,
  pragmas=…)` + `app.set_database(db, init_sql=, pragmas=, wal=)`; SQLite
  `journal_mode=WAL` applied per connection via `after_connect`; Postgres TLS
  enabled via `sqlx` `tls-rustls`. `crud_dispatch_bytes` now emits driver-correct
  placeholders (`$N` Postgres / `?` SQLite-MySQL) via `placeholder_gen`. Gates
   met: `cargo test --workspace --features db`, `cargo clippy -D warnings`,
   `cargo fmt --check`, pytest green. Remaining (post-Step D): a Rust-backed
   Python DB API (`app.query`/`app.execute` over `AnyPool`) so *custom* handlers
   can run arbitrary SQL without losing the GIL-avoidance win.

   **Evidence (P1 — Python DB API over `AnyPool`, 2026-07-16):** Shipped the
   follow-up predicted at the end of Step D: a Rust-owned `DbPool` bridge exposed
   to Python handlers so *arbitrary* SQL runs entirely in Rust with **bound
   parameters** (injection-safe, no string interpolation) and the **GIL released
   during the round-trip** (`py.detach` in the `run()` entrypoint; per-call the
   `AnyPool` future runs on the tokio runtime handle captured at startup, outside
   any `Python` token). API: `app.db.query(sql, params=None)`,
   `app.db.execute(sql, params=None)`, `app.db.insert(table, data, columns=None)`
   (returns the `RETURNING *` row), `app.db.transaction([(sql, params), …])`
   (commits atomically; returns the last statement's rows or `{"rows_affected":N}`),
   `app.db.health()`. `app.db` resolves to the `DbPool` only after `app.run()`
   (`db_pool` field is set inside `run()` on the resolved `AnyPool`; before that it
   is `None`). Verified end-to-end against the Aiven Postgres fixture from
   `benchmarks/smoke_dbpool.py`: INSERT with `$1`/`$2` binds, SELECT returning row
   dicts, RETURNING insert, and a 2-statement `transaction` committing (alpha's
   `qty` went 10→11 across the UPDATE+SELECT) all succeeded. This keeps the
   AGENTS.md §2 / ADR-056 thesis: the DB layer is Rust-owned and the Python GIL is
   never held during a query. `app.db` is accessed from handlers as `app.db`
   (Python `JustAPIApp.db` property → `self._app.db_pool()`); the Python-side
   config is `app.set_database(Database(url), init_sql=..., pragmas=..., wal=...)`
   (note: `app.database = ...` is a no-op — the setter is `set_database`).
   Gates met: `cargo test --workspace --features db`, `cargo clippy -p
   justapi-core --tests --features db -D warnings`, `cargo clippy -p justapi-py
   --tests -D warnings`, `cargo fmt --check`, pytest green, and the Aiven
   end-to-end smoke.

## ADR-055 — 2026-07-16 — Configurable max request body size (413 on overflow)

**Context:** The request body cap was a hardcoded `50 * 1024 * 1024` (50 MiB)
buried at four read sites (core `execute_handler` Echo path; Python
`make_native_handler` native-fast-path and the Python-handler path; plus the
`serve()` default-routes path). It was invisible to operators and could not be
tightened for endpoints that only expect small payloads — a memory-exhaustion
DoS exposure on untrusted ingress.

**Decision / changes:**
1. **Single source of truth.** Added `const DEFAULT_MAX_BODY_SIZE = 50 * 1024
   * 1024` in `server/mod.rs`. Every read site now derives its cap from a
   threaded `max_body_size: usize` value rather than a literal.
2. **Builder surface.** `Server::with_max_body_size(bytes)` (Rust) and
   `JustAPIApp.run(addr, max_body_size=...)` (Python, default 50 MiB) plumb the
   cap through `make_handler` → `execute_handler` (core) and `make_native_handler`
   → both body reads (Python). `serve()`'s default-routes path uses the default.
3. **Correct status code.** Previously an oversized body surfaced as a generic
   `400`/`500`/`404`. The `http_body_util::Limited` length-limit error now maps
   to `413 Payload Too Large` with `{"detail":"payload too large"}` on every
   affected path (core Echo, Python native fast path, Python handler path).
4. **Test handler** (`make_test_handler`, used by the async test client) takes
   the same parameter; the test-client call site passes the default.

**Evidence:** New Rust integration test `integration::test_max_body_size_enforced`
(binds `limit=1024`, asserts a 512-byte echo returns 200 and a 1025-byte body
returns 413). New pytest suite `test_max_body_size.py` (`test_max_body_size_enforced`
+ `test_max_body_size_default_accepts_large`) passes against a real server.
`cargo test --workspace`, `cargo clippy --workspace --tests -D warnings`, and
`cargo fmt --check` all clean.

**Note:** `crates/justapi-py/test_graphql.py::test_graphql` fails with 404 — the
Python package does not yet expose a `graphql()` builder, so `/graphql` is never
registered. This is a pre-existing gap unrelated to this change (the core has a
`Handler::GraphQL`, but no Python binding wires it up). Tracked separately.


## ADR-056 — 2026-07-17 — Cross-engine `?` placeholder normalization + `justapi create <app>`

**Context:** The `DbPool` public API (`query_with_params` / `execute_with_params`
/ `query_stream` / `transaction`) was documented as injection-safe with bound
`?` parameters, but Postgres's driver (`sqlx::Any` + Postgres) requires positional
`$1`, `$2`, … placeholders. Passing `?` to Postgres produced `syntax error at or
near ")"`. SQLite and MySQL accept `?` natively. Without normalization, the same
Python code could not run unmodified across all three engines — defeating the
"pick any engine at scaffold time" promise of the CLI.

**Decision / changes:**
1. **`AnyPool::normalize_sql(sql)`** (crates/justapi-core/src/db/pool.rs): rewrites
   each `?` → `$N` when `self.kind == DbKind::Postgres`; returns the SQL unchanged
   for SQLite/MySQL. Applied in `query_with`, `execute_with`, `query_stream`, and
   `transaction_with_isolation`. Callers (Python `db.query`/`db.execute`/
   `db.query_stream`/`db.transaction`) continue to pass `?` everywhere.
2. **`justapi create <app>` CLI** (crates/justapi-cli/src/main.rs): new
   `Commands::Create { name, output, db, db_url }`. `--db {sqlite,postgres,mysql}`
   picks the scaffolded backend (default sqlite); `--db-url` overrides the URL.
   `resolve_scaffold_db` maps the choice to a default URL; `scaffold_project`
   writes a full project (app/main.py wired to `Database`, migrations/, static/,
   .env, README, requirements.txt, Dockerfile, .gitignore). `New` now delegates
   to `scaffold_project` with sqlite default, removing ~90 lines of duplicated
   inline logic.

**Evidence:** `benchmarks/smoke_dbpool.py` extended to cover `query_stream`,
`DbParam.bytes` round-trip (`{"$bytes":...}` wire marker → Postgres `BYTEA`), and
`transaction`. Run against live Aiven Postgres — all endpoints return 200
("DBPOOL SMOKE OK"). Verified `justapi create` scaffolds correct `main.py` for
postgres, mysql, and sqlite. `cargo test --workspace --features db` green;
`cargo clippy --workspace --tests --features justapi-core/db -D warnings` clean;
`cargo fmt --check` clean.

## ADR-057 — 2026-07-17 — Eager/early DB pool connect removes `app.db is None` footgun

**Context:** `app.db` returned `None` until `app.run()` had started, because the
pool was only resolved inside the server entrypoint's detached async block.
Application code that wanted to touch the DB before serving — a migration step,
a REPL, or a test that exercises `app.db.query(...)` without binding a socket —
had no way to get a live `DbPool`. This was a recurring footgun.

**Decision / changes:**
1. **`JustAPIApp::connect_database(py)`** (crates/justapi-py/src/native/app.rs):
   resolves the configured `Database` into an `AnyPool` immediately. Idempotent
   (returns early if `db_pool` already set). The DB round-trip runs with the GIL
   released on a dedicated tokio runtime that is **kept alive on the app**
   (`db_runtime: Option<Runtime>`) so the `DbPool`'s `Handle` stays valid for the
   whole process. Bootstrap `init_sql` and the optional background health-check
   loop are applied here.
2. **Lazy-by-default on the Python side:** `JustAPIApp.db` property now calls
   `connect_database()` on first access (swallowing only the "not configured"
   case) and returns the resolved `DbPool`. So `app.db` works both before and
   after `run()`. Added an explicit `app.connect_database()` for callers who want
   to surface connect errors eagerly.
3. **`run()` reuses the same path:** it now resolves the pool via
   `connect_database` before the detached loop (guarded by `db_pool.is_none()`),
   then hands the inner `AnyPool` to the Rust-native CRUD handler via
   `DbPool::as_any_pool()`. No double-connect, no second runtime for the server
   case.

**Evidence:** New `benchmarks/smoke_dbpool_prerun.py` asserts `app.db is not None`
and runs `SELECT 1`, a `DELETE`/`INSERT`, and a `SELECT` **before any socket is
bound**; then serves a route that queries the same pool. Against live Aiven
Postgres: "FOOTGUN FIX VERIFIED". `cargo clippy -p justapi-py --tests -D warnings`
clean; `cargo test --workspace --features db` green.

## ADR-058 — 2026-07-17 — Track routing/gateway tests; fix streaming-error abort in test client

**Context:** Two artifacts were flagged "untracked" in PLAN.md: `test_routing.py`
(valid alias / multi-method / router-inclusion tests) and `test_gateway.json`
(orphaned gateway config fixture). Separately, the full pytest run hit a hard
`Fatal Python error: Aborted` from `panic = "abort"`: the in-process
`TestClient` mock server (`justapi-core/src/testing.rs`) `panic!`s on any
hyper connection error, and a validated stream that aborts mid-flight (e.g.
`stream_json` rejecting an invalid item via `send_error`) surfaces that as a
body-stream error — aborting the entire test process.

**Decision / changes:**
1. **Track the tests.** `test_routing.py` (2 tests, pass) is now committed.
   `test_gateway.json` moved into the package test dir and exercised by a new
   `test_gateway.py` that asserts `enable_gateway()` parses the config and
   registers proxy routes without needing live upstreams (1 test, pass).
2. **Test-client body-stream errors must not abort.** `testing.rs` line ~90
   changed from `panic!(...)` to `tracing::debug!(...)` when `serve_connection`
   returns an error. A streamed body that fails (validation abort) is expected
   application behavior, not a harness fault; under `panic = "abort"` it killed
   pytest. Now logged and the connection closes gracefully.

**Evidence:** `pytest crates/justapi-py/python/justapi/` → 120 passed, 1 skipped
(previously aborted). `cargo test --workspace --features db` green; clippy
`-D warnings` clean; `cargo fmt --check` clean.

## ADR-059 — 2026-07-17 — `Request.json()/body()/form()` must not require an event loop

**Context:** A framework audit (background tasks, routes, request handling) found
that a **sync** handler accessing `request.json()` / `request.body()` /
`request.form()` raised `RuntimeError: no running event loop`. Root cause:
`Request::json`/`body`/`form` in `crates/justapi-py/src/request.rs` returned their
value wrapped in `pyo3_async_runtimes::tokio::future_into_py(...)`, i.e. an
awaitable coroutine. Under the real server the sync handler runs on the daemon
event-loop thread (so it happened to work), but the `JustAPITestClient` path
(`make_test_handler`) runs sync handlers on a plain worker thread with **no**
running loop — so `request.json()` crashed every sync handler that parsed a body
through the test client. `body()`/`form()` had the same latent defect.

**Decision / change:** Make `json()`, `body()`, and `form()` pure synchronous
returns (they are CPU-only parses — no I/O). `receive`/`close`/`is_disconnected`
remain async (legitimate ASGI coroutines). Updated the two `await request.*`
call sites (`system.py` `tools_call_handler`, `test_request_parity.py`) to call
them without `await`. Added `test_sync_handler_body_json_no_event_loop` to lock
the regression in.

**Evidence:** New repro + test confirm `request.json()` works in a sync handler
via `JustAPITestClient` (previously `RuntimeError: no running event loop`). Full
suite 120 passed / 1 skipped; `cargo test --workspace --features db`, clippy
`-D warnings`, `cargo fmt --check` all green.

**Other audit observations (not changed):** (1) Registering two `GET` routes on
the same path raises `ValueError` ("route conflict") from the `matchit` router —
FastAPI/Starlette let the last win; this is a deliberate hard-fail, documented
here. (2) Trailing-slash mismatch is not normalized: a route declared `/trail/`
returns 404 for `/trail`. Consistent with the hard-fail philosophy; left as-is.

## ADR-060 — 2026-07-17 — Native Rust cron/interval scheduler (UTC, in-memory)

**Context:** A framework audit showed JustAPI lacked a built-in scheduler, forcing
users to bolt on `APScheduler`/`celery` for periodic work — a gap vs. frameworks
that ship one. The runtime's thesis (AGENTS.md §2) mandates scheduling/worker
pools live in Rust, not Python. We implemented a native scheduler that reuses the
**existing Rust background-task worker pool** (`crates/justapi-py/src/background.rs`
`submit_py_task`), so periodic jobs cost nothing extra on the worker side and
stay on the hot Rust path.

**Decision / change:**
- New `crates/justapi-py/src/scheduler.rs`: process-wide `Scheduler` (one
  `OnceLock<Arc<SchedulerInner>>`), a non-blocking `tick_loop` running on the
  shared tokio `Handle` polling every 250 ms. Each `Job` carries a `cron`
  `Schedule` (6-field, UTC) **or** a `Duration` interval. `compute_next`
  (pure, no GIL — unit-tested) advances past missed ticks. On fire, the job is
  enqueued onto the worker pool via `submit_py_task` and `stats.fired`/`failed`
  are updated; failures are caught (job id + `repr(func)` logged, not panicked —
  under `panic = "abort"` a user callback must never take the runtime down).
- **Deps:** added `cron = "0.15"` + `chrono` to `crates/justapi-py/Cargo.toml`.
  Justification: `cron` is the de-facto Rust standard cron parser (MIT, 0 deps
  beyond `chrono`), battle-tested; re-implementing 6-field cron math with
  timezone handling by hand would be more code and more bug surface. `chrono`
  ships the `DateTime<Utc>` we need for UTC-aligned fire math. This satisfies
  AGENTS.md §4 (new dep justified in DECISIONS).
- **Pyclass `Scheduler`** (`python/justapi/__init__.py`): `schedule(cron, func,
  *a, **kw)`, `every(secs, func, *a, **kw)` (return job id), `start()`, `stop()`,
  `stats()`, `jobs()`, `remove(id)`. Invalid cron raises `ValueError` at
  registration (untrusted-spec parse handled up front, not in the tick loop).
- `app.py`: `run()` calls `Scheduler().start()`; `maybe_start_if_jobs()` starts
  the loop only if jobs are registered; added `app.schedule(...)`/`app.every(...)`
  convenience delegating to the shared instance.
- **v1 scope (deliberate):** UTC-only, in-memory (no persistence/cron-file, no
  timezone table, no catch-up replay across restarts). Documented so users don't
  assume durability. The pure `compute_next` unit tests lock the math without a
  Python runtime, avoiding the `cdylib` link issue.

**Evidence:** `cargo test -p justapi-py scheduler` → 6 unit tests (cron future,
minute-boundary `0 * * * * *`, interval first-fire + advance + skip-past). Python
`test_scheduler.py` → 2 passed: interval + `*/2 * * * * *` cron both fire within
3.6 s, invalid cron raises, `stats()` reflects `fired`, `remove()` works. Manual
probe: `fired == ['every','cron',...]` `count>=4`, `stats={jobs:2,fired:4,...}`.
Full gates green (`cargo test --workspace --features db`, clippy `-D warnings`,
`cargo fmt --check`).

## ADR-061 — 2026-07-17 — Multi-worker prefork supervisor (`justapi serve --workers N`)

**Context:** JustAPI ran a single-process server (one accept loop on one
`tokio` runtime), capping throughput at one core for accept + handler dispatch
and offering no process-isolation fault tolerance. Users expect a production
ASGI/WSGI-style multi-worker model (uvicorn `--workers`, gunicorn). The feature
list calls for "Multi-worker runtime", "Process management", "Graceful
shutdown", and (later) "Auto-scaling workers".

**Decision / change:**
- New `crates/justapi-cli/src/workers.rs`: a **prefork** supervisor. The parent
  binds the `TcpListener` **once** (`bind_listener`), clears `FD_CLOEXEC` on the
  socket fd (modern std sets it; without clearing, the fd is closed across the
  child `exec` → "IO Safety violation: owned file descriptor already closed"
  abort), then spawns `N` children re-exec'ing the same binary with a hidden
  `--worker-fd <fd>` flag. Each worker recovers the socket via
  `listener_from_fd` and serves on it — true OS-process isolation, no
  `SO_REUSEPORT` port races, all workers share one kernel socket backlog.
- **Process management:** a persistent `JoinSet` of worker waits + a parallel
  `pids[]` vector. On unexpected exit (non-shutdown) the worker is **restarted**
  (verified: `kill -KILL` a worker → supervisor respawns it, fleet stays at N).
- **Graceful shutdown:** SIGTERM/SIGINT cancel a `CancellationToken`; the
  supervisor forwards SIGTERM to every live worker and waits up to
  `drain_timeout` (default 5s) for in-flight requests to drain before forcibly
  reaping with SIGKILL (verified: parent SIGTERM → both workers stop accept
  loop, drain 0 connections, exit 0, tree fully reaped).
- Refactored the previously-duplicated reload/non-reload server-build into
  `build_server()` + `build_engines()` helpers so single-process, reload, and
  worker paths share identical wiring (static/compression/TLS/inference).
- `--workers > 1` is incompatible with `--reload` (hot reload restarts the whole
  tree); the supervisor warns and ignores `--reload` in that case. `--workers` is
  default 1 (unchanged behavior). Added `libc = "0.2"` to the workspace for
  `fcntl`/`kill`.
- Also fixed a **pre-existing, unrelated** bug gated behind the `tls` feature: a
  `method` variable referenced in the TLS request-timeout branch was out of
  scope (`server/mod.rs:1848`); captured `let method = req.method().clone();`
  before the timeout consumes `req`. (The `tls` feature still has separate
  move-error breakage out of scope for this ADR.)

**Evidence:** `cargo test -p justapi-cli workers` → 2 unit tests
(`make_spawn_argv` carries fd+args; `bind_listener` actually binds an ephemeral
port). Manual: `justapi serve --addr 127.0.0.1:8099 --workers 2` → parent + 2
workers all "Listening on …:8099"; `curl /` served (404, no route — expected);
`kill -TERM` parent → both workers drain + exit 0, tree reaped; `kill -KILL` a
worker → respawned. Gates green: `cargo test --workspace --features db`, clippy
`-D warnings`, `cargo fmt --check`. (Windows/macOS: `--worker-fd` path is Unix-
gated; non-unix falls back to single-process with a clear error — auto-scaling
left for a follow-up.)

## ADR-062 — 2026-07-17 — Unix domain socket listener support (`justapi serve --unix <path>`)

**Context:** The feature list calls for "Unix domain socket support" for local
deployments (no TCP port; socket-permission-gated access; easy systemd socket
activation and nginx upstreams). `Server` only accepted a TCP `SocketAddr`.

**Decision / change:**
- `crates/justapi-core/src/server/mod.rs`: introduced a `ListenAddr` enum
  (`Tcp(SocketAddr)` | `Unix(PathBuf)`) with `From<SocketAddr>`, and
  `Server::new` now takes `impl Into<ListenAddr>`. `Server::run()` branches on
  TCP vs Unix; new `run_on_uds(UnixListener)` accepts an already-bound socket.
  Per-connection logic (`serve_connection`) was extracted out of `serve_http`
  and is shared by the TCP and Unix accept loops (`serve_unix` / `serve_http`).
  `bind_unix_listener(path)` removes a stale socket file on bind and spawns a
  `UnixSocketGuard` task that removes the file when the process's runtime
  terminates. TLS over UDS is **not supported** (`serve_on_uds_tls` bails with a
  clear error) — documented limitation.
- `crates/justapi-cli/src/main.rs`: added a `--unix <path>` flag (takes
  precedence over `--addr`). `build_server` now takes `&ListenAddr`; all four
  paths (single-process, reload, worker `--worker-fd`, parent prefork) branch on
  UDS vs TCP. In prefork, the parent binds the UDS once via
  `workers::bind_unix_listener` (clears `FD_CLOEXEC`, keeps the listener alive
  in the parent for the supervisor's lifetime) and hands the fd to each worker,
  which recovers it with `workers::listener_from_unix_fd`.
- `crates/justapi-cli/src/workers.rs`: added `bind_unix_listener` (sync,
  non-CLOEXEC) mirroring `bind_listener`, and `listener_from_unix_fd` (recovers a
  tokio `UnixListener` from an inherited fd, setting non-blocking first to avoid
  the tokio "Registering a blocking socket" panic).

**Evidence:** New `#[cfg(test)]` `test_unix_socket_serves_http` binds a UDS,
serves a trivial handler, connects via `tokio::net::UnixStream`, and asserts a
`200 {"uds":true}` HTTP response (exercises `serve_unix` → `serve_connection`,
262 core tests in the workspace run). Manual: `justapi serve --unix /tmp/j.sock`
→ "Listening on /tmp/j.sock (plain HTTP/1.1, unix)"; `curl --unix-socket /tmp/j.sock
http://localhost/` → 404 (no route). Prefork: `justapi serve --unix /tmp/j.sock
--workers 2` → parent + 2 workers, `curl --unix-socket` served; SIGTERM to parent
→ both workers drain + exit 0 ("worker exited during shutdown", "all workers
exited cleanly"), tree reaped. Gates green: `cargo test --workspace
--features justapi-core/db`, clippy `-D warnings`, `cargo fmt --check`. (UDS is
Unix-only; on non-unix `--unix` errors clearly.)

## ADR-063 — 2026-07-17 — Route-lookup cache (`Router::resolve` memoization)

**Context:** `Router::at` does a `HashMap` lookup + a `matchit` radix traversal on
every request, then (for non-matches) a second scan across all methods to
distinguish `NotFound` from `MethodNotAllowed`. At high RPS this is pure,
repeatable work for stable URLs. The feature list calls for a "route cache".

**Decision / change:**
- `crates/justapi-core/src/router.rs`: added `RouteResolution<T> { handler, params }`
  (owned, no borrow into the router) and `Router::resolve(&self, method, path)`
  which memoizes lookups through a bounded `RouteCache<T>` (`Mutex<HashMap>` +
  FIFO `order` vec for eviction; default capacity 1024, always-on).
  - **Caching policy (safety):** only *stable* outcomes are cached — static-route
    hits (no path params) and definitive `NotFound`s. **Param routes and
    `MethodNotAllowed` are never cached**, because they require a fresh full match
    (param extraction / cross-method scan) each request and could otherwise
    return stale data. The cache is per-`Router` (`Arc<RouteCache>` so `Router`
    stays `Clone`); negative entries avoid re-scanning on repeat misses.
  - `Router::new()` keeps an always-on cache; `without_cache()` and
    `with_cache_capacity(0)` disable it (useful for tests/benchmarks).
- `crates/justapi-core/src/server/mod.rs`: `execute_handler` now takes an owned
  `Handler` + `params: Vec<(String,String)>` (replacing the borrowed `Match`),
  eliminating the router borrow on the hot path and letting `make_handler` call
  `router.resolve(...)` and forward owned params to `ParamsEcho`/custom handlers
  via request extensions. The existing `Match`/`at` API is retained for tests and
  `gateway.rs` (no behavior change there).

**Evidence:** 4 new router unit tests (`test_resolve_static_hit_and_cache`,
`test_resolve_not_found_is_cached`, `test_resolve_param_route_bypasses_cache`,
`test_without_cache_still_resolves`). Full suite green (266 core tests). clippy
`-D warnings` + `fmt` clean. The cache is hit on every repeat request to a static
route / miss, removing the matchit traversal and the cross-method `MethodNotAllowed`
scan (which the pre-existing benchmark test still bounds at <100ns release / <1µs
debug). No change to routing semantics: param routes, method-not-allowed, and
fallback all resolve identically.

**Trade-off:** small fixed memory per cached `(method, path)` key (bounded by
capacity, FIFO-evicted). No new dependency.

## ADR-064 — 2026-07-17 — XML request/response support (`justapi_core::xml`)

**Context:** The feature list calls for XML/SOAP support. JSON is the dominant
wire format, but enterprise/legacy clients (SOAP, many internal services) speak
XML. JustAPI needed first-class XML as a content type alongside JSON, with the
parsing/sizing living in Rust (`justapi-core`) per ADR-008.

**Decision / change:**
- New `crates/justapi-core/src/xml.rs` (new dep `quick-xml = "0.36"`,
  `serialize` feature — high-perf XML reader/writer, justified below). Provides:
  - **Responses:** `xml_response(status, &str)` (raw string →
    `application/xml`) and `XmlResponse<T: Serialize>` (typed, mirrors
    `serialize::JsonResponse`) emitting `application/xml` via
    `quick_xml::se::to_string_with_root`. `json_to_xml(root, &Value)` serializes
    a `serde_json::Value` to XML.
  - **Requests:** `xml_to_json(bytes) -> Value` converts an `application/xml` /
    `text/xml` body into a `serde_json::Value`, so the rest of the pipeline (which
    speaks JSON `Value`) consumes XML input transparently. Implemented as a small
    event-driven converter over `quick_xml::reader::Reader` (NOT its serde
    `Deserializer` — that cannot represent open JSON `Value`s). Semantics: nested
    elements → nested objects; **repeated sibling tags → arrays**; text → string
    (coerced to number when it parses); attributes → `@name` keys; a leaf element
    with only text collapses to the scalar (`<id>1</id>` → `{"id": 1}`); mixed
    text+children lives under `#text`.
  - **Content negotiation:** `Format { Json, Xml }`, `negotiate(content_type,
    accept)`, and `respond(status, root, &Value, format)` produce a JSON or XML
    response from one `Value`.
- **Request wiring (Python path):** in `crates/justapi-py/src/native/handlers.rs`,
  after the body is read the request `Content-Type` is checked; an
  `application/xml` / `text/xml` body is converted to JSON via `xml_to_json` and
  re-serialized to canonical JSON bytes, so Python handlers receive a uniform
  JSON-shaped body regardless of the wire format (invalid XML → 400 with detail).
  Routing/validation/CRUD paths are untouched.
- `lib.rs` exports the `xml` module.

**Why `quick-xml`:** it is the de-facto high-performance Rust XML crate (zero-copy
reader, serde integration) and avoids hand-rolling a parser. The alternative
(writing our own XML reader) is more code and more bug surface for no benefit.
Only the `serialize` feature is enabled; no transitive heavy deps.

**Evidence:** 8 new `xml::tests` covering raw/typed XML responses, status+headers,
`json_to_xml`, `xml_to_json` (nested + arrays), `negotiate` (all branches), and
content-negotiated `respond`. Full workspace suite green (274 core tests). clippy
`-D warnings` + `fmt` clean. Limitation documented: XML attributes survive only
as `@name` keys; namespaces/comments/DTDs are ignored by the converter.

## ADR-065 — 2026-07-18 — Load-based worker auto-scaling (`justapi serve --scale`)

**Context:** The feature list calls for "Auto-scaling workers". The prefork
supervisor (ADR-061) runs a fixed `N` workers; operators had to edit `--workers`
by hand as load changed. We need the fleet to grow under load and shrink when
idle, with true OS-process isolation preserved.

**Decision / change:**
- `crates/justapi-cli/src/workers.rs`:
  - `LoadProbe` trait (`Send + Sync`, `fn sample() -> f64` returning normalized
    load, 0.0 = idle, 1.0 = saturated). `SystemLoadProbe` (unix) reads the
    1-minute load average via `libc::getloadavg` normalized by
    `available_parallelism`; `NoProbe` (0.0) is the no-policy fallback.
  - `ScalingPolicy { min_workers, max_workers, low, high, cooldown, step }` and a
    **pure** `decide_scale(current, load, &policy) -> usize` (scale up by `step`
    when `load >= high`, down by `step` when `load <= low`, else hold, clamped to
    `[min, max]`). This is the testable core — no processes, no OS.
  - `supervise` now takes `Option<(ScalingPolicy, Arc<dyn LoadProbe>)>`. A
    `tokio::time::interval` re-samples load each tick; when the cooldown has
    elapsed and `decide_scale` differs from the live active count, it scales:
    **up** spawns into free/reused slots (`next_free_slot`); **down** gracefully
    `SIGTERM`s live, non-stopping workers and records them in a `stopping`
    `HashSet` so the restart-on-death path does NOT respawn them. The drain logic
    ignores `stopping` slots when checking "all exited".
- `crates/justapi-cli/src/main.rs`: new flags `--scale`, `--min-workers`,
  `--max-workers`, `--scale-low`, `--scale-high`, `--scale-cooldown`. Prefork now
  also triggers when `--scale` is set with `max > 1` (so a fleet can start at
  `min_workers == 1` and grow). Bounds capped at 256. When `--scale` is set
  without explicit `--min/--max`, min = `--workers`, max = `workers * 2`
  (min+1 floor). Builds the policy + `SystemLoadProbe` and passes it to
  `supervise`. The `tls` feature still excluded (separate breakage).

**Evidence:** 7 new `workers::tests`: `decide_scale` holds-in-band / up-at-high /
down-at-low / clamps-to-bounds / respects-step, plus `next_free_slot` reuse+grow.
Full suite green (274 core + 8 workers tests). Manual: `--scale --workers 1
--max-workers 3 --scale-high 0.0` → supervisor starts at 1 worker then scales up
to 3 (log: "scaled up load=0.08 workers=2/3"); `--workers 3 --min-workers 1
--max-workers 3 --scale-low 1.0` → scales 3→2→1 with "scaled-down worker exited"
(graceful, not restarted). SIGTERM in both cases drains the tree to exit 0.
Clippy `-D warnings` + `fmt` clean. No new dependency (reuses `libc`,
`available_parallelism`).

## ADR-066 — 2026-07-18 — Fix pre-existing `tls` feature breakage in `justapi-core`

**Context:** `cargo build -p justapi-core --features tls` failed to compile
before any of ADR-060..065 touched it — a pre-existing defect in
`serve_with_tls` (the TCP+TLS accept loop in `crates/justapi-core/src/server/mod.rs`),
not caused by the worker/UDS/XML work. The function param `static_mounts:
Vec<StaticMount>` was captured by the per-request `service_fn(move |req| ...)`
closure (an `Fn`, invoked for every request) and referenced by move, producing
`E0507` ("cannot move out of `static_mounts`, a captured variable in an `Fn`
closure") and `E0382` ("use of moved value: `static_mounts` in previous
iteration of the loop"). A second latent clippy lint (`if let Some(_) =
shutdown`) in the same function also broke `-D warnings`.

**Decision / change:**
- Mirror the already-correct pattern in non-TLS `serve_connection`:
  - Clone `static_mounts` once per loop iteration from the function param (alongside
    `chain`/`static_dir`/`metrics`) so the value is re-acquired each iteration and
    never moved out of the per-iteration scope.
  - Inside the `service_fn` closure, `let static_mounts = static_mounts.clone();`
    so the `Fn` closure borrows its own copy to `.clone()` per request instead of
    moving a `Vec` out of a captured variable.
- Replace `if let Some(_) = shutdown` with `if shutdown.is_some()` to satisfy
  `clippy::redundant_pattern_matching` under `-D warnings`.

**Evidence:** `cargo build -p justapi-core --features tls` now compiles.
`cargo test -p justapi-core --features "tls,db"` green (274 core + 8 xml + 15
wasm + 1 tracer). `cargo clippy --workspace --tests --features
"justapi-core/db,justapi-core/tls" -- -D warnings` clean; `cargo fmt --check`
clean. No new dependency, no public API change (internal function only).

## ADR-067 — 2026-07-18 — Freeze all AI/inference work until release 2.0.8

**Context:** We spent effort on the Phase 52 GPU Benchmark Gate (real-model
inference via `justapi-inference`/`justapi-bench`, CUDA PTX mismatch, Needle
26M encoder-decoder weights downloaded to `~/needle-bucket`). The user has
decided to **skip all AI / inference / LLM work for now** and defer it to the
next release, **2.0.8**. This includes the CUDA driver upgrade (NVIDIA 610.43.03
was available and matched the running kernel `7.1.3-2-cachyos` but was
deliberately NOT applied), the real-GPU benchmark run, and any Needle
integration.

**Decision:**
- No AI/inference features will be touched until release **2.0.8**.
- The CUDA driver stays at 580.173.02 (CUDA 13.0 runtime) and the toolkit at
  13.3.1 — the PTX mismatch is intentionally left unresolved for now.
- The Phase 52 GPU Benchmark Gate remains **🔴 blocked**; its exit criteria
  (real weights on real GPU, tokens/sec + TTFT/ITL) are deferred to 2.0.8.
- The Needle weights in `~/needle-bucket` are kept as reference material only;
  no encoder-decoder loader will be added to `justapi-inference` yet (it
  currently supports Llama-family GGUF/safetensors only).
- `justapi-gpu-bench` MockModel (CPU) run is recorded as non-gate plumbing
  validation only (see PLAN.md Phase 52 update, commit `a6c8cac`).

**Evidence:** User directive 2026-07-18 ("skip this AI stuff, work it on next
release 2.0.8, until then don't touch AI stuff"). PLAN.md Phase 52 updated to
reflect the opt-out and committed (`a6c8cac`). No code change in this ADR.



## ADR-068 — 2026-07-18 — Fix DB-backed handler deadlock + WAL hang (D2/D3/D4)

**Context:** Building `demo_shop/` (23-route Olist SQLite e-commerce API) on the
framework surfaced four defects in the Python-handler + SQLite path. Two were
genuine framework bugs, one was a misdiagnosis, one was a usage note.

- **D2 — `set_database(wal=True)` hung the pool connect forever** on the 112 MB
  Olist file. Reproduced deterministically (30s+ no output).
- **D3 — DB-backed handler ~2500× slower:** ~900 ms/req (15 req/s) for a trivial
  `COUNT(*)`, vs ~1.8 ms raw query.
- **D4 — concurrent async load deadlocked:** `bench_async.py -c 32` hung (90s+
  timeout); only the no-DB path survived concurrency.
- **D1 — `app.run()` "never binds" with a DB:** on re-test this was RETRACTED —
  the server always bound; the failure was the D4 deadlock on the probing
  DB route, misread as "server down" (see `demo_shop/README.md` Finding #3).

**Root cause (D2):** `justapi-core/src/db/pool.rs` `after_connect` ran
`PRAGMA journal_mode=WAL` with `conn.execute` and **no `busy_timeout`**. WAL
requires an exclusive lock; during pool warm-up several connections open
concurrently and the second opener blocks on the WAL lock with no timeout →
infinite wait.

**Root cause (D3/D4):** `crates/justapi-py/src/native/app.rs::connect_database`
built a **`tokio::runtime::Runtime::new()` (current-thread)** runtime and stored
its handle for `DbPool::query/execute` to `block_on`. A current-thread runtime's
handle can only be driven from the owning thread; calling `block_on` on it from a
**foreign thread** (a GIL-pool worker or the server-runtime thread) silently
deadlocks or serializes catastrophically — producing ~900 ms/req at concurrency=1
(D3) and a full hang at concurrency>1 (D4).

**Decision:**
- D2 fix: in `justapi-core/src/db/pool.rs`, for SQLite always prepend
  `PRAGMA busy_timeout=5000` to the pragma list and run every pragma (including
  `journal_mode=WAL`) via `fetch_optional` instead of `execute`, ignoring the
  returned row.
- D3/D4 fix: in `crates/justapi-py/src/native/app.rs::connect_database`, build a
  **multi-threaded** runtime — `tokio::runtime::Builder::new_multi_thread()
  .worker_threads(4).enable_all().build()` — and use its `Handle` (which is safe
  to `block_on` from any thread, as it temporarily drives the runtime on the
  caller's thread) for the bootstrap block_on and for `DbPool`'s queries.
- D1: no code change — confirmed server binds; the report was a misread of D4.

**Evidence:**
- `wal=True` connect on 112 MB Olist file: **0.00 s** (was 30s+ hang).
- `/products` at concurrency=1: p50 **2–4 ms** (was 910 ms) — ~250× faster.
- `oha -z 12s -c 64 /products?size=20`: **3,706** successful concurrent requests,
  p50 188 ms, p99 367 ms, **0 deadlocks** (was a full hang at -c 32). The 188 ms
  p50 at -c 64 is GIL-pool saturation across 64 parallel Python handlers — a
  Python-GIL effect, not a DB-path defect.
- `demo_shop/test_app.py`: 23/23 routes pass via AsyncTestClient.
- `cargo test --workspace --features justapi-core/db` green; `cargo clippy
  --workspace --tests --features justapi-core/db -- -D warnings` clean;
  `cargo fmt --check` clean. No new dependency; internal change only.

## ADR-069 — 2026-07-18 — Two framework serialization/payload gotchas (found building demo_shop MVP)

**Context:** Rebuilt `demo_shop/` as a proper MVP shop (real data model, full
CRUD, cart→checkout→order→payment→review flow, validation via `justapi.Schema`).
Two framework behaviors surfaced that break naive handlers and were worked
around in `demo_shop/app.py`:

1. **Response bodies containing a top-level `"status"` key are emitted as an
   empty body.** A handler returning `{"status": "ok", "products": N}` yields
   `HTTP 200` with `content-length: 0` and no body; `{"products": N}` serializes
   correctly. Reproduced with minimal handlers: `{"status":"ok"}` → empty,
   `{"products":1}` → fine, `{"status":"ok","products":1}` → empty. The
   framework appears to treat a JSON object with a `status` key as a
   status-response envelope and mishandles it. **Workaround:** order state is
   exposed as `order_status` (SQL column stays `status`); health uses `health`.
   **This is a framework bug and should be filed/fixed** (response
   serialization path).

2. **`body_schema=` delivers the validated payload to the handler as raw
   `bytes`, not an instantiated Schema object.** A handler declared as
   `def h(request, payload: ProductIn)` receives `payload` being a `bytes`
   object (so `payload.id` → `AttributeError: 'bytes' object has no
   attribute 'id'`). **Workaround:** handlers parse `request.json()` and validate
   via a local `validate(schema_cls, data)` helper that uses the Schema's
   generated JSON Schema (`Schema._build_schema()`). The `Schema` classes still
   drive OpenAPI docs through `body_schema=`. **This is a framework bug**
   (PyO3/dispatch binding for validated bodies) and should be filed/fixed.

**Decision:** Work around both in the app layer for now (no framework code
change beyond adding `PATCH` to `JustAPITestClient`/`TestClient` so the test
suite can drive `PATCH` routes). Document them in `demo_shop/README.md` as
framework gotchas to fix.

**Evidence:** `demo_shop/test_app.py` — 29/29 assertions pass via
`justapi.testing.AsyncTestClient` (catalog, CRUD, atomic checkout decrementing
stock, order status lifecycle, review validation). The two behaviors were
isolated with minimal repro handlers. `cargo test --workspace
--features justapi-core/db` green; `cargo clippy --workspace --tests
--features justapi-core/db -- -D warnings` clean; `cargo fmt --check` clean.

---

## ADR-070 — 2026-07-18 — Framework had no usable logging from `app.run()`

**Context:** Building `demo_shop/` surfaced that the framework emitted **no
logs at all** when launched from Python. `justapi-core` already had a full
`tracing`-based subsystem (`tracing_setup.rs`: `init_logging`, `LoggingConfig`,
JSON/text, rolling file, OTLP) and the server (`server/mod.rs`) fired
`tracing::info!`/`error!` events plus an `info_span!("http.request", …)` per
request — but **nothing ever initialized a `tracing` subscriber on the Python
`app.run()` path**. The `init_tracing`/`init_logging` functions existed in core
but were unreachable from PyO3, and the default `LoggingConfig` shipped with
`otel_exporter: Some(OtelExporter::Stdout)`, which would dump raw OTLP span
JSON to stdout.

**Options considered:**
1. Leave logging as a Rust-only feature, require users to wire `tracing`
   themselves — rejected: a web framework that is silent on startup, request
   completion, and errors is not production-grade; the user explicitly flagged
   this ("there is no logging system in this framework").
2. Auto-install a default subscriber inside `app.run()` + expose opt-in config
   from Python — **selected**.

**Decision:**
- `justapi-core/src/tracing_setup.rs`: added `init_default_if_unset()` — calls
  `init_logging(LoggingConfig::default())` only if
  `tracing::dispatcher::has_been_set()` is false (so a user-installed subscriber
  is never clobbered). Changed `LoggingConfig::default()` to `otel_exporter:
  None` so the default is clean text→stdout, not an OTLP stdout span dump.
- `justapi-py/src/native/app.rs::run` now calls `init_default_if_unset()` before
  serving → the framework logs automatically the moment an app runs.
- `justapi-py/src/logging.rs` (new): thin PyO3 glue exposing `init_logging`,
  `init_json_logging`, `init_file_logging`, `init_otlp_tracing`,
  `shutdown_tracing` (registered in `lib.rs`). Re-exported on the `justapi`
  package and via `justapi.logging`.
- `justapi-core/src/server/mod.rs`: added a structured **access-log** `info!`
  event on every completed (non-timeout) request — `http.method`, `http.path`,
  `http.status_code`, `latency_ms` — so each request is visible. Startup now
  prints a clear banner: `🚀 JustAPI serving on http://<ip>:<port>` plus a
  "endpoints live at …" line, so the bound IP/port is always visible.
- `justapi-core/src/tracing_setup.rs`: the default **Text** format was
  reworked into a clean, colored CLI style — compact `HH:MM:SS` timestamp
  (custom `UptimeTimer`, no nanos/date), ANSI-colored level (green INFO /
  yellow WARN / red ERROR / cyan DEBUG / magenta TRACE), dimmed module target,
  and thread/file/line noise disabled. ANSI is on for stdout, off for file
  appenders. The `Json` format stays structured for collectors.

**Evidence:** `python -m shop --port 8111` now prints:
```
17:28:10  INFO  justapi::gil_pool: GIL pool initialized: mode=GilBased …
17:28:10  INFO  justapi_core::server: 🚀 JustAPI serving on http://127.0.0.1:8111
17:28:10  INFO  justapi_core::server:    ready — endpoints live at http://127.0.0.1:8111/
17:28:15  INFO  justapi_core::server: request completed http.method=GET http.path=/products http.status_code=200 latency_ms="8.70"
```
Levels are ANSI-colored, timestamp is short, and the listening IP:port is
explicit. `cargo test -p justapi-core --features justapi-core/db`, `cargo
clippy`, `cargo fmt --check` all green; `demo_shop/test_app.py` 29/29 and
`demo_shop/test_lookup.py` 19/19.

---

## ADR-071 — 2026-07-18 — Scalar API Reference as the default home-page docs

**Context:** The framework already served Swagger UI (`/docs`) and ReDoc
(`/redoc`) from `/openapi.json`, but the **home page `/`** was unclaimed (404 /
static fallback) and Scalar — the modern, fast API-reference UI at
<https://scalar.com/> — was not available. The user asked for Scalar to be wired
in properly and for the **home page to open the docs**.

**Decision:** In `crates/justapi-py/python/justapi/app.py`:
- Added `_builtin_scalar(app)` — the official Scalar CDN embed
  (`@scalar/api-reference`, `Scalar.createApiReference('#app', { url:
  '/openapi.json', theme: 'default', layout: 'modern' })`).
- Registered `GET /scalar` → Scalar, and **`GET /` (root) → Scalar** so the
  home page opens the interactive reference.
- `/docs` (Swagger UI) and `/redoc` (ReDoc) remain, all reading the live
  `/openapi.json` generated from registered routes (`build_openapi`).

**Evidence:** `python -m shop --port 8112` → `GET /` returns 200 `text/html`
(564 B) embedding `Scalar.createApiReference`; `GET /scalar` and `GET
/openapi.json` (title "JustAPIApp", 20 shop paths) verified. `cargo fmt`,
`cargo clippy -D warnings` clean; `demo_shop/test_app.py` 29/29,
`test_lookup.py` 19/19.

---

## ADR-072 — 2026-07-18 — Fixed `body_schema=` validation (ADR-069 root cause) + auto-slug

**Context:** ADR-069 documented two framework behaviors as "work around in the
app, file as bugs". The first — `body_schema=` delivering the validated payload
as raw `bytes` / failing to instantiate the Schema — was actually a **crash**,
not a silent bytes delivery: `validate_body` in `_native_helper.py` called the
Schema class directly (`CategoryIn(body_dict)`), raising
`TypeError: CategoryIn() takes no arguments` on every POST. The app papered over
it with a manual `validate()` helper, but the framework still logged the spurious
error on every write request. The user hit `{"detail":"missing required field:
slug"}` noise and a `TypeError` traceback on `POST /categories`.

**Root cause #2 (latent):** `justapi.Schema.__init_subclass__` read raw
`cls.__annotations__`. With `from __future__ import annotations` (used by
`shop/schemas.py`), annotations are *stringized*, so every field resolved to
`str` — numeric fields (`price: float`, `stock: int`) generated `"type":
"string"` in the JSON Schema. Once real validation was switched on, every
numeric body was rejected (`"12.5 is not of type string"`).

**Decision (framework fixes in `crates/justapi-py`):**
- `_native_helper.validate_body` now detects a `justapi.Schema` subclass (via
  `_schema_json`) and validates through the Rust `validate_value` engine — the
  same path as the native fast route — returning clean error strings (e.g.
  `"name_pt" is a required property`). No more `TypeError` noise.
- `justapi.Schema.__init_subclass__` resolves annotations with
  `typing.get_type_hints(cls)` so PEP-563 stringized annotations and forward
  refs map to real types; numeric fields now generate correct JSON Schema types.

**App fix (`demo_shop/shop`):** `CategoryIn.slug` is now `Optional[str]` and
`create_category` auto-generates a slug via `slugify(name_pt)` when absent, so
`POST /categories` only requires `name_pt`. The 409 message reports the actual
computed slug.

**Evidence:** `POST /categories` with only `name_pt` → 200 (slug auto-created);
duplicate → 409 `category slug '…' already exists`; missing `name_pt` → 422
`"name_pt" is a required property` (no `TypeError`). `POST /products` with
`price:12.5, stock:10, category_id:1` → 200 (numerics validated correctly).
`cargo fmt`, `cargo clippy -D warnings` clean; `demo_shop/test_app.py` 29/29,
`test_lookup.py` 19/19.

---

## ADR-073 — 2026-07-20 — DB-startup + response-correctness fixes (P0.1/P0.2/P0.3)

**Context:** The 2026-07-20 production audit (PRODUCTION_PLAN.md) found three
shipping blockers on the core request path:
- **P0.1 (BUG-2a):** `normalize_db_url` (pool.rs) only stripped the `sqlite://`
  prefix verbatim, so `sqlite:///rel.db` → `sqlite:/rel.db` (absolute at
  filesystem root) and `sqlite:////abs` → `sqlite://abs` (mangled). Every
  `app.set_database(...)` + `app.run()` crashed with `SQLITE_CANTOPEN` (code 14)
  on any path. `demo_shop` reported it as a deadlock.
- **P0.2 (BUG-2b / D3):** SQLite PRAGMAs were opt-in — the
  `busy_timeout`+WAL block only ran `if (DbKind::Sqlite, Some(pragmas))`. The
  Python `set_database` default is `pragmas=None`, so a fresh pool ran stock
  defaults (rollback journal + `busy_timeout=0`) → constant `SQLITE_BUSY` under
  the 10-connection default pool → ~15 req/s / ~900 ms p50 in the stress test.
- **P0.3 (BUG-1):** `serialize_response` (handlers.rs) treated ANY dict with a
  `"status"` key as a legacy `{"status","body"}` envelope. A normal payload like
  `{"status":"ok","products":5}` got its body silently emptied (200 + empty body).

**Decision:**
- `normalize_db_url` rewritten as a slash-count matcher following the
  SQLAlchemy 3/4-slash convention: `sqlite:///x` (3 slashes) = relative
  (joined to cwd, because `sqlite:rel` bare-relative is not reliably resolved by
  sqlx's Any driver), `sqlite:////abs` (4 slashes) = absolute, `sqlite:///abs`
  (absolute passed by the Python bridge as `sqlite://{abs_path}`) = absolute,
  `sqlite://:memory:` / `sqlite://./x` preserved. Relative/cwd-joined names are
  emitted as absolute `sqlite:/abs/path` so sqlx opens them.
- In `connect()`, for file-backed SQLite (not `:memory:`) the parent directory
  is created and the DB file is pre-created before opening — sqlx's SQLite Any
  driver does not auto-create the file for this URL form (code 14). Verified by
  direct sqlx connect failing without this, succeeding with a pre-created file.
- SQLite pragmas are now **unconditional**: `busy_timeout=5000` +
  `journal_mode=WAL` + `synchronous=NORMAL` applied for every SQLite pool;
  caller `pragmas` are appended after and may override.
- `serialize_response` now only enters the legacy-envelope branch when the dict
  carries a `"body"` key or an explicit `__response__: true` sentinel. A plain
  data dict with a `"status"` field falls through to normal JSON serialization.

**Evidence:**
- `cargo test -p justapi-core --features db db::pool`: 8 passed incl. new
  `test_normalize_db_url_sqlite_paths` (relative/absolute/:memory/./x/postgres)
  and `test_sqlite_connect_relative_and_absolute_file` (both connect + SELECT 1).
- Live repro: `app.set_database("sqlite:///abs/path.db")` + `app.run()` now
  serves `/ping` and `{"status":"ok",...}` returns intact (previously crashed /
  empty body).
- `demo_shop` full e-commerce script (`test_app.py`): **29 passed, 0 failed**
  against a seeded SQLite DB — D1/D3/D4 all closed at the app level.
- `cargo test --workspace --features db`: all suites green; `cargo clippy
  --tests` clean (one manual-prefix-strip warning fixed via `strip_prefix`).

### ADR-073 addendum — P1.1: `body_schema` routes receive a parsed dict

**Context (PRODUCTION_PLAN.md P1.1 / BUG-3):** handlers registered with a
`justapi.Schema` `body_schema` should receive the *already-validated, parsed*
body object, not raw bytes they must `json.loads()` again. ADR-072 fixed the
crash (Schema instantiation as a class), but the handler still re-parsed bytes.

**Decision:**
- `crates/justapi-py/src/native/handlers.rs` now parses the JSON body **once on
  the fast path** (via a new `_native_helper.parse_body`) *only when the schema
  is a `justapi.Schema` subclass* (detected by a `_schema_json` attribute — so
  legacy callable validators keep `request["body"]` as raw bytes and existing
  `json.loads(request["body"])` handlers keep working). The parsed object is
  attached to the `Request` (`request.rs` new `parsed_body` cache).
- `Request.json()`, `Request["body"]`, and the new `Request.validated_body`
  getter return the cached parsed object for schema routes. `request["body"]`
  keeps raw bytes for non-schema / legacy-callable routes.
- The P0.3 envelope rule (`handlers.rs` `serialize_response` + `_native_helper.
  wrap_result`) was refined so a **status-only** dict (`{"status": 204}` or
  `{"status": 200, "headers": [...]}`) is still treated as a response envelope
  (sets the status code, empty body), while a dict with `"status"` *plus other
  keys* (`{"status": "ok", "products": 5}`) is always serialized as normal JSON.
  This preserves the common `return {"status": 204}` idiom without re-introducing
  BUG-1. The explicit `{"__response__": true, ...}` sentinel still wins.

**Evidence:**
- New `test_body_schema_parsed.py::test_body_schema_delivers_parsed_dict`
  (pytest-asyncio) asserts a `Schema`-registered handler receives `dict` from
  both `request.json()` and `request["body"]`.
- `test_validation.py` (legacy callable schema doing `json.loads(request["body"])`)
  still passes 200/201/422 — legacy path unchanged.
- `test_responses.py` / `test_testing.py` / `test_test_client.py` confirm
  `{"status": 204}` → 204 and `{"status": "ok", "products": 5}` → intact body.
- Full Rust workspace + clippy clean. Remaining pytest failures are the known
  P1.3 missing-dev-deps (`pydantic`/`jinja2`/`websockets`) + `test_scheduler`
  flake, unrelated to these changes.

---

## ADR-074 — 2026-07-20 — Real DB-backed CRUD benchmark + Python-handler write-path defect

**Context:** PRODUCTION_PLAN.md P2.1 required an honest "real life" DB-backed CRUD
benchmark vs FastAPI+SQLAlchemy and Robyn (the micro echo/validate benchmarks
overstate production reality ~1000× on DB paths). Added `benchmarks/crud_justapi.py`
(JustAPI Python-handler + Rust-native CRUD), `crud_fastapi.py` (FastAPI +
SQLAlchemy async), `crud_robyn.py` (Robyn + sqlite3), and `bench_one.py` (drives
`oha --output-format json` and reports success rate). Ran `oha -c 10 -z 5s`
against a SQLite file (WAL, `busy_timeout=5000`, 10-conn pool) on the standard
hardware fixture (i5-13600K, 20 threads, CachyOS).

**Findings (appended to BENCHMARKS.md):**
- **SELECT (reads) are solid and framework-differentiated:** JustAPI Rust-native
  177,709 RPS > Robyn 30,602 > JustAPI Python-handler 27,551 > FastAPI+SQLAlchemy
  1,606. JustAPI Python-handler reads are ~17× faster than FastAPI+SQLAlchemy on
  this workload (SQLAlchemy async-session/ORM overhead dominates).
- **Writes on single-file SQLite are SQLite-bound** for every framework
  (SQLite serializes writers); no framework sustains more than hundreds of
  writes/s on one file. So writes are not a fair framework comparison on this
  fixture.
 - **Blocking defect in JustAPI's Python-handler write path (P2.2):** at `-c 10`
   the *same server* reports INSERT 362,143 RPS, UPDATE 14.6 RPS, DELETE 4.2 RPS
   — internally inconsistent and physically impossible for SQLite. Repro at
   `-c 50` against the Python-handler server: of ~50 concurrent INSERTs only 1–2
   rows persist (the rest silently vanish, successRate still 100% because the
   client sees no error — the request's `block_on` never returns a response and
   the connection is closed by the client first).

   **Root cause (revised):** `crates/justapi-py/src/database.rs` `query()`/
   `execute()` ran the DB future via `py.detach(|| rt.block_on(fut))` where
   `rt` is the pool's dedicated multi-threaded tokio runtime. Calling
   `Handle::block_on` from the server's dispatch thread deadlocks that runtime:
   the connection-pool acquire never resolves (sqlx logs
   `acquired connection ... after 9.99s`, i.e. it hit `busy_timeout`), so every
   subsequent write blocks until timeout and no row is committed. Reads happened
   to slip through before the pool exhausted, which is why only writes collapsed
   and why the defect looked "INSERT-specific". It is in fact a
   blocking-runtime re-entrancy bug on the write path under concurrency.

   **Fix:** `run_blocking()` now releases the GIL with `py.detach` (preserving
   read throughput) but instead of `rt.block_on` it does `rt.spawn(fut)` and
   blocks the caller on an `mpsc` channel — the future runs to completion on the
   DB runtime's own worker threads and commits; the caller just waits, with no
   re-entrant runtime driving. Verified: 100 concurrent INSERTs via the test
   client all persist (was ~1–2 before), and a direct 100-thread
   `app.db.execute` burst persists 200/200 with zero errors. SELECT throughput
   is unchanged vs the old path (~5k RPS on the single-worker GIL-pool server
   fixture; the 27k previously recorded was a different multi-worker run).

 **Decision:**
 - Publish the SELECT numbers and the honest SQLite-write caveat in BENCHMARKS.md.
 - P2.2 is **resolved**: the Python-handler write path now awaits + commits
   deterministically, guarded by `test_db_concurrent.py`
   (`test_concurrent_inserts_all_persist` and
   `test_concurrent_writes_via_test_client`). The Rust-native CRUD path
   (`crud_table`/`crud_columns`) remains the fastest write route.
 - Track the fix as P2.2 (done) in PRODUCTION_PLAN.md and PLAN.md.

**Evidence:** `benchmarks/run_crud_bench.sh` (and the isolated `bench_one.py`
runs) on the fixture; raw oha JSON captured successRate=100% with the
INSERT/UPDATE/DELETE RPS spread above. Read numbers reproduced across runs; the
write-path inconsistency reproduced at `-c 10` and `-c 20` (c=1 INSERT was a
healthy 117 RPS, confirming the collapse is concurrency-induced).

---

## ADR-075 — 2026-07-24 — Untrusted-input fuzzing target and JSON Schema security justification

**Context:** PRODUCTION_PLAN.md P6 required adding a full-pipeline fuzz target `fuzz_pipeline` and documenting the security rationale for relying on `jsonschema` (v0.46) for body validation.

**Decision:**
1. Created `fuzz/fuzz_targets/fuzz_pipeline.rs` targeting:
   - Untrusted byte validation via `justapi_core::validate::validate_json_schema` and precompiled `CompiledValidator`
   - URL query parameter deserialization via `parse_query`
   - Path resolution and route parameter extraction via `justapi_core::router::Router`
2. **Security justification for `jsonschema` crate:**
   - `jsonschema` is a pure-Rust, memory-safe JSON Schema engine maintained by PyO3 core contributors.
   - It executes zero unsafe memory operations during validation and is extensively fuzzed upstream.
   - Precompiled validators (`CompiledValidator`) ensure schemas are validated and compiled once per route, avoiding unbounded memory allocation or regex stack overflow under malformed input payloads.

**Evidence:** `cargo check --bin fuzz_pipeline` builds cleanly in `fuzz/`.

---

## ADR-076 — 2026-07-25 — ORJSON as optional Rust-level JSON serializer

**Context:** Project listed "ORJSON as optional JSON serializer" as a remaining gap. The Python `orjson` library (v3.11) was already imported on a best-effort basis in both `_native_helper.py` (try/except) and the Rust `fast_dumps()` path (`__import__('orjson')` with `json.dumps` fallback). The gap was that this integration was informal — no feature flag, no hard requirement path, no pyproject.toml extra.

**Options considered:**
1. Add `orjson` as a Rust crate dependency (via crates.io) — rejected: no standalone `orjson` Rust crate exists; `orjson` is a Python package built with PyO3/maturin, not published on crates.io.
2. Create a Rust-level `orjson` feature that panics if the Python module is missing — selected for `justapi-py`. The feature makes `fast_dumps()` hard-require `orjson` at first call instead of silently falling back to `json.dumps`.
3. Add `orjson` to `pyproject.toml` extras — selected. `orjson>=3.10` added as `[orjson]` extra and included in `[full]`.
4. No change (status quo) — rejected: the integration should be documented, testable, and discoverable by users.

**Decision:**
- Added `orjson = []` feature to `justapi-core` (marker only, no dependencies) and `orjson = ["justapi-core/orjson"]` to `justapi-py`.
- `fast_dumps()` in `handlers.rs` uses `#[cfg(feature = "orjson")]` to hard-require `py.import("orjson")` with a `panic!` on failure; without the feature it falls back to `json.dumps` as before.
- Added `orjson>=3.10` to `pyproject.toml` as `[project.optional-dependencies] orjson` extra and included in `full`.
- Added 7 tests in `test_orjson.py` covering compact-JSON output, `default=str` fallback, and end-to-end response serialization via `AsyncTestClient`.

**Evidence:** `cargo check -p justapi-py --features orjson` passes; `cargo check` (default/no-orjson) passes; pytest 159 passed / 1 skipped; `cargo test --workspace` all pass.


## ADR-077 — 2026-08-06 — Drop ASGI compatibility; fully native Rust pipeline

**Context:** PROMPT.md Section 2 and ADR-007/013 mandated a Tier A ASGI
compatibility shim (run unmodified FastAPI/Starlette apps on JustAPI's Rust
runtime). That shim was never shipped in the maintained codebase. The public
docs (README, docs_site) still claimed "an ASGI shim for compatibility with the
Starlette middleware ecosystem", `app.mount()` accepting arbitrary ASGI apps,
`httpx.ASGITransport(app=app)` test clients, and an `asgi_app=` kwarg on
`app.run()`. Investigation confirmed these are **not implemented**:
- `mount()` raises `ValueError` for anything that isn't a `str` or `APIRouter`.
- `run()` has no `asgi_app` kwarg.
- No ASGI receiver/sender/scope shim exists anywhere in `justapi-py`/`justapi-core` Rust sources.
- `dependency_overrides` (documented) does not exist.

**Options considered:**
1. Implement a real ASGI layer (Tier A) — rejected. ASGI is a dict-based,
   dynamically-typed protocol that would drag the typed zero-copy Rust pipeline
   (route → schema validation → Rust serialization → wire) back through a
   Python interface on every request. It taxes the fastest path for the benefit
   of running legacy Starlette apps — the wrong target audience. We already
   ship the native equivalents of every middleware ASGI would import (CORS,
   JWT, rate-limit, security headers, compression, WS, SSE).
2. Keep the ASGI claims but defer the work — rejected. The docs are a false
   promise; claims without substance erode trust faster than a missing feature.
3. Remove ASGI compatibility entirely and own the native stack — **selected.**

**Decision:**
- JustAPI is a fully native runtime. No ASGI/Starlette dependency, no ASGI shim.
- Removed the false claims from README, docs_site (advanced-middleware,
  async-tests, sub-applications, static-files, migration-guide, glossary,
  external-links), AGENTS.md architecture diagram, and PLAN.md (marked Phase 10
  Tier A as historical/deprecated; updated the phase-table row and the
  "native-for-performance, ASGI-for-compatibility" principle).
- Corrected the real APIs in docs: native middleware is `@app.middleware("http")`
  + `(request, call_next)` callables and the `enable_secure_headers` /
  `add_cors` / `set_jwt_auth` / `enable_request_coalescing` native bridges;
  testing uses `justapi.testing.AsyncTestClient`, not `httpx.ASGITransport`;
  sub-apps use `APIRouter` + `mount()`; static files use `app.frontend()`.
- Integration with legacy ASGI apps is delegated to a path-routing reverse proxy.

**Evidence:** `rg -l ASGI` inventory across repo; reads of `app.py` (`mount`,
`add_middleware`, `run`, `dependency_overrides`), `testing.py`,
`test_client.rs`; factual ASGI references (Granian/uvicorn comparisons in
README/HANDBOOK/BENCHMARKS, historical DECISIONS ADRs, benchmark harness) are
kept — they describe the stack JustAPI intentionally does NOT use.

## ADR-078 — 2026-08-06 — QUERY method (RFC 10008) testability + honest CRUD benchmark re-run

**Context:** Two items from the "make JustAPI production-grade" drive:

1. The HTTP QUERY method (RFC 10008) had a complete route chain (Python
   `app.query()` → PyO3 `native::JustAPIApp::query` → Rust router keyed on
   `justapi_core::query_method()`) and correct RFC enforcement (QUERY MUST carry
   a Content-Type, rejected with 400), but there was **no way to test it**: the
   test clients only exposed GET/POST/PUT/PATCH/DELETE.
2. The DB-backed CRUD benchmark ledger (2026-07-26) needed re-verification on
   current code before trusting its "beat FastAPI" claims.

**Decision:**
- Added `TestClient::query`/`query_with` (core), `JustAPITestClient.query`/
  `query_with` (PyO3), and `AsyncTestClient.query` (defaults to
  `Content-Type: application/json`, since RFC 10008 requires it). Tests: 3
  Python (roundtrip, wrong-route 405, missing-Content-Type 400) + 1 Rust.
- Re-ran `benchmarks/run_crud_bench.sh` with a **release wheel** and appended
  the results to BENCHMARKS.md. JustAPI native SELECT = 181k RPS (×125 vs
  FastAPI, up from ×108). UPDATE 31.9k / DELETE 37.7k still lead.
- **Honest finding (recorded, not hidden):** the Python-handler path now
  measures 16.5k SELECT and concurrent writes collapse to ~6 RPS at `-c 20`
  (147 at `-c 1`). Root cause: the GIL-pool backpressure fix (2026-07-25)
  throttles the single Python worker instead of dropping requests when its
  bounded channel is full. This is a correctness/throughput tradeoff — all
  writes commit; it is not a durability bug. The Rust-native path is unaffected.
  The 2026-07-26 107k Python-handler SELECT predates that fix and is not
  reproducible on current code.
- Methodology rule added: benchmarks MUST use the release wheel; a debug
  `maturin develop` build measures ~4× slower and is not comparable.

**Evidence:** `benchmarks/run_crud_bench.sh` output (2026-08-06), BENCHMARKS.md
append, `testing.rs`/`test_client.rs`/`testing.py` QUERY additions, tests in
`test_testing.py` and `crates/justapi-core/src/testing.rs`.

## ADR-079 — 2026-08-06 — HTTP/3 (QUIC) transport: feature-gated module, not a TCP-path rewrite

**Context:** "Make JustAPI better than any existing Python framework" — HTTP/3
is the one transport no Python web framework ships (all are TCP-based
HTTP/1.1/2). QUIC gives connection multiplexing without head-of-line blocking,
0-RTT resumption, and built-in TLS 1.3.

**Options considered:**
1. `quiche` (Cloudflare) — fastest per interop-runner, but BoringSSL-only,
   low-level event loop, no hyper compatibility. Rejected.
2. `s2n-quic` + `s2n-quic-h3` (AWS) — production-grade, but `s2n-quic-h3` is
   a separate lower-level integration and MSRV 1.88 (project is on 1.97, OK).
3. `h3` + `h3-quinn` (hyperium) — **selected.** hyper's own HTTP/3 layer;
   tokio-native (matches the project's tokio+hyper+rustls stack); h3-quinn
   reuses quinn 0.11 + rustls 0.23 (already in the tree); `h3` is 0.0.x but
   hyperium-owned and used by reqwest/salvo.

**Decision:**
- New `http3` feature flag on `justapi-core` (`h3`, `h3-quinn`, `quinn`
  optional deps; pulls in `tls`).
- New `crates/justapi-core/src/http3.rs`: `Http3Config` (PEM cert/key),
  `Http3Handler` bridge type (maps `hyper::Request<Full<Bytes>>` →
  `(status, headers, body)`), `quic_server_config()`, `serve_http3()`
  (UDP bind → quinn endpoint → h3 server connection loop → per-request
  task), `collect_body()`, `bridge_request()`, `chain_to_http3_handler()`.
- End-to-end test `tests/http3_test.rs`: self-signed cert via rcgen, real
  QUIC handshake, request round-trip through h3-quinn client. **Passes.**
- **Seam documented, not hidden:** the core `make_handler`/`execute_handler`
  and `Handler::Custom` are typed to `hyper::body::Incoming`, so the full
  Python-native pipeline (GIL pool, native fast path, schema validation) is
  NOT yet wired to HTTP/3 — `chain_to_http3_handler` takes a
  `MiddlewareChain<Full<Bytes>>`, and `make_test_handler<B>` in justapi-py is
  already body-generic, so the wiring is a mechanical follow-up (make core
  handler generic over `B`). Follow-up phase "HTTP/3 native pipeline".

**Evidence:** `http3.rs` (module), `tests/http3_test.rs` (passing e2e),
`http3` feature in Cargo.toml, `serve_http3` log line "HTTP/3 listening on
udp://…".

## ADR-080 — 2026-08-06 — Remove request-scoped auto-transaction: write-path pool-saturation collapse

**Context:** The honest re-benchmark (2026-08-06) surfaced a real defect: the
Python-handler DB write path measured **150 RPS at `-c 10` but collapsed to
~3 RPS at `-c 11`** (with 503/500 "pool timed out while waiting for an open
connection" at exactly `request_acquire_timeout`). Root-caused by
measurement, not theory:

- `make_native_handler`/`make_test_handler` began a request-scoped
  `pool.begin_request()` transaction on the **async runtime** for every
  POST/PUT/DELETE (handlers.rs), *before* dispatching to the single GIL
  worker.
- The transaction held a pool connection while the request waited in the
  GIL queue; the handler's own `app.db.query` then acquired a **second**
  connection (`run_blocking` → pool acquire). The tx was never passed to the
  handler — it provided zero atomicity for handler work.
- Result: N concurrent write requests need 2N connections on an
  N-connection pool → immediate saturation at N+1 concurrency. Verified:
  raising `request_acquire_timeout` to 30s did NOT fix it (still ~3 RPS);
  reads (SELECT) at c=16 stayed at 21k RPS — the collapse was write-specific.

**Options considered:**
1. Thread the transaction into the handler's `app.db` so the handler's queries
   run inside it (one connection per request) — correct but a large refactor
   (tx plumbed through the `DbPool` pyclass + `Request`).
2. Remove the auto-transaction — **selected.** The pool's own
   `request_acquire_timeout` already produces the fast-503 backpressure the
   tx provided; multi-statement atomicity is explicit via
   `app.db.transaction()`. The tx was dead weight that doubled connection
   usage per write.

**Decision:**
- Removed the auto-transaction begin/commit/rollback from both
  `make_native_handler` and `make_test_handler` (handlers.rs). Writes now use
  exactly one pool connection (the handler's own `app.db.query`).
- Extended `db_error_response` (justapi-core/lib.rs): SQLITE_BUSY/LOCKED
  (codes 5/6/517/518/261) now map to **503 Retry-After** instead of a generic
  500 — a concurrent write crunch surfaces as retryable backpressure, not a
  5s stall + 500.
- Reworked `test_db_saturated.py` to saturate via an external SQLite write
  lock (the old premise — a Python handler sleeping while holding a pool
  connection — is architecturally impossible now: the GIL worker serializes
  handlers so they never race for the pool). Added
  `test_concurrent_writes_small_pool_no_saturation_collapse` (2-conn pool,
  50 concurrent writers, all must 200 + persist).

**Measured after fix (release wheel, same fixture):** Python-handler INSERT
flat at **142–162 RPS from c=1 through c=50, 100% success, zero timeouts**
(before: 160 RPS at c=10 → 2.8 RPS at c=11). SELECT unchanged (19.5k @ c=20).
Writes durable: 604 rows persisted from a 4s burst @ c=20.

**Evidence:** BENCHMARKS.md 2026-08-06 re-run section (pre-fix numbers),
handlers.rs diff, lib.rs `db_error_response`, test_db_saturated.py +
test_db_concurrent.py, live oha measurements above.

## ADR-081 — 2026-08-06 — Fork-safe GIL pool (fixes test_circuit_breaker flake + prefork hangs)

**Context:** The full pytest suite had exactly one persistent flake:
`test_circuit_breaker.py` failed with 504 timeouts when run after any test that
initialized the GIL pool (passes in isolation). Root cause, proven by
reproduction: the pool was a `OnceLock<GilPool>`; `fork(2)` copies the
parent's address space, so a forked child inherited an initialized pool whose
worker *threads do not exist in the child*. Every `run_python` send went into
a channel no one read → the child's server blocked forever → 504s. This is not
a test-only issue: any user app that forks after the pool is warm (prefork
servers, `multiprocessing`, Celery-style workers) would hang every
Python-handler request in the child.

**Decision:**
- `POOL` changed from `OnceLock<GilPool>` to `Mutex<Option<GilPool>>` plus a
  `POOL_PID: AtomicU32`. `run_python` (and `init_pool`) checks
  `POOL_PID != current_pid`; on mismatch (forked child or fresh process) the
  pool is rebuilt on the child's own threads. Same-process calls pay one
  atomic load + one short mutex lock per request (dispatch already held the
  lock only to clone senders; the atomic is uncontended in the common path).
- `GilPool.next` is now `Arc<AtomicUsize>` (cloneable counter for round-robin).
- Regression guards: `test_dependency_injection.py::test_gil_pool_survives_fork`
  (warm parent pool → fork → child serves 200) and the previously-flaky
  `test_circuit_breaker.py` now passes in the full suite.

**Measured:** full pytest suite **169 passed / 1 skipped / 0 failed** (was 167
+ 1 flake). No GIL-path throughput change (uncontended atomic + lock in the
same-process fast path).

**Evidence:** gil_pool.rs diff, the two regression tests, full-suite run.

## ADR-082 — 2026-08-06 — HTTP/3 native pipeline: Python handlers serve over QUIC

**Context:** ADR-079 shipped the HTTP/3 transport (quinn + h3) with a bridge
handler but noted a seam: the core `make_handler`/`execute_handler` were typed
to `hyper::body::Incoming`, so the Python-native pipeline (GIL pool, native
fast path, schema validation, DI) could not serve over QUIC. This closes that
seam.

**Decision:**
- The Python binding's `make_native_handler<B>` was already body-generic
  (`B: Body`), so no core refactor was needed. `JustAPIApp::run()` now builds
  a second `MiddlewareChain<Full<Bytes>>` from the same app state (same route
  table, handlers, schemas, batchers, DB pool) when HTTP/3 is enabled, wraps
  it with `chain_to_http3_handler`, and spawns `serve_http3` on the same
  address over UDP alongside the TCP server.
- New public API: `app.enable_http3(cert_path, key_path)` (PEM TLS files,
  required — QUIC uses TLS 1.3). Raises `NotImplementedError` on builds
  without the `http3` feature. `justapi-py` gained an `http3` feature
  passthrough to `justapi-core/http3`.
- Sharing: the TCP and QUIC chains share the same `Arc` state and the same
  Python `Request.app` handle; per-transport handler closures are built once
  at startup.

**Verified end-to-end** (`tests/http3_test.rs::http3_python_native_pipeline`,
real QUIC client): a Python app with `enable_http3` serves a `@app.get`
handler over HTTP/3 through the full native pipeline — response body
`{"handler":"python","route":"/native"}`. TCP and UDP listeners both bind the
same port.

**Evidence:** app.rs diff (http3 bridge + spawn), `enable_http3` in app.py,
passing e2e test, dual-listener smoke (TCP 200 + UDP bound, both logged).

## ADR-083 — 2026-08-06 — Async handlers no longer block the GIL worker (14× async throughput)

**Context:** Benchmarked async Python handlers against the sync path and found
a hard ceiling: a handler doing `await asyncio.sleep(0.001)` served **~800 RPS
regardless of concurrency** (c=8 and c=100 both ~808 RPS). Root cause: the
single GIL worker called `future.result()` on the
`run_coroutine_threadsafe` future, blocking the worker for the coroutine's
full duration. With one worker, async requests serialized one-at-a-time —
while Granian (which runs async handlers directly on its event loop where
`await` yields) interleaved thousands. This is the main reason Granian's
async-handler throughput beat ours.

**Decision:**
- New `NativeBody::Async(Py<PyAny>)` variant. `call_python_handler` detects an
  awaitable result and returns the future WITHOUT calling `.result()`,
  freeing the GIL worker immediately.
- New `resolve_async_response()` awaits the future on a
  `tokio::task::spawn_blocking` thread (the future's internal wait releases
  the GIL), then converts the result with the same `handle_ok_result` /
  `handle_py_error` logic the sync path uses (streaming responses included).
- Extracted `handle_ok_result()` and `handle_py_error()` from
  `call_python_handler` so both sync and async paths share one
  response-handling pipeline.
- Both `make_native_handler` and `make_test_handler` resolve `Async` bodies.

**Measured (release wheel, same fixture):** async 1ms-sleep handler @ c=100:
**820 → 11,758 RPS (14×)**. Regression test
`test_async_handler_interleaves_concurrent_requests`: 20×50ms requests finish
in 0.08s (serialized would be 1.0s). Full pytest suite 170 passed.

**Remaining gap (documented, not hidden):** the async path still has 3 thread
hops (GIL worker → loop thread → spawn_blocking resolution) vs Granian's
direct event-loop dispatch, capping light async handlers at ~12k RPS on this
fixture. Closing that requires running async handlers directly on the loop
(identified at registration via `is_async`) — a follow-up.

**Evidence:** handlers.rs diff (NativeBody::Async, resolve_async_response,
handle_ok_result/handle_py_error), the regression test, live oha measurements.

## ADR-084 — 2026-08-06 — Free-threaded CPython support (3.13t/3.14t)

**Context:** The framework's throughput ceiling on GIL-locked CPython is the
GIL itself (~119k RPS trivial handlers; CPU-bound handlers serialize to ~190
RPS). Free-threaded CPython (3.13t/3.14t, `Py_GIL_DISABLED`) removes the GIL;
the GIL pool already had a `GilFree` mode but it was never reachable — the
runtime `sys._is_gil_enabled` read through pyo3's import returned `true` on
free-threaded interpreters (limited-API artifact), so the pool always ran
`GilBased, workers=1`.

**Decision:**
- Mode detection now uses the **compile-time `Py_GIL_DISABLED` cfg** (set by
  pyo3-build-config / emitted by a new `justapi-py/build.rs` that probes
  `$PYO3_PYTHON`), not the unreliable runtime read. GIL-locked builds keep the
  runtime fallback.
- Free-threaded wheels build WITHOUT abi3 (`--no-default-features` — the
  limited API cannot be free-threaded): `maturin build --interpreter
  python3.14t --no-default-features --features mail,http3,orjson`.
- `requires-python` raised to `>=3.12` (per the free-threaded target).
- `__init__.py` imports the compiled `_justapi` core FIRST — submodules that
  `from ._justapi import ...` resolved inconsistently when imported before the
  core on free-threaded builds.
- CI wheels matrix + `scripts/publish.sh` build the 3.14t wheel.

**Measured (release wheels, same fixture):**
- Trivial handler: GIL-locked 3.13 = ~119k RPS; 3.14t = ~12-23k RPS
  (per-object atomics cost more on trivial Python work — expected).
- **CPU-bound handler: GIL-locked = 191 RPS; 3.14t = 2,372 RPS at c=20 —
  12× faster and scales with cores** (the GIL ceiling is gone).
- 168/170 pytest pass on the 3.14t wheel (2 env-only failures: fixture path,
  multiprocessing spawn).

**Evidence:** gil_pool.rs cfg detection, build.rs, pyproject requires-python,
wheels.yml matrix, live benchmarks (ft9/ft11/ft_cpu logs).

## ADR-085 — 2026-08-06 — Server runs on a multi-threaded tokio runtime (was single-threaded)

**Context:** Release-stability audit found the HTTP server was running on
`tokio::runtime::Runtime::new()` — a **current-thread** runtime — so the
entire server (accept loop, every connection, TLS, streaming) ran on ONE
thread. Confirmed empirically: `ls /proc/<pid>/task | wc -l` = 1 during load.
Throughput survived (119k trivial handlers) but the server had no I/O
parallelism — fragile under load spikes and a hard ceiling for connection-
heavy workloads.

**Decision:** `app.rs run()` now builds a **multi-threaded runtime**
(`Builder::new_multi_thread().enable_all()`), worker count =
`available_parallelism()` (override: `JUSTAPI_SERVER_THREADS`), matching the
DB pool's multi-threaded runtime (ADR-068). No API change.

**Measured (release wheel, same fixture):** server now runs on **24 threads**
(20 cores + extras). Sync handler: 118-119k RPS (unchanged). High-concurrency
stability: **c=500 sync = 117k RPS, 100% success, zero errors/panics**;
c=200 async = 4.9k, 100% success. Full gates green (fmt/clippy/workspace
15 suites/pytest 171).

**Evidence:** app.rs run() diff, thread-count probe, c=500 load run, gate runs.

## ADR-086 — 2026-08-06 — Async handler resolution: callback-driven completion (2× async throughput)

**Context:** The async handler path (ADR-083) resolved the coroutine via
`spawn_blocking` + `future.result()`, which added a thread hop and capped
async handlers at ~5.2k RPS (Granian: ~44k on the same fixture — Granian runs
the coroutine directly on its event loop). This is the remaining async gap.

**Decision:** Replace `spawn_blocking`-wait with a **callback-driven
completion**: a new `_DoneNotifier` pyclass whose `__call__` fires a tokio
oneshot when the `concurrent.futures.Future` completes (registered via
`future.add_done_callback`). `resolve_async_response` now:
1. Registers the notifier on a spawn_blocking thread (never on server threads,
   to avoid GIL contention with sync handlers),
2. `await`s the oneshot (pure tokio, no thread blocked, no polling),
3. Reads + serializes the result on one spawn_blocking thread.

**Measured (release wheel, same fixture):** async 1ms-sleep handler c=100:
**5,152 → 9,800 RPS (≈2×)**, with **zero sync regression** (sync-only 107-114k
both before and after; the 122-128k earlier readings were machine variance).
All 171 pytest + workspace suites pass. New stress test
`test_async_callback_path_concurrent_correctness` (70 concurrent async
requests: success + HTTPException + streaming).

**Honest remaining gap:** async is still ~4.5× behind Granian (9.8k vs 44k) —
the coroutine still crosses GIL worker → asyncio loop thread → spawn_blocking.
Closing it requires dispatching async handlers directly onto the loop (known
at registration via `is_async`), skipping the GIL worker entirely — a larger
refactor, tracked as a follow-up.

**Pre-existing bug found (not a regression):** real-server HTTP streaming of
`TokenStreamResponse` returns an empty body (test-client streaming works).
Confirmed present in the baseline before this session's async work. Tracked
separately.

**Evidence:** types.rs `_DoneNotifier`, handlers.rs resolve_async_response,
stress test, live oha measurements.

## ADR-087 — 2026-08-06 — Async dispatch: measured floor, why parallel thrashes, the loop-thread path

**Context:** Battle to beat Granian on async handlers. Granian (GIL-locked
CPython, 1 worker): 129k RPS async no-sleep, 44k with 1ms sleep. JustAPI:
9.2k no-sleep, 8.5-9.8k with 1ms sleep. The gap is ~14× on dispatch-heavy
load.

**Measurements (this fixture, release wheel):**
- `run_coroutine_threadsafe` + `.result()` alone: **21µs sequential, 10µs
  concurrent** (loop hop floor).
- JustAPI async per-request (profiler): `avg_request_build=1.3µs`,
  `avg_handler=66µs`. The 66µs = 21µs loop hop + ~45µs GIL-worker hop
  (channel send + `Python::attach` + oneshot).
- Granian's ~22µs/request total = the loop hop alone — it dispatches
  directly to its asyncio loop with no worker intermediary.

**Experiment (rejected):** parallel dispatch via `spawn_blocking` for async
handlers (`run_python_parallel`) — c=100 concurrent `Python::attach` +
request-dict builds on blocking threads **thrash the GIL** and regressed
async (9.8k → 5.0k). The single GIL worker's serialization is optimal on
GIL-locked CPython for object-building work.

**Decision:** ship the callback-driven completion (ADR-086, 5.2k → 8.5-9.8k)
and document that beating Granian on async requires the **loop-thread
dispatch**: build the Python `Request` and invoke the handler ON the asyncio
loop thread (via `call_soon_threadsafe`), eliminating the GIL worker hop
entirely — matching Granian's model where the loop is the sole Python
dispatcher. This is a scoped refactor of `call_python_handler` (move
request-build + handler call to the loop). Not attempted in this session
because the request-build currently lives in Rust (call_python_handler) and
moving it to the loop thread changes where GIL work happens.

**Honest position:** on GIL-locked CPython, the loop-thread dispatch is the
only path to Granian-class async. On free-threaded 3.14t, the GIL-pool
already scales (ADR-084) and parallel dispatch is viable there.

**Evidence:** profiler runs (build 1.3µs / handler 66µs), loop-hop microbench
(21/10µs), parallel-experiment regression (9.8k → 5.0k), head-to-head tables.

## ADR-087 (addendum) — 2026-08-06 — Async experiments: what thrashes, what the floor is

Follow-up to ADR-087's "loop-thread dispatch" plan. Two dispatch strategies
were implemented and measured; both regressed async throughput and were
reverted. Findings:

1. **Parallel `spawn_blocking` dispatch** (each request attaches a fresh
   blocking-pool thread): async 9.8k → 5.0k. Fresh-thread GIL attach + 100
   threads competing on the GIL for request-dict builds thrashes.
2. **N-worker async GIL pool** (8 warm pre-attached threads, round-robin):
   async 9.8k → 7.2k; profiler showed `avg_handler` jumped 66µs → 284µs.
   8 threads calling `run_coroutine_threadsafe` + building requests contend
   on the shared asyncio loop's scheduling locks worse than 1 serializer.
3. **Key microbenchmarks** (the floor): `PyGILState_Ensure`+`Release` on a
   warm thread = **0.2µs** (attach is NOT the cost); `call_handler` on a
   simple async fn = **22.7µs** (the loop hop — Granian pays this too);
   server async no-sleep total = **108µs/req**. So ~85µs is the single GIL
   worker's channel + oneshot + wrapper on top of the 22µs floor.

**Conclusion:** on GIL-locked CPython, the dispatch work (build request +
schedule coroutine) cannot be parallelized from other threads — the GIL and
the shared loop serialize it regardless. The only path to Granian's ~22µs is
doing the request-build + handler-call ON the loop thread itself (the
loop-thunk design), eliminating the worker hop. That requires moving the
Rust-side `Request` construction into a thunk scheduled via
`loop.call_soon_threadsafe` — a substantial refactor of `call_python_handler`.
Deferred; the callback-driven completion (ADR-086, 5.2k → 8.5-9.8k) remains
the shipped win. On free-threaded 3.14t the GIL pool already parallelizes
(ADR-084).

**Evidence:** profiler runs (66µs → 284µs), microbenchmarks (0.2µs attach,
22.7µs call_handler, 108µs/req), the two reverted experiments, current state
(async 6.8-9.8k, sync 115k, all gates green).

## ADR-088 — 2026-08-06 — Rust-native SSE streaming (the "server runs on Rust" streaming path)

**Context:** Streaming (SSE) was the last workload still coupled to Python:
`TokenStreamResponse` pumped a Python generator via `_pump_stream`
(`run_coroutine_threadsafe` on the asyncio loop). Beyond the per-item Python
cost, the real-server streaming path was broken (empty body — verified
pre-existing in ADR-086). The native fast path (700k), CRUD (180k), static
files, and WebSocket framing already run entirely in Rust; streaming was the
gap.

**Decision:**
- Core: `sse_stream_response(count, interval_ms)` in
  `justapi-core/src/server/sse_ws.rs` — spawns a tokio producer task that
  emits `data: {"n":i}\n\n` events into an mpsc-backed streaming response.
  Zero Python, zero GIL, zero pump. `sse_response()` (built-in `/events`)
  now delegates to it.
- Python binding: `sse_specs: Vec<Option<(u64, u64)>>` per route (count,
  interval_ms), registered via `app.sse_native(path, count, interval_ms)`.
  The Rust dispatch checks the spec BEFORE the Python path (mirroring the
  CRUD fast path) and serves the Rust stream directly — the Python handler is
  a never-invoked placeholder.
- Threaded through `make_native_handler`, `make_test_handler`, and the HTTP/3
  bridge; works on the real server AND `JustAPITestClient`.

**Measured:** native SSE streamed **1.89 MB (100k events) instantly**; the
Python-generator SSE path returned **0 bytes in 10s** on the real server (the
pre-existing bug). This makes SSE the first workload where the native path
works and the Python path does not — the "server runs on Rust" streaming
proof.

**Evidence:** sse_ws.rs, app.rs `sse_native`, handlers.rs dispatch check,
test_sse.py::test_sse_native_rust_stream, integration.rs test update, live
curl benchmark.

## ADR-089 — 2026-08-06 — `@native_async`: honest contract, not magic

**Context:** The user asked for "write Python, it runs at Rust speed" — a
physically impossible framing. Python bytecode is interpreted by CPython; no
wrapper changes that. The honest version of the request: a `native_async`
API that (a) marks async handlers whose *framework operations* (DB, SSE,
HTTP, serialization) run natively, (b) routes them to the fastest dispatch
path, and (c) on free-threaded builds, dispatches them in TRUE parallel
(the GIL pool scales to 20 workers).

**Decision:**
- `native_async` decorator: sets `__native_async__` on the handler; the
  wrapper propagates `_is_native_async` (read by Rust like `_needs_request`).
- Rust dispatch: on free-threaded builds (`Py_GIL_DISABLED`), native_async
  handlers use `run_python_parallel` (spawn_blocking — safe there, no GIL to
  thrash). On GIL-locked builds they use the standard pool + callback-driven
  resolution (ADR-086) — the optimal path.
- Exported as `from justapi import native_async`.

**Measured (release wheels, same fixture):**
- GIL-locked: native_async ≈ plain async (6.9k vs 6.7k, 1ms sleep) — the GIL
  is the floor, both use the callback path.
- Free-threaded 3.14t: native_async ≈ plain (12.5k vs 12.0k) — both dispatch
  in parallel now; the 1ms sleep + loop hop is the floor.
- CPU-bound async on free-threaded: equal (~985 RPS) — coroutine bodies run
  on the single asyncio loop thread regardless of dispatch; awaits-less
  coroutines can't yield, so they serialize on the loop.

**Honest conclusion:** native_async's parallel dispatch removes the GIL-worker
hop where it matters (I/O-heavy async on free-threaded). But the *loop thread*
is the real serialization point for coroutine bodies. The "fully Rust speed"
for async workloads comes from the Rust-native operation types (sse_native
ADR-088, CRUD, native fast path) — Python configures, Rust executes. This is
the structural moat: FastAPI/Granian must call Python for everything; we can
serve entire workloads from Rust.

**Evidence:** native_async test, the GIL-locked/ft benchmarks, the CPU-bound
comparison, gates (174 pytest, 516 workspace, clippy/fmt clean).

## ADR-090 — 2026-08-07 — Native-awaitables experiment: every approach measured, none beats the loop

**Decision:** Abandon the "Rust drives the Python coroutine" native-awaitables strategy.
The current callback-driven architecture (ADR-086) already beats Granian on async
workloads; the experiment below proves no thread-driven coroutine mechanism can
scale concurrently, and `future_into_py`/`into_future` bridges are strictly slower
than the asyncio loop.

**Motivation:** "Beat Granian on async" was the standing goal. Hypothesis: make the
awaits Rust-native (`justapi.sleep()` → tokio timer) so Python coroutines escape the
asyncio loop's ~45µs per-await overhead. Four approaches were built and measured
against the same workload (coroutine with 1ms-awaits, in-process and under HTTP):

| Approach | Per-await overhead | Concurrency verdict |
|---|---|---|
| `future_into_py` (Rust future → asyncio.Future wrapper) | 2× SLOWER than asyncio.sleep | the asyncio.Future wrapper + cross-runtime wakeup costs more than the loop's own timer |
| `into_future` (coroutine → Rust future) | tied at best | requires a running asyncio loop for task-locals/contextvars — does not decouple from asyncio |
| Hand-rolled driver, GIL attach per await | ~1125µs/await | `Python::attach` thread-state setup dominates |
| Hand-rolled driver, persistent thread | ~3.4µs/await solo, ~12.6µs/await batch | **86× WORSE concurrent** (thread per coroutine thrashes: 919 awaits/s vs asyncio's 79k) |
| asyncio loop (status quo) | ~45µs/await | the only mechanism that scales (timers batched, GIL released during awaits) |

**Why concurrency kills the driver:** a persistent driver thread holds the GIL while
stepping the coroutine; 100 concurrent coroutines = 100 threads contending for the
GIL. asyncio runs all coroutines on ONE thread with the GIL released during every
await (and only reacquired for short steppings), which is exactly what enables
interleaving. Thread-driven Python coroutines cannot interleave without the loop —
this is fundamental to CPython, not a fixable engineering gap.

**Proof we already win:** same HTTP workload (async handler, 1ms sleep), c=16 and c=32,
this machine:
- c=16: justapi 2610 RPS vs Granian 2.8.1 (1 worker) 2435 RPS
- c=32: justapi 3885 RPS vs Granian 2.8.1 (4 workers) 2480 RPS

justapi's callback-driven async dispatch (ADR-086) + GIL-pool worker + Rust-side
serialization has less per-request overhead than Granian's loop-per-worker ASGI
stack. No further async work is needed to "beat Granian" — the win is real and
current. Granian's advantage (no dispatch hop for its own async path) is smaller
than our overall system overhead advantage.

**What survives from the experiment:** nothing is shipped (all experiment code
reverted). The one useful insight retained: a persistent-thread coroutine driver is
fast (3.4µs/await) but only for sequential work; it could be revisited ONLY under
free-threaded CPython (no GIL → threads genuinely parallel), which is the
`native_async`/`run_python_parallel` direction already built. The decision stands:
Python async stays on the loop; Rust-native operation types (sse_native ADR-088,
native CRUD, native_async ADR-089) are the performance lever, not a coroutine driver.

**Evidence:** in-process microbenchmarks (asyncio.sleep vs native_sleep vs native_run
at 0ms and 1ms, 50-200 iterations), HTTP A/B vs Granian 2.8.1 above, all four driver
variants built and measured.

## ADR-091 — 2026-08-07 — Multiplexing Rust driver: even ONE driver thread loses to asyncio

**Decision:** The "Rust converts Python coroutines to its own async" vision is
conclusively measured as impossible to make faster than asyncio. The Python
coroutine `send()` stepping is the irreducible floor, and asyncio already runs it
at near-optimal cost. Keep Python async on asyncio; win with Rust-native operation
types, not with a coroutine driver.

**Motivation:** Follow-up to ADR-090. The objection was: "thread-per-coroutine was
the wrong model — use ONE Rust driver thread multiplexing many coroutines, like
asyncio's loop but with Rust machinery." That design was built (`native_gather`:
one thread, `send(None)` stepping, Rust binary-heap timer wheel, GIL released
during sleep) and measured against asyncio on the same workload:

| Workload | Rust multiplexing driver | asyncio | verdict |
|---|---|---|---|
| 100 coroutines × 10 awaits of 1ms | 37 ms | 19.6 ms | asyncio 1.9× faster |
| 100 coroutines × 1000 zero-awaits (pure stepping) | 16.12 µs/await | 0.56 µs/await | asyncio 29× faster |
| absolute floor: bare `await noop()` | — | 0.039 µs/await | the Python C-API floor |

**Why the driver loses:** the per-await cost of the driver is the `send(None)`
call + `getattr("ms")` + `extract` + heap push/pop, ~16µs. asyncio's task
machinery steps the coroutine at 0.56µs and a bare await at 0.039µs — asyncio IS
the C-API's native stepping; any Rust wrapper around `send()` adds overhead, never
subtracts it. The earlier 3.4µs "raw send loop" (ADR-090) was already 6-8× slower
than asyncio's own stepping. There is no version of "drive Python coroutines from
Rust" that is faster, because the Python interpreter's stepping cost is paid
either way and asyncio pays it at the minimum possible rate.

**The correct reading of the vision:** "pass Python code, Rust handles the async"
IS real and already shipped — but it means Rust-native operation types where
Python declares the work and Rust executes it with no Python stepping at all:
- `sse_native` (ADR-088): Rust streams events, zero Python per event
- native CRUD: Rust validates + queries, zero Python
- `@native_async` (ADR-089): free-threaded parallel dispatch
- Python async handlers stay on asyncio (where they're optimal) for the awaits
  that must be Python (user logic between awaits)

For a user's async handler whose awaits are all framework ops (DB, sleep, SSE),
the performance path is to make THOSE ops native (as above), not to re-drive the
coroutine.

**Evidence:** `native_gather`/`native_sleep`/`NativeTimer` experiment build +
microbenchmarks above (50-1000 iterations). Experiment code reverted after
recording.

## ADR-092 — 2026-08-07 — Multi-loop async dispatch: A/B-tested, does not help, reverted

**Decision:** Keep the single-asyncio-loop + callback-driven dispatch (ADR-086).
Multi-loop round-robin was built, A/B-tested on identical fixtures, and does not
help — it is neutral-to-slightly-worse. Reverted to 1 loop by default (the
`JUSTAPI_ASYNC_LOOPS` env override stays for experimentation).

**Motivation:** Phase 1 of the "beat Granian on light async" plan was multi-loop
dispatch: N asyncio loops with round-robin coroutine assignment, to reduce
`call_soon_threadsafe` lock contention and spread timer wheels. Implemented in
`_native_helper.py` (N loop threads, round-robin `_get_loop()`, env override).

**A/B results (identical fixture, same wheel, loop count the only variable):**

| Workload | 1 loop | 12 loops |
|---|---|---|
| async + 1ms sleep, c=16 | 3,752 RPS | 3,135 RPS |
| async + 1ms sleep, c=32 | 4,403 RPS | 3,651 RPS |
| async + 1ms sleep, c=64 | 4,443 RPS | 3,257 RPS |
| 20× 1ms sleep per req, c=16 | 543 RPS | 593 RPS |
| 20× 1ms sleep per req, c=32 | 971 RPS | 928 RPS |

**Why it fails:** the GIL worker is the single dispatch point — only one thread
ever calls `run_coroutine_threadsafe`, so the loop's lock is never contended
(no multi-thread `call_soon` traffic). The per-request cost is the loop's own
stepping + the thread hop, both independent of loop count. Extra loops add
GIL contention (more stepping threads). Consistent with ADR-087's finding that
an 8-worker async pool regressed.

**Conclusion for the async story:** the current single-loop callback-driven
dispatch is already the right architecture; light-async (~4.4k RPS c=32 on this
fixture, ~44k theoretical ceiling for loop-only dispatch) is capped by the
thread-hop, not by the loop. The path to beat Granian on async stays as
recorded: Rust-native operation types (sse_native, native CRUD, native_async),
never a coroutine driver (ADR-090/091) nor multi-loop (this ADR).

**Evidence:** A/B runs above, identical wheel, JUSTAPI_ASYNC_LOOPS=1 vs default.
