# AGENTS.md

> Operating rules for anyone — human or AI — working on justapi Runtime.
> Read this before touching code. If you are an AI agent resuming after a
> context reset, read this file and `PLAN.md` first, in that order.

## 1. The one rule that overrides everything else

Do not begin a phase in `PLAN.md` until the previous phase's exit criteria
are checked off there. If you're unsure whether a gate passed, check
`BENCHMARKS.md` and CI history — don't assume.

## 2. Rust-first implementation mandate

If a feature **can** be implemented in Rust, it **must** be implemented in Rust — not in the
Python package. Python is reserved for application-facing glue and user callbacks only.

This is the runtime's core performance thesis (see `DECISIONS.md` ADR-008: "Rust owns I/O and
scheduling, Python owns application logic"). Concretely, the following belong in Rust and must
**not** be re-implemented under `crates/justapi-py/python/justapi/`:

- Networking, sockets, TLS, connection management.
- HTTP/protocol parsing and response writing.
- Routing, middleware chaining, rate-limiting, auth/JWT/OAuth2 validation.
- Scheduling, worker/thread pools, background-task execution, concurrency primitives.
- (De)serialization of framework data (encoding bodies/responses for the framework, not user
  callbacks).
- Hot paths: parsing, validation, buffering, compression.

Python in the framework package is acceptable only for:

- Thin PyO3 re-exports / binding glue (`from ._justapi import X`).
- Translating between Rust values and Python callables (e.g. invoking a user route/handler).
- User-facing helpers that are not on a hot path and have no Rust equivalent.

Before adding Python logic to the framework, ask: *"Could this live in `justapi-core` /
`justapi-py` Rust instead?"* If yes, put it in Rust.

## 3. Roles

Even in single-agent execution, work through these lenses in sequence for
any non-trivial change — they catch different classes of mistakes:

- **Core Runtime** — networking, protocol, memory, scheduler correctness and
  safety (`justapi-core`).
- **Python Bindings** — PyO3 boundary, GIL/free-threading correctness, the
  native API (`justapi-py`).
- **Protocol/Security** — TLS, auth, JWT, OAuth2/OIDC, request validation,
  SAST scanning, fuzzing, anything parsing untrusted input.
  Every change here needs a fuzz target or an explanation of why not.
- **Data & Validation** — Pydantic-style schemas, DB ORM integration,
  serialization, type-safe request/response contracts, background tasks.
- **Resilience** — circuit breakers, graceful degradation, retry policies,
  rate-limit backpressure, connection pooling.
- **QA/Benchmark** — owns `BENCHMARKS.md`, the CI regression gate, and
  calling out unqualified performance claims. Target: beat FastAPI on every
  benchmark.
- **Platform/DX** — CLI, hot reload, OpenAPI generation, Docker/K8s tooling,
  multi-cloud deployment, `skills/` directory upkeep.
- **Observability** — distributed tracing (OTel), structured logging, metric
  dashboards, health checks, alerting integration.

## 4. Conventions

- **Commits:** `<emoji> <phase>: <what changed>` e.g. `📦 p3: add per-request
  arena allocator`. One logical change per commit.
- **Commit size:** Keep each commit under **100 lines of diff** (`git diff
  --cached --stat`). If a file is over 100 lines, split it into smaller
  files (`types.rs`, `message.rs`, `state.rs` instead of a single
  `mod.rs`). Use `git add -p` to stage partial hunks if needed.
  Exceptions: Cargo.lock (auto-generated) and single cohesive files at
  ~140 lines max.
- **Branches:** `phase-N/<short-description>`.
- **`unsafe` code:** every block needs a `// SAFETY:` comment explaining the
  invariant that makes it sound, and a test that would fail if the invariant
  were violated where feasible.
- **New dependency:** justify it in `DECISIONS.md` — what it replaces or
  enables, why the alternative (usually: writing it yourself) is worse.
- **Public API changes:** rustdoc/docstrings required before merge, not after.
- **Error handling:** use `anyhow::Result` in application code (CLI, bench);
  use typed errors (`thiserror`) in library crates (`justapi-core`,
  `justapi-py`) for errors that cross crate boundaries.

## 5. Before opening a PR / calling a phase done

- [ ] No feature that could be Rust was added in Python (see §2); new framework
      Python is glue only
- [ ] Tests pass (`cargo test --workspace`)
- [ ] `cargo clippy --workspace --tests -- -D warnings` clean (includes test code)
- [ ] `cargo fmt --check` clean
- [ ] `bash scripts/sanitize.sh` clean (if core touched) — ASan on the full core
      suite + a targeted Miri run on the only `unsafe` module (`memory.rs`).
      This replaces the full-suite `cargo miri` run (interpreted = 20+ min for
      mostly-safe code; sanitizers run at near-native speed).
- [ ] Benchmarks run, numbers appended to `BENCHMARKS.md` (not overwritten)
- [ ] No p99 regression >5% vs. previous phase without a `DECISIONS.md` entry
- [ ] `PLAN.md` updated: phase status, next steps
- [ ] New recurring pattern or gotcha? Add/update a `skills/*/SKILL.md`

## 6. Resuming work after a context reset

1. Read `PLAN.md` — find the current phase and its status.
2. Read the most recent `DECISIONS.md` entries for context on why things are
   the way they are.
3. Skim relevant `skills/*/SKILL.md` files for the area you're touching.
4. Check `BENCHMARKS.md` for the last recorded numbers so you know the bar.
5. Continue from where `PLAN.md` says to continue — don't restart planning
   from scratch.

## 7. When something in the master prompt seems wrong

It might be. If a requirement conflicts with something you've learned during
implementation (e.g., a benchmark shows a "required" custom scheduler isn't
needed, or a crate assumption in the master prompt is stale), write the
conflict and your recommendation into `DECISIONS.md` and proceed with the
better-informed choice. Don't silently deviate, and don't blindly follow a
now-wrong instruction either.

## 8. Architecture quick reference

```
justapi/
  Cargo.toml                # workspace root
  crates/
    justapi-core/          # networking, protocol, scheduler, memory
    justapi-py/            # PyO3 bindings, native API
    justapi-cli/           # `justapi` CLI binary
    justapi-bench/         # internal benchmark harness
  python/
    justapi/               # pip-installable Python package (maturin-built)
  skills/                   # reusable knowledge modules
  benchmarks/               # workload scripts for baseline comparisons
  AGENTS.md                 # this file
  PLAN.md                   # living roadmap (single source of truth)
  DECISIONS.md              # append-only ADR log
  BENCHMARKS.md             # append-only results ledger
  PROMPT.md                 # master engineering prompt (reference only)
```

### Request pipeline (target shape)

```
Kernel → epoll/io_uring → Connection Manager → tokio task
  → TLS (rustls) → HTTP parse (hyper) → Router (matchit)
  → Middleware chain (Rust: auth/CORS/rate-limit/compression)
  → Python boundary (typed, zero-copy request view via PyO3 buffer protocol)
  → Application code (native handler)
  → Rust serializer (serde_json/simd-json)
  → Response write → Socket
```

### Crate dependency graph

```
justapi-cli → justapi-core
justapi-py  → justapi-core
justapi-bench → justapi-core
```

## 9. Key skills to consult

| Area | Skill file |
|---|---|
| PyO3 / GIL / unsafe at the FFI boundary | `skills/rust-ffi-safety/SKILL.md` |
| Running & interpreting benchmarks | `skills/benchmark-harness/SKILL.md` |

Add new skills as patterns emerge. A good skill saves the next session from
rediscovering a non-obvious invariant or workflow.
