# Deploy JustAPI to Fly.io

## Prerequisites

- `flyctl` CLI installed and authenticated

## 1. Create a `fly.toml`

```toml
app = "justapi-app"
primary_region = "iad"

[build]
  dockerfile = "Dockerfile"

[http_service]
  internal_port = 8080
  force_https = true
  auto_stop_machines = true
  auto_start_machines = true
  min_machines_running = 2

[[services]]
  port = 80
  handlers = ["http"]
  [services.concurrency]
    type = "requests"
    hard_limit = 1000
    soft_limit = 500

  [[services.ports]]
    port = 443
    handlers = ["tls"]

[env]
  RUST_LOG = "info"
  DATABASE_URL = "postgres://..."
  REDIS_URL = "redis://..."
```

## 2. Launch the app

```bash
flyctl launch --no-deploy
flyctl deploy
```

## 3. Scale

```bash
flyctl scale count 3
flyctl scale vm shared-cpu-1x
```

## 4. Attach a Fly Postgres database

```bash
flyctl postgres create
flyctl postgres attach <pg-app-name>
```

## Verification

```bash
flyctl open
# or
curl https://justapi-app.fly.dev/health
```
