---
title: Behind a Proxy
description: Run JustAPI behind a reverse proxy (Nginx, Traefik, HAProxy, Cloudflare).
keywords: [JustAPI, reverse proxy, Nginx, Traefik, HAProxy]
---

JustAPI is a plain HTTP server, so it drops cleanly behind any reverse proxy
(Nginx, Traefik, HAProxy, Cloudflare, a cloud load balancer). Offload TLS,
compression, and rate-limit-at-edge to the proxy, and let JustAPI focus on your
app's hot path.

## Nginx Configuration

```nginx
location /api/ {
    proxy_pass http://localhost:8000/;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_http_version 1.1;
    proxy_set_header Connection "";
}
```

For streaming responses (SSE / LLM token streams) keep buffering off:

```nginx
proxy_buffering off;
proxy_read_timeout 3600s;
```

## Host/Origin Validation

JustAPI does not ship a host-header allow-list middleware. If you need to guard
against HTTP host-header injection (e.g. password-reset poisoning), validate the
`Host` header in your own Python middleware:

```python
from justapi import JustAPIApp

app = JustAPIApp()

ALLOWED_HOSTS = {"example.com", "*.example.com"}

def _host_allowed(host: str) -> bool:
    import fnmatch
    return any(fnmatch.fnmatch(host, p) for p in ALLOWED_HOSTS)

@app.middleware("http")
async def trusted_host(request, call_next):
    host = (request.get("headers") or {}).get("host", "")
    if not _host_allowed(host.split(":", 1)[0]):
        return {"status": 403, "body": {"detail": "forbidden"}}
    return await call_next(request)
```

Alternatively, enforce the allow-list at the proxy layer (Nginx `server_name`,
Traefik `Host()` rule), which is where most production setups validate it anyway.

## See Also

- [Docker](/deployment/docker/) — containerized deployment
- [Kubernetes / Helm](/deployment/kubernetes-helm/) — K8s deployment
- [Production Checklist](/deployment/production-checklist/) — production setup