# JustAPI: a Python web framework with a Rust core — the story, the numbers, and what I actually learned

> **Read this first.** I built JustAPI — I'm Swadhin Biswas — with agentic AI as
> a guide: AI helped me design, implement, debug, and document a large part of
> this project. I am not claiming this is the best framework in the world, and
> I am not claiming every number here was measured on your hardware. I am
> describing what I built, why I built it, what I verified, and — just as
> important — what I tried that did not work. The documentation site is large,
> much of it written with AI assistance, and not everything in it is
> independently verified. If you find a claim that seems wrong, it probably
> is. Verify it, and open an issue.
>
> **What this article is:** a technical write-up. **What it is not:** marketing.

---

## 1. What JustAPI is

JustAPI is a Python web framework where the web server, the router, the TLS
termination, the request validation, the JSON serialization, and the database
access all run in Rust. Your Python code handles the application logic. That is
the entire premise, and it is the one sentence that explains every design
decision in the project.

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/")
def hello():
    return {"Hello": "World"}

app.run()   # a real server, not a dev server
```

That is a complete program. It gives you HTTP/1.1 and HTTP/2, TLS, routing with
path and query parameters, JSON serialization, automatic OpenAPI docs at
`/docs`, and a server that does not need uvicorn or gunicorn. All of it runs in
Rust. The Python process just runs your handler.

The package is on PyPI (`pip install justapi`), it works on Linux, macOS, and
Windows, and there are wheels for the free-threaded build of CPython 3.14t.

---

## 2. Why I built it (the story)

The starting point was a simple observation that most Python web frameworks
treat as a given: **the framework runs in Python.** FastAPI runs on Starlette,
Starlette runs on uvicorn, and uvicorn is a Python event loop with a lot of
asyncio machinery between the socket and your handler. Robyn and Granian moved
the *server* to Rust, but the request still crosses into Python for routing,
validation, and serialization.

I asked a different question: what if almost nothing crossed into Python?
What if the framework — the thing that does the same work on every request —
lived entirely in Rust, and Python only ran the code that is different for each
application?

That question shaped a project with an unusual constraint, which I wrote down
early and enforced throughout: **if a feature can be implemented in Rust, it
must be implemented in Rust.** Not "can be implemented faster in Rust" — *can
be implemented in Rust*. Python in the framework is reserved for the parts that
have to be Python: the glue between Rust values and your handlers.

I did not do this alone. I worked with agentic AI throughout — using it as an
engineering guide. I made the architecture decisions; the AI helped me turn
them into working code, find the bugs I could not see, write the tests, and
keep the project honest when I wanted to skip the boring parts. There are
parts of this codebase I understand completely, parts I understand well enough
to maintain, and parts the AI wrote that I reviewed and tested but would be
lying if I said I could reproduce from memory. I am not hiding that; it is
part of how this project exists.

The project then went through the standard engineering motions, in order:

1. A minimal server: tokio + hyper, a PyO3 boundary, one route. Measured
   766k req/s on the hello-world fixture.
2. A native router (matchit radix trie, 51ns average lookup on 500 routes).
3. A memory pipeline (per-request arena, buffer pools).
4. Middleware in Rust: CORS, security headers, JWT, rate limiting.
5. Serialization in Rust (serde_json, optional simd-json).
6. TLS with rustls — no OpenSSL dependency.
7. A native API surface, replacing the ASGI shim.
8. Database access via sqlx: SQLite, PostgreSQL, MySQL, DuckDB.
9. WebSockets, SSE, background tasks, a scheduler, a plugin system.
10. Operational tooling: OpenTelemetry, Prometheus metrics, health checks,
    circuit breakers.

None of this was fast in the sense of "one weekend." It was fast in the sense
of "the numbers are in the repo, the tests pass, and the failures are
documented."

The release history tells part of the story on its own:

| Version | What changed |
|---|---|
| 2.0.8 | Multi-threaded tokio server, free-threaded CPython support, async handler fix (14× async throughput) |
| 2.0.9 | Native async DB awaits, Rust-native SSE, type stubs, tokenless CI publishing, first PyPI release |
| 2.0.10 | The CLI shipped in the wheel (it was a stub before) |

---

## 3. The architecture

The request path is a pipeline, and almost every stage is Rust:

```
Kernel (epoll/io_uring)
  → tokio connection manager (multi-threaded)
  → TLS (rustls)
  → HTTP parse (hyper)
  → Router (matchit, ~51ns lookup)
  → Middleware chain (auth, CORS, rate-limit, compression)
  → Python boundary (zero-copy PyO3, GIL pool)
  → Your Python handler
  → Rust serializer (serde_json / simd-json)
  → Response write → socket
```

The crate layout mirrors the pipeline:

```
justapi/
├── crates/
│   ├── justapi-core/       # networking, routing, middleware, serialization, DB pool
│   ├── justapi-py/         # PyO3 bindings + the Python package
│   ├── justapi-cli/        # the `justapi-cli` binary (serve, create, check, profile)
│   └── justapi-bench/      # benchmark harness
├── python/justapi/         # pip-installable package (maturin-built)
├── BENCHMARKS.md           # append-only performance ledger
├── DECISIONS.md            # ADR log (ADR-001 through ADR-093)
└── PLAN.md                 # roadmap with an honesty status on every phase
```

The dependency graph is intentionally small:

```
justapi-cli  → justapi-core
justapi-py   → justapi-core
justapi-bench → justapi-core
```

## 4. The numbers (verified, with the hardware caveat)

All numbers below are from `BENCHMARKS.md`, measured on a single machine with
100 concurrent connections and 30-second runs. **Your machine will differ.**
What matters is the *ratio* on the same hardware, not the absolute numbers.

| Framework | Hello-world req/s | JSON echo req/s | p99 hello-world | RSS |
|---|---|---|---|---|
| JustAPI (native fast path) | 766k | 782k | 0.48 ms | 12 MB |
| Granian (ASGI) | 314k | 145k | 0.74 ms | 37 MB |
| Robyn | 39k | — | — | — |
| FastAPI + Uvicorn | 36k | 33k | 24.63 ms | 29 MB |

The "native fast path" needs two things: a route registered with `native=True`
and a `Schema`. In that configuration, Rust validates the body and writes the
response — Python never runs. Without a schema, you get the Python handler
path, which is GIL-bound at roughly 60–120k req/s on this hardware, same as any
other Python framework.

Other verified numbers:

- Route lookup: 51ns average on a 500-route table.
- Native CRUD SELECT: 181k req/s — 125× FastAPI on the same SQLite fixture.
- Native async DB awaits: 320 RPS vs 6 RPS for the blocking path on a slow
  query (a 2M-row recursive CTE) — 53×.
- Multi-worker prefork (`justapi serve --workers 4`): 1.88× throughput
  (99.7k RPS vs 53k).
- Free-threaded CPython 3.14t, CPU-bound Python handler: 12.4× vs
  GIL-locked — the GIL ceiling disappears.

**What I do not claim:** there is no "beats every framework on every
workload" claim here. Light async handlers (no I/O, just `asyncio.sleep`) sit
at 4–12k req/s — the asyncio loop floor — and I measured that Granian's
direct loop dispatch is faster on that specific shape. I do not hide it; it is
in the benchmarks and in the article below.

## 5. The async story: what I tried, and what the measurements said

This is the most important section in this article, because it is where I
spent the most effort proving myself wrong.

The initial async path was bad. An async handler doing `await
asyncio.sleep(0.001)` served about **808 RPS** — a hard ceiling, not
contention. The single GIL worker was blocking on each coroutine's full
duration, serializing every async request. Fixing that (ADR-083) took async
throughput from ~800 to **11,758 RPS** — 14× — by making completion
callback-driven instead of blocking.

Then I got ambitious. The plan was: "make the awaits Rust-native." If a
Python coroutine's `await` could complete on tokio instead of on the asyncio
loop, per-await overhead would drop, and I would beat Granian on async.

I built four approaches and measured every one:

| Approach | Per-await overhead | Verdict |
|---|---|---|
| `future_into_py` (Rust future wrapped as an asyncio.Future) | 2× slower than `asyncio.sleep` | The wrapper + cross-runtime wakeup costs more than the loop's own timer |
| `into_future` (coroutine → Rust future) | tied at best | Requires a running asyncio loop for task-locals; does not decouple |
| Hand-rolled driver, GIL attach per await | ~1125 µs/await | Thread-state setup dominates |
| Hand-rolled driver, persistent thread | ~3.4 µs/await solo | **86× worse concurrent** (thread per coroutine) |
| asyncio loop (status quo) | ~45 µs/await | The only mechanism that scales |

The multiplexing driver — one persistent Rust thread stepping many coroutines,
the design that should have worked — measured **16 µs/await vs asyncio's
0.56 µs/await on pure stepping** (ADR-091). The absolute floor, a bare
`await noop()` in asyncio, is **0.039 µs**.

The conclusion, written into the ADR log: *the Python coroutine `send()`
stepping is the irreducible floor, and asyncio already runs it at the minimum
possible rate. Any Rust wrapper around `send()` adds overhead; it cannot
subtract it.* A multi-loop dispatch experiment (ADR-092) also came back
neutral-to-worse: the GIL worker is the single dispatch point, so extra loops
only added GIL contention.

I stopped. That is the part of engineering nobody writes articles about: the
work you delete. The async path stays on asyncio, which is the right answer,
and I went back to making the *operations* native instead of the coroutine.

## 6. Native operations: where the real win is

Since I cannot make Python coroutines faster, I made the framework operations
they await faster — by taking Python out of them.

**Native async DB awaits (`query_async`).** A normal async handler doing a
database query would block the asyncio loop thread for the whole query. One
slow Postgres query froze every other async handler on the server.
`await app.db.query_async(...)` runs the SQL on the DB's own multi-threaded
tokio runtime with the GIL released. The loop is never blocked. Measured: 53×
faster than the blocking path on slow queries, and it is the only path that is
*safe* for real databases with network latency.

**Rust-native SSE (`sse_native`).** A stream of events is generated entirely
in Rust — tokio timers, an mpsc channel, the HTTP layer drains it. Zero Python
per event. 100k events stream instantly.

**`@native_async`.** A marker that routes async handlers to the fastest
dispatch path, and on free-threaded CPython enables true parallel dispatch
across workers.

The pattern is consistent: Python declares the operation, Rust executes it,
and nothing in the middle is Python.

## 7. Bugs I found in my own framework (and fixed)

The honest part of any framework article is the bug list. Some highlights:

**Write-path collapse.** Concurrent Python-handler writes dropped to ~3 RPS at
11 connections (from ~150 at 10). The request-scoped auto-transaction was
double-acquiring pool connections — 2N for N writes. Removed. (ADR-080)

**GIL pool not fork-safe.** A forked child inherited the parent's initialized
GIL pool whose worker threads did not exist, so every Python request hung and
returned 504s. The pool now tracks its PID and rebuilds after fork. This was
also the root cause of a long-standing flaky test. (ADR-081)

**Pydantic models as `body_schema` were broken.** `validate_body` treated a
Pydantic model class as a plain callable and invoked
`BaseModel(body_dict)`, which raised `TypeError`. This broke the single most
important FastAPI migration path. Fixed by validating through
`model_json_schema()` in the Rust engine. It shipped fixed in 2.0.9.

**`app.run()` required an address.** The README quickstart showed
`app.run()` with no arguments, which crashed with a `TypeError`. Now defaults
to `127.0.0.1:8000`.

**The CLI in the wheel was a stub.** The PyPI package's `justapi` command
printed a help message and did nothing else. It shipped fixed in 2.0.10
(serve, create, check, openapi — and it delegates to the Rust CLI when
installed).

**Unix-only APIs were not cfg-gated.** `UnixListener`-based functions failed
to compile on Windows. Gated with `#[cfg(unix)]`.

Each of these is in the changelog with the commit that fixed it. None of them
was found by reading the code. They were found by running things, by
benchmarking, and by trying to use the framework the way a user would.

## 8. What JustAPI ships that no other Python framework ships

- **HTTP/3 (QUIC)** — feature-gated, serves Python handlers over QUIC through
  the full native pipeline. As far as I know, the only Python framework with
  a working HTTP/3 transport.
- **Free-threaded CPython wheels** (3.14t) with auto-detected parallel
  dispatch.
- **Rust-native SSE** where the stream never touches Python.
- **Multi-worker prefork with auto-scaling** built into the CLI.
- **OIDC trusted publishing** — the release pipeline uses zero tokens; a tag
  push builds 9 platform wheels and publishes them.

## 9. The honest caveats (read this before using it)

1. **The documentation site is large and much of it is AI-written.** I
   generated a lot of the docs with AI assistance. I verified the code and
   the benchmarks; I did not independently verify every sentence in every
   doc page. If you rely on a doc page, test the behavior yourself. Some doc
   pages were already found to describe APIs that did not exist, and I fixed
   them as I found them. There are probably more.

2. **The LLM/inference features are unverified.** Phases 41–52 of the roadmap
   (inference engine, KV cache, continuous batching, OpenAI-compatible server)
   are implemented but were never run with real GPU weights. The only honest
   status is "implemented, not verified." They are not part of the release
   story, and I do not make performance claims about them. (ADR-067)

3. **The absolute numbers are from one machine.** Ratios matter; absolutes
   depend on hardware, OS, and load generator.

4. **The roadmap contains 17 "implemented but unverified" phases.** I mark
   them 🟡 in PLAN.md. That is not a typo. It means: code exists, tests pass,
   and the real-world claim is not proven.

5. **Async light handlers are loop-bound.** If your handler is
   `await asyncio.sleep(0.001); return {}`, JustAPI is not faster than
   Granian. I measured it. Use the native operations for DB/SSE/IO work.

6. **There is no 1M RPS claim here.** The roadmap phase title once said
   "1M RPS tuning." I never measured 1M RPS. I changed the status to
   reflect that. (The phase overclaims; I say so in PLAN.md.)

7. **First impressions matter, and mine were rough.** The first published
   CLI was a stub. `app.run()` crashed without an argument. The docs had
   fabricated API references. All fixed, but if you are evaluating the
   project, the version history is honest about the rough edges.

## 10. How to actually use it

```bash
pip install justapi==2.0.10
```

```python
# main.py
from justapi import JustAPIApp, native_async
from pydantic import BaseModel

app = JustAPIApp()
app.set_database("sqlite://app.db")

class Item(BaseModel):
    name: str

@app.get("/")
def root():
    return {"Hello": "World"}

@app.get("/items/{item_id}")
async def get_item(item_id: int):
    return await app.db.query_async(
        "SELECT * FROM items WHERE id = ?", [item_id])

@app.post("/items", body_schema=Item)
def create(request):
    data = request.json()
    return app.db.execute(
        "INSERT INTO items (name) VALUES (?)", [data["name"]])

# Rust-native SSE: no Python per event
app.sse_native("/events", count=1000, interval_ms=0)

app.run()   # defaults to 127.0.0.1:8000
```

Then:

```bash
python main.py
justapi check main.py        # validate routes without serving
justapi serve 0.0.0.0:8080   # or use the CLI
```

For the high-performance Rust CLI:

```bash
cargo install justapi-cli    # provides serve --workers N --reload, create, profile
```

## 11. Where to verify everything in this article

| Claim | Source |
|---|---|
| Benchmarks, methodology, hardware | `BENCHMARKS.md` in the repo |
| Architecture decisions (all 93) | `DECISIONS.md` — ADR-001 through ADR-093 |
| Roadmap status incl. unverified phases | `PLAN.md` (the honesty key is at the top) |
| Async experiments and their failure | ADR-090, ADR-091, ADR-092 |
| Native async DB awaits | ADR-093 |
| Bug fixes | `CHANGELOG.md` (2.0.8 → 2.0.10) |
| Code | https://github.com/swadhinbiswas/JustAPI |
| Docs | https://justapi.pages.dev |

---

## 12. The thing I actually learned

The takeaway of this project is not "Rust is faster than Python." Everyone
knows that. The takeaway is more specific and more useful:

**The performance of a Python web framework is decided by how much Python
runs per request — and the parts you cannot remove, you should not try to make
faster by rewriting them in Rust.**

I tried to make asyncio faster by moving it to Rust. The measurements said
no. I made the operations *around* asyncio native instead, and that worked:
53× on slow database queries, 125× on native CRUD, zero-Python SSE. The
framework's job is to decide which parts of a request are yours and which are
the framework's — and then to make the framework's parts not run Python at
all.

That, and the discipline to write down the failures, is what JustAPI is.

There is a second lesson, specific to how I built this: **agentic AI is a
force multiplier, not a substitute for judgment.** The AI guided me through
Rust lifetimes, PyO3 FFI, and tokio internals I had never touched before. It
also wrote things that were wrong, and I only caught them because I
benchmarked, tested, and read the output. Use the tool; verify the work.

---

*Written by Swadhin Biswas, maintainer of JustAPI, with agentic AI as a
guide. Corrections welcome — open an issue at
https://github.com/swadhinbiswas/JustAPI/issues. If a number here disagrees
with your measurement, your measurement wins.*
