# OWASP Top 10 (2021) Compliance Checklist

## A01:2021 – Broken Access Control
- [x] Route-level access via middleware chain (JwtAuth, custom auth middleware)
- [x] CORS middleware with configurable `allow_origin`
- [x] Per-route authorization middleware — route-level `dependencies=[...]` (FastAPI-style, runs before the handler) + core `JwtAuth.require_for(path_prefix, requirement)`
- [ ] Rate limiting per authenticated user (not just per-IP) — risk-accepted in ADR-096

## A02:2021 – Cryptographic Failures
- [x] TLS via rustls 0.23 (TLS 1.2/1.3 only, no legacy protocols)
- [x] HSTS header with configurable `max-age` and optional `includeSubDomains`/`preload`
- [x] JWT signing and verification with HS256/RS256/ES256
- [x] Secrets management module (`secrets.rs`): env/file/inline with no logging of values
- [x] Password/secret rotation via env var or file re-read on restart
- [ ] Key rotation without restart (hot-reload) — risk-accepted in ADR-096

## A03:2021 – Injection
- [x] SQL injection prevention: parameterized queries via `sqlx` (never string interpolation)
- [x] JSON Schema validation rejects unexpected types
- [x] XSS prevention: `X-XSS-Protection: 0` (modern XSS prevention is via CSP)
- [x] Content-Type sniffing prevention: `X-Content-Type-Options: nosniff`
- [x] HTML sanitization if rendering user content — `Jinja2Templates` enables `autoescape=True` by default (CSP as defence-in-depth)
- [x] Command injection prevention — no process spawning on the request path (`std::process::Command` only in operator CLI/build scripts)

## A04:2021 – Insecure Design
- [x] Rate limiting: global (`RateLimiter`) and per-IP (`IpRateLimiter`)
- [x] Circuit breaker pattern for external service calls
- [x] Timeout middleware (504 Gateway Timeout)
- [x] Graceful degradation via `FallbackMiddleware`
- [x] Bulkhead pattern for concurrent request limiting
- [x] Graceful shutdown (SIGTERM → drain → stop)
- [x] Request size limits — `max_body_size` (default 50 MiB, `with_max_body_size` builder) + path/header/query caps
- [x] Request depth limits — `serde_json` recursion limit on body parsing; `fuzz_body` exercises deeply nested payloads

## A05:2021 – Security Misconfiguration
- [x] CORS with configurable origins, methods, headers
- [x] Security headers: `X-Content-Type-Options`, `X-Frame-Options`, `Strict-Transport-Security`, `Content-Security-Policy`
- [x] Non-root user in Docker image
- [x] HEALTHCHECK in Dockerfile
- [x] Automated security header check in CI — `test_secure_headers.py` (nosniff / DENY / CSP / HSTS opt-in)
- [x] Default-deny CORS (not `*` in production) — `Cors` defaults to an empty origin list; `permissive()` is explicit opt-in; allow/deny covered by `test_cors.py`

## A06:2021 – Vulnerable and Outdated Components
- [x] `cargo-audit` in CI (vulnerability scanning)
- [x] `cargo-deny` in CI (license + advisory checks)
- [x] Dependabot for automated dependency updates
- [x] Supply chain: lockfile committed, `deny.toml` configured
- [x] Trivy scanning in CI for Docker image vulnerabilities — `trivy` CI job scans the **filesystem** (vuln + secret + misconfig, HIGH/CRITICAL, fixed-only) on every PR; pinned binary with SHA256 verification (not `trivy-action`, compromised 2026-03)
- [x] SBOM generation for Docker images — CycloneDX SBOM (`sbom.cdx.json`) produced by Trivy and uploaded as a CI artifact; Docker-image-level SBOM still open

## A07:2021 – Identification and Authentication Failures
- [x] JWT authentication middleware with configurable validation
- [x] OAuth2/OIDC support via custom middleware
- [x] Session management — server-side sessions (`enable_sessions()`, `Session`, Rust session store; cookie/query id, TTL handled in Rust)
- [ ] Multi-factor authentication support — risk-accepted in ADR-096 (app-level; OTP primitives exist in `mail/otp.rs`)
- [x] Password brute-force protection — per-IP rate limiting (partial: also add per-account throttling in app code)

## A08:2021 – Software and Data Integrity Failures
- [x] Supply chain: lockfile, `cargo-deny`, Dependabot
- [ ] CI/CD pipeline signing (sigstore/cosign) — risk-accepted in ADR-096 (PyPI publishes via OIDC trusted publishing)
- [ ] Package provenance (SLSA level 1+) — risk-accepted in ADR-096 (follow-up: `actions/attest-build-provenance`)
- [ ] Plugin signature verification (Phase 19) — risk-accepted in ADR-096

## A09:2021 – Security Logging and Monitoring Failures
- [x] Structured logging (JSON or text format)
- [x] Audit logging middleware (POST/PUT/DELETE/PATCH logging)
- [x] Prometheus metrics including status codes and latency histograms
- [x] Health check endpoints (`/health`, `/live`, `/ready`)
- [x] Alerting: Slack, PagerDuty, Generic webhook
- [x] OpenTelemetry tracing integration
- [x] Centralized log aggregation documentation — `observability/structured-logging.md` (JSON to stdout for ELK/Loki/Datadog)
- [x] Alert on 5xx rate increase — `observability/metrics-monitoring.md` ships a Prometheus `HighErrorRate` rule

## A10:2021 – Server-Side Request Forgery (SSRF)
- [ ] URL validation middleware for outbound requests — risk-accepted in ADR-096 (app-level)
- [x] Allow-list of outbound hosts — gateway upstreams are operator-configured (`GatewayRoute` config, not request input)
- [x] No user-controlled URL fetching by default — the framework performs no outbound fetches from request data (alerting webhooks and gateway routes are operator-set)

## Legend
- [x] Implemented
- [ ] Planned / not yet implemented
- [ ] Not applicable

---

_Last updated: 2026-10-09_
