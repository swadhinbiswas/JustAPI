# Deploy JustAPI to Railway

## Prerequisites

- Railway account (railway.app)
- `railway` CLI installed

## 1. Set up the project

```bash
# Login
railway login

# Initialize in your JustAPI project directory
railway init
```

## 2. Configure `railway.toml`

```toml
[build]
  builder = "DOCKERFILE"
  dockerfile_path = "Dockerfile"

[deploy]
  restartPolicyType = "ON_FAILURE"
  healthcheckPath = "/health"
  healthcheckTimeout = 5

[[service]]
  name = "justapi"
  port = 8080

[[service]]
  name = "postgres"
  image = "postgres:16-alpine"
  [service.env]
    POSTGRES_USER = "justapi"
    POSTGRES_PASSWORD = "justapi"
    POSTGRES_DB = "justapi"
```

## 3. Add environment variables

```bash
railway vars set RUST_LOG=info
railway vars set DATABASE_URL=$(railway vars get postgres://...)
```

## 4. Deploy

```bash
railway up
# The CLI will build and deploy your application

railway domain
# Get the public URL
```

## 5. Scale (Railway Pro)

```bash
railway scale 3
```

## Verification

```bash
curl https://<your-app>.railway.app/health
# {"status":"healthy","components":[]}
```
