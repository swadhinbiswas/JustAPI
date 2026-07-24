---
title: OWASP Top 10 Compliance
description: JustAPI's compliance status against OWASP Top 10 (2021) security categories.
---

## A01: Broken Access Control
- ✅ Route-level access via middleware chain (JwtAuth, custom auth)
- ✅ CORS middleware with configurable `allow_origin`
- [ ] Per-route authorization middleware
- [ ] Rate limiting per authenticated user

## A02: Cryptographic Failures
- ✅ TLS via rustls (TLS 1.2/1.3 only, no legacy protocols)
- ✅ HSTS header with configurable `max-age`
- ✅ JWT signing with HS256/RS256/ES256
- ✅ Secrets management (env/file/inline, no logging)
- [ ] Key rotation without restart (hot-reload)

## A03: Injection
- ✅ SQL injection prevention: parameterized queries via `sqlx`
- ✅ JSON Schema validation rejects unexpected types
- ✅ XSS prevention via CSP headers
- ✅ Content-Type sniffing prevention: `X-Content-Type-Options: nosniff`
- [ ] HTML sanitization for user-rendered content

## A04: Insecure Design
- ✅ Rate limiting: global and per-IP
- ✅ Circuit breaker pattern
- ✅ Timeout middleware (504 Gateway Timeout)
- ✅ Graceful degradation via FallbackMiddleware
- ✅ Bulkhead pattern for concurrent request limiting
- ✅ Graceful shutdown (SIGTERM → drain → stop)
- [ ] Request body depth limits

## A05: Security Misconfiguration
- ✅ CORS with configurable origins, methods, headers
- ✅ Security headers (X-Content-Type-Options, X-Frame-Options, HSTS, CSP)
- ✅ Non-root user in Docker image
- ✅ HEALTHCHECK in Dockerfile
- [ ] Automated security header check in CI

## A06: Vulnerable and Outdated Components
- ✅ `cargo-audit` in CI (vulnerability scanning)
- ✅ `cargo-deny` in CI (license + advisory checks)
- ✅ Dependabot for automated dependency updates
- ✅ Supply chain: lockfile committed, `deny.toml` configured
- [ ] Trivy scanning for Docker image vulnerabilities

## A07: Identification and Authentication Failures
- ✅ JWT authentication middleware
- ✅ OAuth2/OIDC support via custom middleware
- [ ] Multi-factor authentication support

## A08: Software and Data Integrity Failures
- ✅ Supply chain: lockfile, cargo-deny, Dependabot
- [ ] CI/CD pipeline signing (sigstore/cosign)
- [ ] Plugin signature verification

## A09: Security Logging and Monitoring Failures
- ✅ Structured logging (JSON or text format)
- ✅ Audit logging middleware
- ✅ Prometheus metrics with latency histograms
- ✅ Health check endpoints (/health, /live, /ready)
- ✅ Alerting: Slack, PagerDuty, Generic webhook
- ✅ OpenTelemetry tracing integration

## A10: Server-Side Request Forgery (SSRF)
- [ ] URL validation middleware for outbound requests
- [ ] Allow-list of outbound hosts

## Legend
- ✅ Implemented
- [ ] Planned / not yet implemented

## See Also

- [Security Policy](/security/policy/) — Vulnerability reporting
- [Penetration Testing](/security/penetration-testing/) — Pentest guidelines
- [Secure Configuration](/security/secure-configuration/) — Hardening guide
