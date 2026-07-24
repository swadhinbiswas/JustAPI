---
title: Fly.io
description: Deploy JustAPI to Fly.io for global edge deployment — the FastAPI alternative with edge computing support.
keywords: JustAPI, FastAPI alternative, Fly.io, edge deployment, global deployment
---

## 1. Install flyctl

```bash
curl -L https://fly.io/install.sh | sh
```

## 2. Create `fly.toml`

```toml
app = "my-justapi-app"
primary_region = "iad"

[build]
  image = "my-justapi-app:latest"

[http_service]
  internal_port = 8080
  force_https = true

[[services]]
  protocol = "tcp"
  internal_port = 8080

  [[services.ports]]
    port = 80
    handlers = ["http"]
  [[services.ports]]
    port = 443
    handlers = ["tls", "http"]
```

## 3. Launch

```bash
flyctl launch
flyctl deploy
```

## 4. Scale

```bash
flyctl scale count 3
flyctl scale memory 512
```

## 5. Attach Database

```bash
flyctl postgres create
flyctl postgres attach my-justapi-app
```

## See Also

- [Railway](/deployment/railway/) — Deploy on Railway
- [Docker](/deployment/docker/) — Container setup
- [Production Checklist](/deployment/production-checklist/) — Production readiness
