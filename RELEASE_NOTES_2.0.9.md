# JustAPI v2.0.9 — Native async DB awaits, Rust-native SSE, tokenless CI publishing

**Release date:** 2026-08-08

The release that makes "Python writes the logic, Rust does everything else"
literal: async DB queries run on Rust's runtime with the GIL released, SSE
streams are generated with zero Python per event, and publishing to PyPI now
happens from a tag push with no token.

---

## Highlights

### ⚡ Native async DB awaits (ADR-093)
`await app.db.query_async(...)` / `execute_async(...)` run SQL on the DB's own
multi-threaded tokio runtime — the asyncio loop is **never blocked**, so slow
queries (Postgres, network DBs) don't serialize other requests.
**Measured: 53× faster than the blocking path on slow queries** (320 vs 6 RPS).

```python
@app.get("/users/{uid}")
async def get_user(request, uid: int):
    row = await app.db.query_async("SELECT * FROM users WHERE id = ?", [uid])
    return row
```

### 📡 Rust-native SSE streaming (ADR-088)
`app.sse_native(path, count, interval_ms)` — events generated entirely in Rust.
Zero Python, zero GIL per event. 100k events stream instantly.

### 🚀 `@native_async` (ADR-089) + callback-driven async (ADR-086)
Mark handlers for the fastest dispatch; true parallel dispatch on free-threaded
CPython 3.14t. `_DoneNotifier` removes a thread hop from every async request.

### 🏷️ Type-checked DX
`py.typed` + complete `.pyi` stubs — **mypy-clean** on user code. 200+ stub
errors fixed (implicit-Optional, missing imports, duplicate defs, missing route
methods).

### 🛠️ Multi-worker prefork — verified
`justapi serve --workers N --scale`: **1.88× at 4 workers** (99.7k RPS),
auto-scale verified.

### 🎨 Modern README
Animated SVG hero (matrix rain), request-pipeline diagram, benchmark chart,
feature matrix — all SMIL, GitHub-safe.

### 🔐 Tokenless publishing
`wheels.yml` publishes to PyPI via **OIDC trusted publishing** — tag push
→ 9-platform wheel matrix → PyPI → GitHub Release. No tokens, no secrets.

---

## Full change log (27 commits)

### Native async DB awaits — `ed4da41`, `8f63e8a`
- `query_async`/`execute_async` on `DbPool` (ADR-093)
- 2 new tests (query/execute roundtrip, 40-concurrent via HTTP)
- Benchmarks recorded: 53× on slow queries; fast queries at parity (~4-4.7k RPS)

### Rust-native SSE — `1281187`, `d8c16dd`, `576a7be`, `96c6ac3`, `1ad2e27`
- `sse_stream_response(count, interval_ms)` in core (ADR-088)
- `app.sse_native()` registration + `sse_specs` route alignment (bug fixed:
  route/vec index desync)
- Test client wiring + tests

### Async dispatch — `3e6fe90`, `8d3c5dc`, `364904a`, `b750899`
- `_DoneNotifier` callback-driven completion (ADR-086)
- `run_python_parallel` for free-threaded dispatch (ADR-087)
- `@native_async` decorator + `_is_native_async` marker (ADR-089)

### DX — `d663625`, `f05b19c`, `4322714`
- `py.typed` + mypy-clean stubs
- Scaffold demos differentiators (`query_async`, `@native_async`, `sse_native`)
- Animated SVG README (hero, pipeline, benchmark, feature matrix)

### Experiments (recorded, not shipped) — `28d6fda`, `bfff404`
- ADR-090/091: native-awaitables — no driver beats asyncio's stepping
  (16µs vs 0.56µs/await); justapi beats Granian on real async (3.9k vs 2.9k RPS)
- ADR-092: multi-loop dispatch A/B — neutral-to-worse, reverted

### CI & Release — `bf007b1`, `0598cf5`, `76a363c`, `3537c75`, `7746001`, `51015a2`, `4188668`, `a9534e5`, `905df9e`
- OIDC trusted publishing (no token) + GitHub Release in-pipeline
- 9-platform wheel matrix: manylinux/musllinux × x86_64/aarch64, macOS arm64
  + x86_64 (zig cross), Windows x64, free-threaded 3.14t — all green
- Windows compile fix: Unix-only APIs `#[cfg(unix)]`-gated
- Dropped PyO3/maturin-action (invalid inputs, broken venv) → direct maturin
  + zig from ziglang.org
- Repo hygiene: 160 MB purged from git history (.git 225 MB → 61 MB)

### Honesty — `5426d7e`
- AI/inference phases (41-52) stay 🟡/🔴 (no real GPU run) and are excluded
  from the release narrative — the story is the verified web framework.

---

## Install

```bash
pip install justapi==2.0.9
```

## Wheels

- manylinux_2_17 + musllinux_1_2 × x86_64 + aarch64
- macOS arm64 + x86_64
- Windows x64
- Free-threaded CPython 3.14t (cp314-cp314t)

## Docs

- [README](https://github.com/swadhinbiswas/JustAPI) — animated overview
- [BENCHMARKS.md](https://github.com/swadhinbiswas/JustAPI/blob/main/BENCHMARKS.md) — full performance ledger
- [CHANGELOG.md](https://github.com/swadhinbiswas/JustAPI/blob/main/CHANGELOG.md) — full history
