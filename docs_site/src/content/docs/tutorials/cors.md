---
title: CORS (Cross-Origin Resource Sharing)
description: Configure CORS headers in JustAPI to allow requests from other domains.
keywords: [JustAPI, CORS, cross-origin, security, browser]
---

## Basic CORS

CORS (Cross-Origin Resource Sharing) allows browsers to make requests to your API from different domains. Without CORS, browsers block these requests.

```python
from justapi import JustAPIApp

app = JustAPIApp()

app.add_middleware(
    CORSMiddleware,
    allow_origins=["https://example.com"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)
```

## Permissive CORS (Development)

For local development, allow all origins:

```python
from justapi import JustAPIApp
from starlette.middleware.cors import CORSMiddleware

app = JustAPIApp()

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)
```

:::warning
Never use `allow_origins=["*"]` in production. It defeats the purpose of CORS restrictions.
:::

## Restrictive CORS (Production)

In production, specify exactly which origins are allowed:

```python
app.add_middleware(
    CORSMiddleware,
    allow_origins=[
        "https://app.example.com",
        "https://admin.example.com",
    ],
    allow_credentials=True,
    allow_methods=["GET", "POST", "PUT", "DELETE"],
    allow_headers=["Authorization", "Content-Type"],
)
```

## Options

| Parameter | Type | Description |
|-----------|------|-------------|
| `allow_origins` | `list[str]` | Allowed origins (`["*"]` for all) |
| `allow_methods` | `list[str]` | HTTP methods to allow |
| `allow_headers` | `list[str]` | Request headers to allow |
| `allow_credentials` | `bool` | Allow cookies and auth headers |
| `expose_headers` | `list[str]` | Headers the browser can read |

## See Also

- [Middleware](/tutorials/middleware/) — general middleware usage
- [Security Policy](/security/policy/) — security best practices
