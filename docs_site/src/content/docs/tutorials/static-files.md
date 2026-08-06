---
title: Static Files
description: Serve static files (HTML, CSS, JS, images) from a JustAPI application.
keywords: [JustAPI, static files, assets, mount, CSS, JavaScript]
---

## Basic Static Files

Serve files from a directory at a path prefix:

```python
from justapi import JustAPIApp

app = JustAPIApp()

app.frontend("/static", "static")
```

This serves files from the `static/` directory at `/static/`. For example, `static/style.css` is accessible at `/static/style.css`.

## Custom Mount Point

```python
# Serve at /assets/*
app.frontend("/assets", "public/assets")
```

## Multiple Static Directories

```python
app.frontend("/css", "static/css")
app.frontend("/js", "static/js")
app.frontend("/images", "static/images")
```

## Mounting an APIRouter as a Sub-application

JustAPI supports mounting an `APIRouter` at a path prefix:

```python
from justapi import APIRouter, JustAPIApp

app = JustAPIApp()

# Serve a static SPA with index.html fallback for client-side routing
app.frontend("/", "public", html=True)

# Mount a router as a sub-application
router = APIRouter(prefix="/api/v2")
app.mount("/api/v2", router, name="v2")
```

> JustAPI does not mount third-party ASGI/Starlette applications — the runtime
> is a native Rust pipeline. Use `APIRouter` for sub-app structure, or place
> JustAPI behind a path-routing reverse proxy to co-host legacy apps.

## See Also

- [Response Classes](/api-reference/responses/) — response types
- [Custom Response Classes](/advanced/custom-response/) — HTML, stream, file responses
- [Deployment — Docker](/deployment/docker/) — containerized static files
