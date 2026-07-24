---
title: Penetration Testing Guide
description: Guidelines and test scenarios for security testing JustAPI applications.
---

## Attack Surface

| Component | Description | Port |
|---|---|---|
| HTTP/HTTPS server | Main application server | 8080 (configurable) |
| WebSocket endpoint | `/ws` upgrade path | Same as HTTP |
| SSE endpoint | `/events` streaming | Same as HTTP |
| Health endpoints | `/health`, `/live`, `/ready` | Same as HTTP |
| Metrics endpoint | `/metrics` | Same as HTTP |
| OpenAPI docs | `/docs`, `/redoc`, `/scalar` | Same as HTTP |

## Test Scenarios

### 1. Authentication & Authorization
- JWT forgery: malformed, expired, algorithm-confusion, `none`-algorithm tokens
- Missing auth: access protected routes without Authorization header
- Token replay: capture and replay from different IP
- CORS bypass: cross-origin requests not in allow list

### 2. Input Validation
- JSON injection: malformed, deeply nested, duplicate keys, large payloads
- SQL injection: parameterized queries via `sqlx` (should be safe)
- Path traversal: `../` sequences, URL-encoded variants
- Parameter pollution: duplicate query parameters

### 3. Rate Limiting & DoS
- Global rate limit: flood from single IP (expect 429 + Retry-After)
- Per-IP rate limit: exceed per-IP burst (expect 429)
- Bulkhead saturation: concurrent connections (expect 503)
- Slow loris: slow data send (expect 504 timeout)
- Resource exhaustion: large allocations

### 4. TLS & Network
- Downgrade attack: TLS 1.1 or 1.0 (should fail — rustls only supports 1.2/1.3)
- Weak cipher suites: NULL, EXPORT, RC4 ciphers (should fail)
- Certificate validation: self-signed, expired, wrong-domain
- Host header injection

### 5. Information Disclosure
- Stack traces: trigger unhandled errors (should return generic 500)
- Debug endpoints: verify `/metrics` doesn't leak sensitive data
- Server header: verify not present or generic
- Timing attacks: measure response time differences

## Fuzz Testing

```bash
# Install cargo-fuzz (nightly)
rustup toolchain install nightly
cargo install cargo-fuzz

# Run fuzz targets (300 seconds each)
cargo fuzz run fuzz_jwt    -- -max_total_time=300
cargo fuzz run fuzz_headers -- -max_total_time=300
cargo fuzz run fuzz_body    -- -max_total_time=300
cargo fuzz run fuzz_router  -- -max_total_time=300
cargo fuzz run fuzz_query_params -- -max_total_time=300
cargo fuzz run fuzz_file_paths   -- -max_total_time=300
```

## Security Tools

| Tool | Purpose |
|---|---|
| `cargo audit` | Vulnerability scanning |
| `cargo deny` | License/advisory checks |
| `curl` | Manual request testing |
| `oha` | Load testing |
| `nmap` | Port/service discovery |
| `testssl.sh` | TLS configuration test |
| `jwt_tool` | JWT attack toolkit |
| `sqlmap` | SQL injection detection |

## Reporting

Use CVSS 3.1 for severity scoring:

```
Title: <brief description>
Severity: CVSS 3.1 <score> (<Critical/High/Medium/Low>)
Endpoint: <method> <path>
Type: <OWASP category>

## Description
## Steps to Reproduce
## Impact
## Recommended Fix
```

## See Also

- [Security Policy](/security/policy/) — Vulnerability reporting
- [OWASP Compliance](/security/owasp-compliance/) — Compliance checklist
- [Secure Configuration](/security/secure-configuration/) — Hardening guide
