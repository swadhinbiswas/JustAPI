# JustAPI v2.1.0 — FastAPI parity, Python workers, and the first real-weights run

**Release date:** 2026-10-10

The parity-and-hardening release: the flagship FastAPI patterns that were
broken now work (Pydantic body models!), Python apps get production process
management (`--workers`, `--reload`), the container/K8s story is fixed and
verified end to end, and the inference engine ran real model weights for the
first time.

> **One behaviour change:** middleware ordering now matches Starlette/FastAPI
> (ADR-095) — the **last** middleware added is the outermost. Reverse your
> `@app.middleware("http")` registrations if you relied on the old order.

---

## Highlights

### 🐍 Pydantic body models work (the flagship pattern was broken)
`def create_item(item: Item)` returned `422 field required` before this
release — the repository's own `demo_app.py` was broken. Now:
plain model params, `Body(..., embed=True)`, and `Form(...)` with Pydantic
models (multipart **and** urlencoded) all parse into validated instances.
`Query(...)`/`Body(...)` required params return `422` instead of leaking
`Ellipsis` as the value.

### 🏭 Python app process management — `--workers N` / `--reload`
`justapi serve --workers 4` prefork with `SO_REUSEPORT` (dead workers respawn,
SIGTERM drains cleanly); `justapi serve --reload` watches `*.py` and restarts.
**Measured: 3.20× at 4 workers** (34.3k → 109.8k RPS, p99 3.15 → 2.11 ms).

### 🧩 FastAPI parity completed
- `Param` constraints (`gt/ge/lt/le/multiple_of/min/max_length/pattern`) + metadata
  (`title`, `description`, `deprecated`, `include_in_schema`) — enforced and in OpenAPI.
- `@app.exception_handler(ExcClass)` decorator; `exc.errors()` works as method and attribute.
- Auth schemes `HTTPBearer`/`HTTPBasic`/`APIKeyHeader/Query/Cookie`; JWT `aud`/`iss` enforced.
- `JustAPIApp(lifespan=...)`; multi-file uploads (`List[UploadFile]`); OpenAPI security schemes.
- **TestClient runs the full native pipeline** (CORS, secure headers, JWT, timeout, cookie jar) with `options_with` for preflight tests.

### 🔐 Security & ops
- **CORS + JWT fixed**: preflight no longer 401s; 401s carry CORS headers.
- `app.enable_user_rate_limit(...)` (per JWT `sub`), `app.rotate_jwt_secret(...)` (no restart), `app.enable_timeout(seconds)`.
- Trivy + CycloneDX SBOM in CI (pinned binary, SHA256-verified); `cargo deny` green with zero ignored advisories (h2/rustls/wasmtime bumped).
- Docker/K8s entrypoint fixed and verified: `ENTRYPOINT justapi` + `/app/main.py`, Helm `args` work.

### 🤖 First real-weights inference run (ADR-098)
TinyLlama-1.1B Q4 GGUF on CPU: **22.1 tok/s, TTFT 0.14 s**, fluent output —
plus fixes for the SentencePiece tokenizer and logits rank bugs that made real
models unusable. **No GPU run yet** — "beats vLLM" remains untested and unclaimed.

### 🛡️ CI made real
- Free-threaded (3.14t) tests are a **real gate** (previously `|| true`): 223 passed locally.
- Integration-tests job re-enabled (was always-failing); per-PR short fuzz; p99 A/B workflow (warn-only until 2026-10-23); mypy stub gate.

---

## Upgrade

```bash
pip install -U justapi==2.1.0
```

No breaking API changes; the middleware-order behaviour change is the only
semantic difference (see ADR-095).
