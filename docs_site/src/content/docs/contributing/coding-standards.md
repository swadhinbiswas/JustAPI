---
title: Coding Standards
description: Conventions and guidelines for contributing to JustAPI — a high-performance FastAPI alternative built in Rust.
keywords: coding standards, contribution guide, FastAPI alternative, Rust web framework, developer guidelines
---

## Rust-First Mandate

If a feature **can** be implemented in Rust, it **must** be implemented in Rust. Python is reserved for:

- Thin PyO3 re-exports (`from ._justapi import X`)
- Translating Rust values to Python callables
- User-facing helpers with no Rust equivalent

The following **must live in Rust**:
- Networking, sockets, TLS, connection management
- HTTP/protocol parsing and response writing
- Routing, middleware, rate-limiting, auth
- Scheduling, worker/thread pools
- (De)serialization of framework data
- Hot paths: parsing, validation, buffering, compression

## Commit Messages

```
<phase>: <what changed>
```

Examples:
- `p3: add per-request arena allocator`
- `p42: implement paged KV cache`

One logical change per commit.

## Branch Naming

```
phase-N/<short-description>
```

Examples:
- `phase-42/paged-kv-cache`
- `phase-51/scheduler-engine`

## `unsafe` Code

Every `unsafe` block must have a `// SAFETY:` comment explaining the invariant:

```rust
// SAFETY: The GIL is held, so the pointer is valid.
let ptr = obj.as_ptr();
```

Whenever feasible, add a test that would fail if the invariant were violated.

## Error Handling

| Code Location | Error Type |
|---|---|
| Application code (CLI, bench) | `anyhow::Result` |
| Library crates (core, py) | Typed `thiserror` |

## Public API Changes

All new public APIs require documentation (rustdoc or Python docstrings) before merge.

## Pre-Submission Checklist

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --tests -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] `cargo miri test -p justapi-core` clean (if core touched)
- [ ] Benchmarks appended to BENCHMARKS.md (no p99 regression >5%)
- [ ] PLAN.md updated

## See Also

- [Development Setup](/contributing/development-setup/) — Get started
- [Testing Guide](/contributing/testing-guide/) — Writing tests
- [Documentation Guide](/contributing/documentation-guide/) — Writing docs
