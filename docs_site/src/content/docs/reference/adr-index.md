---
title: ADR Index
description: Architecture Decision Records — key decisions that shaped JustAPI, a high-performance FastAPI alternative built in Rust.
keywords: architecture decision records, ADR, FastAPI alternative, Rust web framework, technical decisions
---

JustAPI documents significant architectural decisions as Architecture Decision Records (ADRs) in `DECISIONS.md`.

## Architecture & Design

| ADR | Title | Key Point |
|---|---|---|
| 001 | Crate defaults for v1.0 | Workspace structure and crate responsibilities |
| 005 | Rust edition: 2021 | Deliberately stayed on 2021 edition |
| 008 | GIL strategy | Dedicated Python worker thread (later superseded) |
| 014 | Project rename: Hyperion → JustAPI | Rebranding rationale |
| 021 | GIL-free architecture | Removed dedicated Python worker thread |

## Performance

| ADR | Title | Key Point |
|---|---|---|
| 006 | Benchmark tooling: oha over wrk | Chose oha for consistency |
| 020 | Performance target | Beat FastAPI on every benchmark |
| 024 | Bucket-based latency histograms | Custom over HDR Histogram |
| 047 | Remove per-request Python↔Rust boundary | Native fast path |
| 056 | Rust-native route compiler | Skip GIL on common routes |
| 064 | Route-lookup cache | Memoization for repeated lookups |

## Security

| ADR | Title | Key Point |
|---|---|---|
| 018 | Security strategy | SAST in CI, fuzzing, OWASP baseline |
| 050 | Secure-by-default CORS | Opt-in security headers |
| 052 | Single error-response contract | `{"detail": ...}` format |
| 053 | Panic model: abort + supervisor | Not catch_unwind |
| 055 | Configurable max body size | 413 on overflow |

## Database

| ADR | Title | Key Point |
|---|---|---|
| 016 | Database strategy | sqlx with compile-time checks |
| 058 | Eager/early DB pool connect | Removes `app.db is None` footgun |
| 069 | Fix DB-backed handler deadlock | WAL hang fix |
| 074 | DB-startup + response-correctness | Fixed normalize_db_url |

## Inference

| ADR | Title | Key Point |
|---|---|---|
| 030 | Native inference engine | Candle, not torch delegation |
| 036 | Disaggregated prefill/decode | Separate prefill and decode resources |
| 068 | Freeze AI/inference work | Until 2.0.8 release |

## Full List

All 75+ ADRs are documented in [`DECISIONS.md`](https://github.com/swadhinbiswas/JustAPI/blob/main/DECISIONS.md) at the repository root.

## See Also

- [Architecture Overview](/getting-started/overview/) — High-level design
- [Rust Core Deep Dive](/advanced/rust-core-deep-dive/) — Runtime internals
- [Release Notes](/reference/release-notes/) — Version history
