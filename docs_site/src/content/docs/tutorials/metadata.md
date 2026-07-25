---
title: Metadata and Docs URLs
description: Configure OpenAPI metadata, documentation URLs, and API information in JustAPI.
keywords: [JustAPI, metadata, OpenAPI, Swagger, docs URLs, API info]
---

## App Metadata

JustAPI automatically generates OpenAPI 3.1 documentation. Configure the metadata displayed in the docs:

```python
from justapi import JustAPIApp

app = JustAPIApp(
    title="My API",
    version="1.0.0",
    description="A production API built with JustAPI",
    summary="My API summary",
    contact={"name": "Support", "email": "support@example.com"},
    license_info={"name": "MIT", "url": "https://opensource.org/licenses/MIT"},
)
```

## Documentation URLs

```python
app = JustAPIApp(
    docs_url="/docs",        # Swagger UI (default: /docs)
    redoc_url="/redoc",      # ReDoc (default: /redoc)
    openapi_url="/openapi.json",  # OpenAPI schema (default: /openapi.json)
)
```

Set any URL to `None` to disable that endpoint:

```python
app = JustAPIApp(
    docs_url=None,       # disable Swagger UI
    redoc_url=None,      # disable ReDoc
)
```

## Tags

Tags group related endpoints in the docs:

```python
@app.get("/users/", tags=["users"])
def list_users():
    return []

@app.post("/items/", tags=["items"])
def create_item():
    return {}
```

Configure tag descriptions in the app constructor:

```python
app = JustAPIApp(
    openapi_tags=[
        {"name": "users", "description": "User management"},
        {"name": "items", "description": "Item catalog"},
    ],
)
```

## See Also

- [FastAPI CLI](/getting-started/cli-scaffolder/) — project scaffolding
- [Configuration Reference](/reference/configuration/) — all config options
