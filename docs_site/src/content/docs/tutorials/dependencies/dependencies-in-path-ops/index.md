---
title: Dependencies in Path Operation Decorators
description: Declare dependencies directly in the route decorator instead of as function parameters.
keywords: [JustAPI, dependencies, route decorator, dependency injection]
---

## Decorator-Level Dependencies

You can declare dependencies in the decorator itself — they won't appear as function parameters:

```python
from justapi import JustAPIApp, Depends

app = JustAPIApp()

def verify_api_key():
    api_key = request.headers.get("X-API-Key")
    if api_key != "secret-key":
        raise HTTPException(status_code=403)
    return True

@app.get("/items/", dependencies=[Depends(verify_api_key)])
def list_items():
    return {"items": []}
```

The `verify_api_key` runs before `list_items`, but `list_items` doesn't receive the result as a parameter.

## Useful For

- Authentication checks where the handler doesn't need the result
- Logging or rate limiting
- Cross-cutting concerns that apply to many routes

## See Also

- [Global Dependencies](/tutorials/dependencies/global-dependencies/) — app-wide deps
- [Security](/tutorials/security/first-steps/) — authentication patterns
