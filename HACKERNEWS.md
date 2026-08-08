Show HN: JustAPI — a Python web framework where the framework itself is Rust

I spent months building a Python web framework where almost nothing in the
request path is Python. Networking, TLS, routing, validation, serialization,
database access — all Rust. Your handler is the only Python in the pipeline.

Why: every Python framework treats "the framework runs in Python" as a given.
FastAPI sits on Starlette on uvicorn, and even Rust-served frameworks like
Granian cross into Python for routing and serialization. I asked what happens
if the framework work never crosses into Python at all.

The measured numbers (one machine, 100 concurrent connections, 30s runs —
ratios matter more than absolutes):

  hello-world:        766k req/s (native fast path)  vs Granian 314k, FastAPI 36k
  JSON echo:          782k req/s                     vs Granian 145k, FastAPI 33k
  route lookup:       51ns on a 500-route table
  native CRUD SELECT: 181k req/s — 125x FastAPI on the same SQLite fixture
  native async DB:    53x faster than the blocking path on slow queries
  memory:             12 MB RSS vs Granian 37 MB, FastAPI 29 MB

The part I'm most proud of is what didn't work.

I spent real effort trying to make asyncio faster by moving it to Rust —
driving Python coroutines from tokio. I built four approaches and measured
every one. The best result was 16 microseconds per await vs asyncio's 0.56
microseconds on pure stepping. The absolute floor, a bare "await noop()" in
asyncio, is 0.039 microseconds. Python's coroutine stepping is the irreducible
cost, and asyncio already pays it at the minimum rate. Any Rust wrapper only
adds overhead. So I stopped, documented it in the ADR log (090-092), and
instead made the operations the coroutines await native — DB queries run on
Rust's tokio runtime with the GIL released, SSE streams never touch Python.

What you get that other Python frameworks don't:

  - HTTP/3 (QUIC) through the full native pipeline (feature-gated)
  - free-threaded CPython 3.14t wheels with auto-detected parallel dispatch
  - Rust-native SSE — zero Python per event
  - multi-worker prefork with auto-scaling in the CLI
  - tokenless PyPI publishing (OIDC trusted publisher — tag push = release)

Bugs I found in my own framework, because I benchmarked instead of assuming:
  - concurrent Python writes collapsed to 3 RPS at 11 connections (a
    double-acquire in request-scoped transactions)
  - the GIL pool wasn't fork-safe — every request 504'd after a fork
  - Pydantic models as body_schema crashed with a TypeError (broke the
    FastAPI migration path)
  - app.run() required an address, so the README's first example crashed
  - the CLI shipped in the wheel was a stub; fixed in 2.0.10
  - Unix-only code wasn't cfg-gated; Windows didn't compile

The honest caveats, because you will find these if you try it:
  - the docs are large and much of it is AI-written; not every page is
    independently verified. If a doc claims something, test it.
  - the LLM/inference features (OpenAI-compatible server, KV cache, etc.) are
    implemented but never run with real GPU weights. No claims.
  - light async handlers (just asyncio.sleep, no I/O) are 4-12k req/s — the
    asyncio floor. Granian is faster on that shape. Measured, not hidden.
  - there is no "beats everything" claim. There is a benchmark ledger and a
    roadmap that marks 17 phases as "implemented, not verified."

I built this with agentic AI as a guide — it helped write a lot of the code
and docs, and it also wrote things that were wrong, which I only caught by
benchmarking and testing everything myself. Treat the AI-written parts with
suspicion.

If you want the real write-up (with architecture diagram and the full async
failure log): https://github.com/swadhinbiswas/JustAPI/blob/main/ARTICLE.md

PyPI: https://pypi.org/project/justapi/
Repo: https://github.com/swadhinbiswas/JustAPI
Benchmark ledger: https://github.com/swadhinbiswas/JustAPI/blob/main/BENCHMARKS.md
ADR log (all 93 decisions, including the failures): https://github.com/swadhinbiswas/JustAPI/blob/main/DECISIONS.md

Swadhin Biswas
