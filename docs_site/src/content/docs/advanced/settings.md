---
title: Settings and Environment Variables
description: Manage configuration with environment variables and Pydantic Settings in JustAPI.
keywords: [JustAPI, settings, environment variables, configuration, Pydantic Settings]
---

## Basic Settings with Pydantic

```python
from pydantic_settings import BaseSettings

class Settings(BaseSettings):
    database_url: str = "sqlite:///app.db"
    secret_key: str = "change-me"
    debug: bool = False
    api_prefix: str = "/api/v1"

    class Config:
        env_file = ".env"

settings = Settings()
```

## Using Settings in Your App

```python
from justapi import JustAPIApp

app = JustAPIApp(title="My App", debug=settings.debug)

@app.get("/config")
def get_config():
    return {
        "debug": settings.debug,
        "api_prefix": settings.api_prefix,
    }
```

## .env File

```env
# .env
DATABASE_URL=postgresql://user:pass@localhost/db
SECRET_KEY=my-production-secret
DEBUG=false
API_PREFIX=/api/v1
```

:::warning
Never commit `.env` files to version control. Add it to `.gitignore`.
:::

## Accessing Environment Variables

```python
import os

# Standard library approach
debug = os.getenv("DEBUG", "false").lower() == "true"
database_url = os.getenv("DATABASE_URL", "sqlite:///app.db")
```

## See Also

- [Lifespan Events](/advanced/lifespan-events/) — startup/shutdown
- [Configuration Reference](/reference/configuration/) — all config options
- [Deployment — Docker](/deployment/docker/) — environment in containers
