# ROADMAP.md

> **This file is the aspirational list — explicitly labeled "not yet built."**
> Anything here is future work. For the current status of what *is* built, see
> [`PLAN.md`](PLAN.md) (the living phase table) and [`BENCHMARKS.md`](BENCHMARKS.md).

JustAPI is currently a FastAPI-compatible, Rust-accelerated Python web framework
with an MCP/agent-native serving layer. The **native tool registry** (`@app.tool`,
served over `/_system/tools` and the bundled MCP server) is **built** and lives in
Rust (`crates/justapi-py/src/native/app.rs`). **Streaming structured-output
validation** (`@app.stream_json` + the Rust `validate_value` / `ValidatedStreamResponse`
primitives) and **agent session state** (`app.enable_sessions()` + the Rust-backed
`Session` store) are **built** as of v0.2, and a complete runnable demo lives at
`examples/agent_demo/`. This file collects the features we are deliberately *not*
claiming as done.

## Non-goals for v1 (stated up front, not built yet)

These are intentionally out of scope for the first stable release. Don't assume
they exist just because they're listed in marketing copy elsewhere.

- **Custom edge / WASM runtime.** We ship a Rust core + PyO3 bindings and an
  ASGI shim. We are *not* building a bespoke edge/WASM execution environment in
  v1.
- **Full ORM / migrations system.** We provide a database pool + query bridge,
  but not a from-scratch ORM or migration engine. Use the bridge with an
  existing solution.
- **Multi-tenant auth platform.** Per-route JWT/RBAC exists; a full multi-tenant
  identity platform does not.
- **Reinventing validation.** We wrap proven Rust-backed validators (Pydantic
  v2 core / JSON Schema). We are not writing a new validator from scratch.

## Aspirational (future, not yet built)

Reproduced from the original 14-section wishlist. Each item is a candidate
future phase, not a shipped feature.

### Auth & security
- [ ] Passkey / WebAuthn login flows (beyond JWT/OAuth2)
- [ ] OAuth2 provider federation UI
- [ ] Fine-grained, policy-as-code authorization (OPA-style)

### Observability
- [ ] Prebuilt Grafana/Prometheus dashboards shipped as artifacts
- [ ] Distributed tracing across async boundaries with automatic span linking
- [ ] Anomaly-detection on latency/error budgets

### Deployment & ops
- [ ] `justapi deploy` CLI with multi-cloud provisioning
- [ ] Edge/WASM middleware runtime
- [ ] Self-healing workers (automatic respawn + drain)
- [ ] Distributed cache primitive (beyond single-node)
- [ ] Blue/green + canary deployment helpers

### DX & AI-assisted
- [ ] AI-generated docs from route introspection (we expose the data via
      `/_system/help`; the generator is future work)
- [ ] In-editor "generate handler from OpenAPI" assist

### Protocols
- [ ] Full GraphQL federation at scale (basic GraphQL integration exists)
- [ ] gRPC streaming codegen CLI

## How we decide what to build next

Per the project's build brief: every new capability needs working code **and**
at least one test **and** a line in the relevant doc (README / ARCHITECTURE /
ROADMAP) before it is considered shipped. We do not publish selective benchmark
numbers — see [`BENCHMARKS.md`](BENCHMARKS.md) for the full, append-only ledger
including cases where competitors win.
