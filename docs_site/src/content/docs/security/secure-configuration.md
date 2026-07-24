---
title: Secure Configuration
description: Hardening guide for running JustAPI securely in production.
---

## Security Headers

Enable secure headers explicitly (not automatic):

```python
app.enable_secure_headers(with_hsts=True)
```

Headers added:
- `Strict-Transport-Security: max-age=31536000` (with `includeSubDomains`)
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `Content-Security-Policy: default-src 'self'`
- `X-XSS-Protection: 0`
- `Referrer-Policy: no-referrer`

## CORS Configuration

CORS is deny-by-default. Configure explicitly:

```python
app.add_cors(
    allow_origins=["https://app.example.com"],  # Never use ["*"] in production
    allow_methods=["GET", "POST", "PUT", "DELETE"],
    allow_headers=["Authorization", "Content-Type"],
    allow_credentials=True,
)
```

## Rate Limiting

```python
# Global rate limit
app.enable_rate_limiter(max_requests=1000, window_seconds=60)

# Per-IP rate limit
app.enable_ip_rate_limiter(max_requests=100, window_seconds=60)
```

## Request Body Size

```python
app.run("0.0.0.0:8000", max_body_size=1024 * 1024)  # 1 MiB
```

## Secrets Management

```python
# Never hardcode secrets
# Use environment variables:
import os
DATABASE_URL = os.environ["DATABASE_URL"]
JWT_SECRET = os.environ["JWT_SECRET"]
```

## TLS Configuration

Recommended: terminate TLS at the edge proxy. If using native TLS:

```python
# Server configured with rustls (TLS 1.2/1.3 only)
app.run_tls(
    "0.0.0.0:8443",
    cert_path="/etc/certs/cert.pem",
    key_path="/etc/certs/key.pem",
)
```

## Security Checklist

- [ ] Secure headers enabled
- [ ] CORS restricted to specific origins
- [ ] Rate limiting configured
- [ ] Body size limits set
- [ ] TLS termination configured (proxy or native)
- [ ] Secrets in environment variables, not code
- [ ] CI/CD: cargo audit, cargo deny passing
- [ ] Logging: no PII in request bodies
- [ ] Non-root user in Docker
- [ ] Regular dependency updates via Dependabot

## See Also

- [Security Policy](/security/policy/) — Vulnerability reporting
- [OWASP Compliance](/security/owasp-compliance/) — Compliance checklist
- [Penetration Testing](/security/penetration-testing/) — Pentest guidelines
