---
title: Frontend
description: Serve a static SPA frontend from JustAPI — React, Vue, Next.js, or any static build.
keywords: [JustAPI, frontend, SPA, static files, React, Vue, Next.js]
---

## Serve a Frontend

Mount a static directory as an SPA:

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.frontend("/", directory="dist")
def serve_frontend():
    pass
```

This serves `dist/` at `/` and handles client-side routing (any path not matching an API route returns `index.html`).

## With API Routes

```python
app = JustAPIApp()

@app.get("/api/hello")
def hello():
    return {"message": "Hello from API"}

# Must come AFTER API routes
@app.frontend("/", directory="dist")
def serve_frontend():
    pass
```

:::note
Place `frontend()` after all API routes so API paths take priority.
:::

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | `str` | — | Mount path |
| `directory` | `str` | — | Static file directory |
| `html` | `str` | `None` | Custom index HTML (overrides file) |
| `check_dir` | `bool` | `True` | Raise error if directory doesn't exist |

## Custom HTML

```python
@app.frontend("/", html="<h1>My App</h1>")
def serve():
    pass
```

## See Also

- [Static Files](/tutorials/static-files/) — serving CSS/JS/images
- [Sub Applications — Mounts](/advanced/sub-applications/) — mounting sub-apps
- [Behind a Proxy](/advanced/behind-a-proxy/) — reverse proxy config
