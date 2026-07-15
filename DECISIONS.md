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

**Status:** Open issue, filed separately from ADR-048. Root-caused (location
known); fix not yet implemented.

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
