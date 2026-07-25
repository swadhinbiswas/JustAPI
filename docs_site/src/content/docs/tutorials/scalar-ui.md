---
title: Scalar API Reference
description: Use Scalar as an alternative API documentation UI in JustAPI — modern, fast, and developer-friendly.
keywords: [JustAPI, Scalar, API reference, documentation, Swagger alternative]
---

JustAPI includes [Scalar](https://scalar.com/) as a built-in API documentation UI alongside Swagger UI and ReDoc.

## Accessing Scalar

Start your app and open:

```
http://localhost:8000/scalar
```

Scalar provides a clean, modern interface for exploring and testing your API.

## All Three Docs UIs

JustAPI serves three documentation UIs out of the box:

| URL | UI | Description |
|-----|-----|-------------|
| `/docs` | Swagger UI | Classic interactive docs |
| `/redoc` | ReDoc | Clean, readable reference |
| `/scalar` | Scalar | Modern, fast, developer-friendly |

## Configure Scalar URL

```python
from justapi import JustAPIApp

app = JustAPIApp(
    scalar_url="/scalar",     # default
    docs_url="/docs",         # Swagger UI
    redoc_url="/redoc",       # ReDoc
)
```

Set any URL to `None` to disable:

```python
app = JustAPIApp(
    scalar_url=None,   # disable Scalar
    docs_url=None,     # disable Swagger UI
    redoc_url="/redoc",
)
```

## Scalar vs Swagger UI vs ReDoc

| Feature | Scalar | Swagger UI | ReDoc |
|---------|--------|------------|-------|
| Speed | Fast | Medium | Medium |
| Design | Modern, minimal | Classic | Clean, readable |
| Try-it-out | Yes | Yes | No |
| Auth testing | Yes | Yes | No |
| Dark mode | Yes | No | No |
| Schema download | Yes | Yes | Yes |

## Admin Token Protection

When `admin_token` is set, `/scalar` is protected:

```python
app = JustAPIApp(admin_token="my-secret-token")
```

Include the token in requests:

```bash
curl -H "Authorization: Bearer my-secret-token" http://localhost:8000/scalar
```

## See Also

- [Metadata & Docs URLs](/tutorials/metadata/) — configuring doc endpoints
- [Configure Swagger UI](/how-to/configure-swagger-ui/) — Swagger customization
- [API Reference](/api-reference/) — full API reference
