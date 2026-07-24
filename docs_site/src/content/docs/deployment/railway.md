---
title: Railway
description: Deploy JustAPI to Railway for zero-config cloud deployment — the FastAPI alternative that deploys effortlessly.
keywords: JustAPI, FastAPI alternative, Railway, zero-config, cloud deployment
---

## 1. Create `railway.json`

```json
{
  "build": {
    "builder": "DOCKERFILE"
  },
  "deploy": {
    "restartPolicyType": "ALWAYS",
    "numReplicas": 2
  }
}
```

## 2. Configure Environment

Set these environment variables in Railway dashboard:

| Variable | Description |
|---|---|
| `PORT` | Usually set automatically to 8080 |
| `RUST_LOG` | `info` |
| `DATABASE_URL` | PostgreSQL connection string |
| `REDIS_URL` | Redis connection string |

## 3. Deploy

```bash
railway login
railway init
railway up
```

Or connect via GitHub — Railway auto-deploys on push.

## See Also

- [Fly.io](/deployment/flyio/) — Deploy on Fly.io
- [Docker](/deployment/docker/) — Container setup
- [Production Checklist](/deployment/production-checklist/) — Production readiness
