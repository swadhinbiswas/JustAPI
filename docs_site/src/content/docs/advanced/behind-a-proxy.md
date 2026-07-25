---
title: Behind a Proxy
description: Configure JustAPI to run behind a reverse proxy (Nginx, Traefik, HAProxy, Cloudflare).
keywords: [JustAPI, reverse proxy, Nginx, Traefik, root_path, X-Forwarded]
---

## root_path

When running behind a proxy, set `root_path` so JustAPI knows the external path:

```python
app = JustAPIApp(root_path="/api/v1")
```

## Nginx Configuration

```nginx
location /api/ {
    proxy_pass http://localhost:8000/;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
}
```

## Cloudflare

Cloudflare automatically adds the right headers. JustAPI detects them:

```python
app = JustAPIApp(root_path="/")
```

## Trusted Hosts with Proxy

```python
from starlette.middleware.trustedhost import TrustedHostMiddleware

app.add_middleware(
    TrustedHostMiddleware,
    allowed_hosts=["example.com", "*.example.com"],
)
```

## See Also

- [Docker](/deployment/docker/) — containerized deployment
- [Kubernetes / Helm](/deployment/kubernetes-helm/) — K8s deployment
- [Production Checklist](/deployment/production-checklist/) — production setup
