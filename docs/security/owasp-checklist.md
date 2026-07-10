# OWASP Top 10 (2021) Compliance Checklist

## A01:2021 – Broken Access Control
- [x] Route-level access via middleware chain (JwtAuth, custom auth middleware)
- [x] CORS middleware with configurable `allow_origin`
- [ ] Per-route authorization middleware
- [ ] Rate limiting per authenticated user (not just per-IP)

## A02:2021 – Cryptographic Failures
- [x] TLS via rustls 0.23 (TLS 1.2/1.3 only, no legacy protocols)
- [x] HSTS header with configurable `max-age` and optional `includeSubDomains`/`preload`
- [x] JWT signing and verification with HS256/RS256/ES256
- [x] Secrets management module (`secrets.rs`): env/file/inline with no logging of values
- [x] Password/secret rotation via env var or file re-read on restart
- [ ] Key rotation without restart (hot-reload)

## A03:2021 – Injection
- [x] SQL injection prevention: parameterized queries via `sqlx` (never string interpolation)
- [x] JSON Schema validation rejects unexpected types
- [x] XSS prevention: `X-XSS-Protection: 0` (modern XSS prevention is via CSP)
- [x] Content-Type sniffing prevention: `X-Content-Type-Options: nosniff`
- [ ] HTML sanitization if rendering user content
- [ ] Command injection prevention (no `std::process::Command` by default)

## A04:2021 – Insecure Design
- [x] Rate limiting: global (`RateLimiter`) and per-IP (`IpRateLimiter`)
- [x] Circuit breaker pattern for external service calls
- [x] Timeout middleware (504 Gateway Timeout)
- [x] Graceful degradation via `FallbackMiddleware`
- [x] Bulkhead pattern for concurrent request limiting
- [x] Graceful shutdown (SIGTERM → drain → stop)
- [ ] Request size limits (max body size middleware)
- [ ] Request depth limits (e.g., max JSON nesting depth)

## A05:2021 – Security Misconfiguration
- [x] CORS with configurable origins, methods, headers
- [x] Security headers: `X-Content-Type-Options`, `X-Frame-Options`, `Strict-Transport-Security`, `Content-Security-Policy`
- [x] Non-root user in Docker image
- [x] HEALTHCHECK in Dockerfile
- [ ] Automated security header check in CI
- [ ] Default-deny CORS (not `*` in production)

## A06:2021 – Vulnerable and Outdated Components
- [x] `cargo-audit` in CI (vulnerability scanning)
- [x] `cargo-deny` in CI (license + advisory checks)
- [x] Dependabot for automated dependency updates
- [x] Supply chain: lockfile committed, `deny.toml` configured
- [ ] Trivy scanning in CI for Docker image vulnerabilities
- [ ] SBOM generation for Docker images

## A07:2021 – Identification and Authentication Failures
- [x] JWT authentication middleware with configurable validation
- [x] OAuth2/OIDC support via custom middleware
- [ ] Session management (stateless JWT only; no server-side sessions)
- [ ] Multi-factor authentication support
- [ ] Password brute-force protection (per-IP rate limiter covers this partially)

## A08:2021 – Software and Data Integrity Failures
- [x] Supply chain: lockfile, `cargo-deny`, Dependabot
- [ ] CI/CD pipeline signing (sigstore/cosign)
- [ ] Package provenance (SLSA level 1+)
- [ ] Plugin signature verification (Phase 19)

## A09:2021 – Security Logging and Monitoring Failures
- [x] Structured logging (JSON or text format)
- [x] Audit logging middleware (POST/PUT/DELETE/PATCH logging)
- [x] Prometheus metrics including status codes and latency histograms
- [x] Health check endpoints (`/health`, `/live`, `/ready`)
- [x] Alerting: Slack, PagerDuty, Generic webhook
- [x] OpenTelemetry tracing integration
- [ ] Centralized log aggregation documentation
- [ ] Alert on 5xx rate increase

## A10:2021 – Server-Side Request Forgery (SSRF)
- [ ] URL validation middleware for outbound requests
- [ ] Allow-list of outbound hosts
- [ ] No user-controlled URL fetching by default

## Legend
- [x] Implemented
- [ ] Planned / not yet implemented
- [ ] Not applicable

---

_Last updated: 2026-07-05_
