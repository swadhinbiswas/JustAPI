---
title: Static Files
description: Serve static files (HTML, CSS, JS, images) from a JustAPI application.
keywords: [JustAPI, static files, assets, mount, CSS, JavaScript]
---

## Basic Static Files

Mount a directory to serve static files:

```python
from justapi import JustAPIApp

app = JustAPIApp()

app.mount("/static", directory="static", name="static")
```

This serves files from the `static/` directory at `/static/`. For example, `static/style.css` is accessible at `/static/style.css`.

## Custom Mount Point

```python
# Serve at /assets/*
app.mount("/assets", directory="public/assets", name="assets")
```

## Multiple Static Directories

```python
app.mount("/css", directory="static/css", name="css")
app.mount("/js", directory="static/js", name="js")
app.mount("/images", directory="static/images", name="images")
```

## Mounting a Sub-application

JustAPI supports mounting sub-applications (like Starlette or ASGI apps) at a path:

```python
from justapi import JustAPIApp

app = JustAPIApp()

# Mount a sub-application
from starlette.applications import Starlette
sub_app = Starlette()
app.mount("/api/v2", sub_app, name="v2")
```

## See Also

- [Response Classes](/api-reference/responses/) — response types
- [Custom Response Classes](/advanced/custom-response/) — HTML, stream, file responses
- [Deployment — Docker](/deployment/docker/) — containerized static files
