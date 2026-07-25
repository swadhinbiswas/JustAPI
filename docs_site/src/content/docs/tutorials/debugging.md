---
title: Debugging
description: Debug JustAPI applications with detailed error messages, logging, and common troubleshooting patterns.
keywords: [JustAPI, debugging, errors, logging, troubleshooting]
---

## Error Messages

JustAPI returns structured error responses for validation failures:

```json
{
  "detail": [
    {
      "type": "int_parsing",
      "loc": ["path", "item_id"],
      "msg": "Input should be a valid integer",
      "input": "foo"
    }
  ]
}
```

The `detail` array contains every validation error with:
- `type` — error category
- `loc` — where the error occurred (`["path", "body", "query", "header"]`)
- `msg` — human-readable message
- `input` — the value that was rejected

## Debug Mode

```python
app = JustAPIApp(debug=True)
```

Debug mode enables detailed tracebacks in responses.

:::warning
Never use `debug=True` in production. It exposes internal details.
:::

## Common Errors

### 422 Validation Error

```python
@app.get("/items/{item_id}")
def get_item(item_id: int):
    return {"item_id": item_id}
```

`GET /items/foo` returns 422 because `"foo"` is not a valid `int`.

**Fix:** Ensure the URL value matches the declared type.

### 404 Not Found

`GET /nonexistent` returns 404 with no body.

**Fix:** Check your route path and method.

### 405 Method Not Allowed

`POST /items` when only `@app.get("/items")` is defined returns 405.

**Fix:** Add the missing HTTP method decorator.

## Logging

Use standard Python logging with JustAPI:

```python
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

@app.get("/items")
def list_items():
    logger.info("Listing items")
    return {"items": []}
```

## See Also

- [Testing](/tutorials/testing/) — automated tests
- [Error Codes](/reference/error-codes/) — complete error code reference
- [Security Policy](/security/policy/) — reporting security issues
