---
title: Configure Swagger UI
description: Customize the Swagger UI appearance and behavior in JustAPI.
keywords: [JustAPI, Swagger UI, documentation, OpenAPI, customization]
---

## Basic Configuration

```python
from justapi import JustAPIApp

app = JustAPIApp(
    docs_url="/docs",
    redoc_url="/redoc",
    swagger_ui_parameters={
        "docExpansion": "none",
        "filter": True,
        "tagsSorter": "alpha",
        "operationsSorter": "alpha",
    },
)
```

## Common Options

| Parameter | Type | Description |
|-----------|------|-------------|
| `docExpansion` | `str` | `"none"`, `"list"`, `"full"` |
| `filter` | `bool` or `str` | Enable search filter |
| `tagsSorter` | `str` | `"alpha"` for alphabetical |
| `operationsSorter` | `str` | `"alpha"` for alphabetical |
| `defaultModelsExpandDepth` | `int` | Model expansion depth |
| `defaultModelExpandDepth` | `int` | Individual model depth |

## Disable Docs in Production

```python
import os

app = JustAPIApp(
    docs_url="/docs" if os.getenv("ENVIRONMENT") == "development" else None,
    redoc_url="/redoc" if os.getenv("ENVIRONMENT") == "development" else None,
)
```

## See Also

- [Additional Responses in OpenAPI](/advanced/additional-responses/) — error docs
- [Metadata & Docs URLs](/tutorials/metadata/) — app metadata
- [OpenAPI Callbacks & Webhooks](/advanced/openapi-callbacks/) — outbound schemas
