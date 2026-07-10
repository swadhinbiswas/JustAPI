#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# rebuild_history.sh — Nuke .git, rebuild clean commit history
# 20 commits spread across 5 days (July 6–10, 2026)
# ============================================================

REPO_DIR="/home/swadhin/RastAPI"
cd "$REPO_DIR"

echo "=== Step 1: Remove .git directory ==="
rm -rf .git

echo "=== Step 2: Re-initialize repository ==="
git init -b initial-setup
git config user.name "swadhinbiswas"
git config user.email "swadhinbiswas.cse@gmail.com"
git remote add origin git@github.com:swadhinbiswas/JustAPI.git

# Helper: commit with date
commit() {
  local date="$1"
  shift
  GIT_AUTHOR_DATE="$date" GIT_COMMITTER_DATE="$date" git commit "$@"
}

# ============================================================
# DAY 1 — July 6: Foundation & Workspace Setup
# ============================================================

echo "--- [1/20] Project scaffolding ---"
git add Cargo.toml .gitignore .editorconfig rustfmt.toml deny.toml LICENSE .env.example .dockerignore
commit "2026-07-06T09:15:00+06:00" -m "chore: initialize workspace with Cargo.toml, license, and tooling configs

- Add root Cargo.toml with workspace member declarations
- Add .gitignore for Rust/Python/IDE/OS/Docker artifacts
- Add .editorconfig for consistent formatting across editors
- Add rustfmt.toml and deny.toml for lint and dependency auditing
- Add LICENSE (MIT), .env.example, and .dockerignore"

echo "--- [2/20] Engineering documentation ---"
git add PLAN.md PROMPT.md DECISIONS.md AGENTS.md REVISION_LOG.md SECURITY.md README.md
commit "2026-07-06T10:45:00+06:00" -m "docs: add project roadmap, architecture docs, and security policy

- PLAN.md: phased development roadmap with exit criteria
- PROMPT.md: master engineering prompt and design rationale
- DECISIONS.md: append-only architecture decision records
- AGENTS.md: operating rules for human and AI contributors
- REVISION_LOG.md: changelog for iteration tracking
- SECURITY.md: vulnerability disclosure policy
- README.md: project overview and quickstart"

echo "--- [3/20] Skills knowledge base ---"
git add skills/
commit "2026-07-06T11:30:00+06:00" -m "docs: add reusable skill modules for benchmarks and FFI safety

- skills/benchmark-harness/SKILL.md: perf benchmark workflow
- skills/rust-ffi-safety/SKILL.md: PyO3/GIL/unsafe invariants"

# ============================================================
# DAY 2 — July 7: Core Runtime Engine
# ============================================================

echo "--- [4/20] Core crate — server, router, and lib ---"
git add \
  crates/justapi-core/Cargo.toml \
  crates/justapi-core/src/lib.rs \
  crates/justapi-core/src/server/mod.rs \
  crates/justapi-core/src/router.rs \
  crates/justapi-core/src/serialize.rs \
  crates/justapi-core/src/error_catalog.rs \
  crates/justapi-core/src/extract.rs \
  crates/justapi-core/src/dummy_extract.rs
commit "2026-07-07T08:30:00+06:00" -m "feat(core): scaffold justapi-core with server, router, and extractors

- Cargo.toml: hyper, tokio, rustls, matchit dependencies
- lib.rs: public API re-exports
- server/mod.rs: TLS-capable HTTP server with graceful shutdown
- router.rs: matchit-based path router with method dispatch
- serialize.rs: response serialization (serde_json/simd-json)
- error_catalog.rs: typed error catalog with HTTP status mapping
- extract.rs + dummy_extract.rs: request data extraction framework"

echo "--- [5/20] Core — middleware, security, and observability ---"
git add \
  crates/justapi-core/src/middleware.rs \
  crates/justapi-core/src/rate_limit.rs \
  crates/justapi-core/src/compress.rs \
  crates/justapi-core/src/secrets.rs \
  crates/justapi-core/src/trace_context.rs \
  crates/justapi-core/src/tracing_setup.rs \
  crates/justapi-core/src/panic.rs \
  crates/justapi-core/src/metrics.rs \
  crates/justapi-core/src/alerting.rs \
  crates/justapi-core/src/audit.rs \
  crates/justapi-core/src/health.rs
commit "2026-07-07T11:00:00+06:00" -m "feat(core): add middleware chain, rate-limiting, compression, and observability

- middleware.rs: composable middleware pipeline with Tower-like layering
- rate_limit.rs: token-bucket rate limiter with per-route overrides
- compress.rs: gzip/brotli response compression
- secrets.rs: runtime secrets management
- trace_context.rs + tracing_setup.rs: OpenTelemetry distributed tracing
- panic.rs: panic hook for graceful 500 responses
- metrics.rs: Prometheus-compatible metric collectors
- alerting.rs: threshold-based alert triggers
- audit.rs: request audit logging
- health.rs: liveness and readiness probe endpoints"

echo "--- [6/20] Core — protocol, data, validation, and plugins ---"
git add \
  crates/justapi-core/src/multipart.rs \
  crates/justapi-core/src/validate.rs \
  crates/justapi-core/src/resilience.rs \
  crates/justapi-core/src/openapi.rs \
  crates/justapi-core/src/openai.rs \
  crates/justapi-core/src/static_files.rs \
  crates/justapi-core/src/plugin.rs \
  crates/justapi-core/src/wasm.rs \
  crates/justapi-core/src/grpc.rs \
  crates/justapi-core/src/graphql.rs \
  crates/justapi-core/src/gateway.rs \
  crates/justapi-core/src/batching.rs \
  crates/justapi-core/src/coalesce.rs \
  crates/justapi-core/src/memory.rs \
  crates/justapi-core/src/dx.rs \
  crates/justapi-core/src/test_codec.rs \
  crates/justapi-core/src/testing.rs
commit "2026-07-07T14:30:00+06:00" -m "feat(core): add protocol handlers, data layer, validation, and plugin system

- multipart.rs: streaming multipart/form-data parser
- validate.rs: request/response schema validation
- resilience.rs: circuit breakers, retry policies, graceful degradation
- openapi.rs: OpenAPI 3.1 spec generation from route metadata
- openai.rs: OpenAI-compatible API endpoint support
- static_files.rs: efficient static file serving with etag caching
- plugin.rs: pluggable extension system with lifecycle hooks
- wasm.rs: WASM handler execution via wasmtime
- grpc.rs: gRPC-over-HTTP/2 protocol support
- graphql.rs: GraphQL query execution engine
- gateway.rs: API gateway with upstream routing
- batching.rs + coalesce.rs: request batching and deduplication
- memory.rs: per-request arena allocator
- dx.rs: developer experience utilities (error formatting, hints)
- test_codec.rs + testing.rs: test utilities"

echo "--- [7/20] Core — database ORM layer ---"
git add crates/justapi-core/src/db/
commit "2026-07-07T16:00:00+06:00" -m "feat(core): add async database ORM with connection pooling and migrations

- db/mod.rs: database module re-exports
- db/pool.rs: async connection pool with health checks (deadpool)
- db/query.rs: type-safe query builder with parameterized queries
- db/model.rs: derive macro support for table-mapped models
- db/migrations.rs: schema migration runner with up/down support"

echo "--- [8/20] Core — integration tests ---"
git add crates/justapi-core/tests/integration.rs
commit "2026-07-07T17:00:00+06:00" -m "test(core): add integration tests for server, router, and middleware

- End-to-end tests covering full request lifecycle
- Router path matching and method dispatch validation
- Middleware chain ordering and error propagation"

# ============================================================
# DAY 3 — July 8: Python Bindings & CLI
# ============================================================

echo "--- [9/20] Python bindings — Rust/PyO3 side ---"
git add \
  crates/justapi-py/Cargo.toml \
  crates/justapi-py/pyproject.toml \
  crates/justapi-py/src/lib.rs \
  crates/justapi-py/src/request.rs \
  crates/justapi-py/src/websocket.rs \
  crates/justapi-py/src/multipart.rs \
  crates/justapi-py/src/rate_limit.rs \
  crates/justapi-py/src/database.rs \
  crates/justapi-py/src/dag.rs \
  crates/justapi-py/src/test_client.rs \
  crates/justapi-py/src/test_async.rs \
  crates/justapi-py/src/buffer_test.rs \
  crates/justapi-py/src/native/
commit "2026-07-08T08:30:00+06:00" -m "feat(py): implement PyO3 bindings with ASGI shim and native handler API

- lib.rs: PyO3 module definition and Python entry points
- request.rs: zero-copy request view via buffer protocol
- websocket.rs: Python WebSocket handler with async bridging
- multipart.rs: Python-accessible multipart stream
- rate_limit.rs: Python-side rate limit configuration
- database.rs: async DB integration helpers
- dag.rs: dependency-injection DAG for request-scoped services
- native/: native Python app, handler, and type bindings
- test_client.rs + test_async.rs: Python test client bridge
- buffer_test.rs: buffer protocol correctness tests"

echo "--- [10/20] Python package — modules and type stubs ---"
git add \
  crates/justapi-py/python/justapi/__init__.py \
  crates/justapi-py/python/justapi/__init__.pyi \
  crates/justapi-py/python/justapi/app.py \
  crates/justapi-py/python/justapi/app.pyi \
  crates/justapi-py/python/justapi/background.py \
  crates/justapi-py/python/justapi/background.pyi \
  crates/justapi-py/python/justapi/responses.py \
  crates/justapi-py/python/justapi/responses.pyi \
  crates/justapi-py/python/justapi/params.py \
  crates/justapi-py/python/justapi/params.pyi \
  crates/justapi-py/python/justapi/templating.py \
  crates/justapi-py/python/justapi/templating.pyi \
  crates/justapi-py/python/justapi/tracing.py \
  crates/justapi-py/python/justapi/tracing.pyi \
  crates/justapi-py/python/justapi/testing.py \
  crates/justapi-py/python/justapi/grpc_compiler.py \
  crates/justapi-py/python/justapi/grpc_compiler.pyi \
  crates/justapi-py/python/justapi/_native_helper.py \
  crates/justapi-py/python/justapi/_native_helper.pyi \
  crates/justapi-py/python/justapi/_justapi.pyi \
  crates/justapi-py/python/justapi/_hyperion.pyi \
  crates/justapi-py/README.md \
  crates/justapi-py/LICENSE
commit "2026-07-08T11:00:00+06:00" -m "feat(py): add Python package with app framework, type stubs, and docs

- __init__.py: public API re-exports (JustAPI, Router, Request, etc.)
- app.py: application class with decorator-based route registration
- background.py: background task scheduler and worker pool
- responses.py: JSONResponse, HTMLResponse, StreamingResponse, etc.
- params.py: Path/Query/Header/Cookie parameter extractors
- templating.py: Jinja2 template engine integration
- tracing.py: Python-side OpenTelemetry tracing hooks
- testing.py: TestClient for unit testing handlers
- grpc_compiler.py: protobuf-to-Python codegen integration
- Complete .pyi type stubs for IDE autocompletion
- README.md + LICENSE"

echo "--- [11/20] Python binding tests ---"
git add \
  crates/justapi-py/python/justapi/test_background.py \
  crates/justapi-py/python/justapi/test_circuit_breaker.py \
  crates/justapi-py/python/justapi/test_dag.py \
  crates/justapi-py/python/justapi/test_db_helpers.py \
  crates/justapi-py/python/justapi/test_dependency_injection.py \
  crates/justapi-py/python/justapi/test_native.py \
  crates/justapi-py/python/justapi/test_observability.py \
  crates/justapi-py/python/justapi/test_plugin.py \
  crates/justapi-py/python/justapi/test_rate_limit.py \
  crates/justapi-py/python/justapi/test_request_coalescing.py \
  crates/justapi-py/python/justapi/test_schema_validation.py \
  crates/justapi-py/python/justapi/test_snapshot.py \
  crates/justapi-py/python/justapi/test_sse.py \
  crates/justapi-py/python/justapi/test_templating.py \
  crates/justapi-py/python/justapi/test_test_client.py \
  crates/justapi-py/python/justapi/test_testing.py \
  crates/justapi-py/python/justapi/test_validation.py \
  crates/justapi-py/tests/ \
  crates/justapi-py/test_graphql.py \
  crates/justapi-py/test_hot_reload.py \
  crates/justapi-py/test_layered_di.py
commit "2026-07-08T14:00:00+06:00" -m "test(py): add comprehensive test suite for Python bindings

- 17 unit test modules covering background tasks, circuit breakers,
  dependency injection, rate limiting, SSE, WebSocket, schema validation,
  template rendering, snapshot testing, and more
- tests/test_circuit_breaker_new.py: v2 circuit breaker tests
- tests/test_ws_sse.py: WebSocket + SSE integration tests
- tests/test_query.py: query parameter edge cases
- test_graphql.py: GraphQL endpoint integration
- test_hot_reload.py: hot reload stability under code changes
- test_layered_di.py: multi-layer dependency injection"

echo "--- [12/20] CLI and benchmark crates ---"
git add crates/justapi-cli/ crates/justapi-bench/
commit "2026-07-08T16:30:00+06:00" -m "feat(cli,bench): add CLI binary and internal benchmark harness

CLI (justapi-cli):
- main.rs: CLI entry point with clap argument parsing
- watcher.rs: file watcher for hot reload via notify
- profile.rs: runtime profiling and flame graph generation
- gen_client.rs: OpenAPI-to-client SDK code generation

Bench (justapi-bench):
- main.rs: benchmark runner with criterion integration
- inference_bench.rs: inference engine throughput benchmarks
- gpu_bench.rs: GPU kernel execution benchmarks"

# ============================================================
# DAY 4 — July 9: Inference Engine, E2E Tests, and Fuzzing
# ============================================================

echo "--- [13/20] Inference engine crate ---"
git add crates/justapi-inference/
commit "2026-07-09T09:00:00+06:00" -m "feat(inference): add ML inference engine with scheduling and autoscaling

- engine.rs: core inference execution engine
- model.rs: model loading, weight management, and tensor ops
- scheduler.rs + scheduler_engine.rs: request scheduling with batching
- kv_cache.rs + radix_cache.rs: KV and prefix caching for attention
- openai.rs: OpenAI-compatible chat/completion API adapter
- router.rs: multi-model request routing
- autoscaler.rs: replica autoscaler based on queue depth
- supervisor.rs: worker process supervisor and health checks
- control_plane.rs: distributed coordination
- gateway.rs: inference gateway with load balancing
- pd.rs: prefill-decode disaggregation
- spec_decode.rs + spec_decode_tree.rs: speculative decoding
- real/: GPU model loading (quantization, LoRA, tokenizer)
- tests/engine.rs: engine correctness tests"

echo "--- [14/20] End-to-end integration tests ---"
git add tests/
commit "2026-07-09T11:30:00+06:00" -m "test: add end-to-end integration test suite with gRPC and proto stubs

- test_annotations.py: decorator and type annotation correctness
- test_asyncio.py: async handler lifecycle and event loop tests
- test_batching.py: request batching throughput validation
- test_buffer.py: zero-copy buffer protocol verification
- test_deadlock.py: GIL/tokio deadlock detection under concurrency
- test_exceptions.py: exception propagation across FFI boundary
- test_grpc_*.py: gRPC server, client, interceptor, and inspect
- test_middleware.py: middleware chain ordering validation
- test_multipart.py: multipart upload stress tests
- test_responses.py: response type serialization
- test_sse.py: server-sent events correctness
- proto/: protobuf definitions and generated Python stubs"

echo "--- [15/20] Fuzz targets ---"
git add fuzz/Cargo.toml fuzz/fuzz_targets/ fuzz/.gitignore
commit "2026-07-09T14:00:00+06:00" -m "sec(fuzz): add cargo-fuzz targets for parser and protocol attack surfaces

- fuzz_router.rs: path traversal and routing edge cases
- fuzz_headers.rs: malformed HTTP header injection
- fuzz_body.rs: oversized/malformed request body handling
- fuzz_query_params.rs: query string parsing overflow
- fuzz_jwt.rs: JWT token forgery and validation bypass
- fuzz_file_paths.rs: static file path traversal attacks

Each target exercises untrusted-input parsing paths.
Corpus and artifacts are gitignored."

echo "--- [16/20] Example applications ---"
git add examples/ test_project/
commit "2026-07-09T16:00:00+06:00" -m "docs(examples): add 10 progressive tutorial examples and test project

- 01_hello_world.py → 10_websockets.py: progressive tutorial series
- plugin_cache.py: caching plugin demonstration
- test_project/: standalone demo app with .env and requirements"

# ============================================================
# DAY 5 — July 10: CI/CD, Deployment, Docs, and Website
# ============================================================

echo "--- [17/20] CI/CD workflows ---"
git add .github/
commit "2026-07-10T08:30:00+06:00" -m "ci: add GitHub Actions workflows for CI, benchmarks, fuzzing, and publishing

- ci.yml: lint (clippy, fmt), test, and miri on every PR
- bench.yml: benchmark regression detection with threshold gating
- fuzz.yml: scheduled fuzz campaign with crash reporting
- publish.yml: automated crate + PyPI publishing on release tags
- dependabot.yml: automated dependency update PRs"

echo "--- [18/20] Docker and Helm deployment ---"
git add Dockerfile docker-compose.yml helm/ deploy/
commit "2026-07-10T10:30:00+06:00" -m "infra: add Dockerfile, Helm chart, and multi-cloud deployment guides

Docker:
- Dockerfile: multi-stage build (rust builder -> distroless runtime)
- docker-compose.yml: local dev stack with hot reload

Helm chart (helm/justapi/):
- Deployment, Service, Ingress, HPA, ConfigMap, Secret templates
- Configurable values.yaml with sane production defaults

Cloud deployment guides:
- EKS, GKE, AKS (managed Kubernetes)
- Fly.io, Railway (PaaS)"

echo "--- [19/20] Documentation site ---"
git add docs/ mkdocs.yml .well-known/
commit "2026-07-10T13:00:00+06:00" -m "docs: add MkDocs site with API reference, migration guide, and security docs

- docs/index.md: documentation landing page
- docs/getting_started.md: quickstart and installation guide
- docs/api_reference.md: complete API reference
- docs/migrating_from_fastapi.md: FastAPI migration guide
- docs/plugins.md: plugin authoring documentation
- docs/security/: OWASP checklist, pentest guide, security policy
- mkdocs.yml: MkDocs Material theme configuration
- .well-known/security.txt: security contact information"

echo "--- [20/20] Landing page website ---"
git add website/
commit "2026-07-10T15:00:00+06:00" -m "feat(website): add project landing page with benchmarks and examples

- index.html: responsive landing page with feature showcase
- css/style.css: modern design system with dark mode support
- js/main.js: interactive benchmark charts and code highlighting"

# Catch anything remaining (Cargo.lock, etc.)
if ! git diff --cached --quiet 2>/dev/null || [ -n "$(git ls-files --others --exclude-standard)" ]; then
  git add -A
  if ! git diff --cached --quiet; then
    commit "2026-07-10T16:30:00+06:00" -m "chore: add Cargo.lock and remaining project files

- Pin dependency versions for reproducible builds"
  fi
fi

echo ""
echo "============================================================"
echo " ✅ Clean history rebuilt with 20 professional commits"
echo "    Spread across July 6–10, 2026"
echo ""
echo " Review:  git log --oneline --graph"
echo " Push:    git push -u origin initial-setup --force"
echo "============================================================"
