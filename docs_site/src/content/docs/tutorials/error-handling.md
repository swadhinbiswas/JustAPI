---
title: Handling Errors
description: Handle HTTP errors, validation errors, and custom exception handlers in JustAPI.
keywords: [JustAPI, error handling, HTTPException, RequestValidationError, exception handlers]
---

## HTTPException

Raise an `HTTPException` to return an error response:

```python
from justapi import JustAPIApp, HTTPException

app = JustAPIApp()

@app.get("/items/{item_id}")
def get_item(item_id: int):
    if item_id == 0:
        raise HTTPException(status_code=404, detail="Item not found")
    return {"item_id": item_id}
```

### Adding Headers

```python
@app.get("/items/{item_id}")
def get_item(item_id: int):
    if item_id == 0:
        raise HTTPException(
            status_code=404,
            detail="Item not found",
            headers={"X-Error": "item-not-found"},
        )
    return {"item_id": item_id}
```

## RequestValidationError

When request data doesn't match the schema, JustAPI returns a `422` error automatically:

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

## Override Validation Error Handler

Customize the validation error response:

```python
from justapi import JustAPIApp
from justapi import Request
from justapi import JSONResponse

app = JustAPIApp()

@app.exception_handler(RequestValidationError)
async def validation_error_handler(request: Request, exc):
    return JSONResponse(
        status_code=422,
        content={
            "error": "validation_error",
            "message": "Invalid request data",
            "details": exc.errors(),
        },
    )
```

## Override HTTPException Handler

Change the default HTTP error format:

```python
from fastapi.exception_handlers import http_exception_handler

@app.exception_handler(HTTPException)
async def custom_http_exception(request, exc):
    return JSONResponse(
        status_code=exc.status_code,
        content={
            "error": "http_error",
            "message": exc.detail,
            "status_code": exc.status_code,
        },
    )
```

## Custom Exception

Create your own exception and handler:

```python
class InsufficientFundsError(Exception):
    def __init__(self, balance: float, amount: float):
        self.balance = balance
        self.amount = amount

@app.exception_handler(InsufficientFundsError)
async def insufficient_funds_handler(request, exc):
    return JSONResponse(
        status_code=402,
        content={
            "error": "insufficient_funds",
            "message": f"Need {exc.amount}, have {exc.balance}",
        },
    )

@app.post("/purchase")
def purchase(amount: float):
    balance = 100.0
    if amount > balance:
        raise InsufficientFundsError(balance, amount)
    return {"status": "ok"}
```

## Debug Mode

```python
app = JustAPIApp(debug=True)
```

Debug mode returns detailed tracebacks in responses.

:::warning
Never use `debug=True` in production. It exposes internal details.
:::

## Common Errors

| Status Code | Meaning | Cause |
|-------------|---------|-------|
| 404 | Not Found | Route doesn't exist |
| 405 | Method Not Allowed | Wrong HTTP method |
| 422 | Validation Error | Request data doesn't match schema |
| 401 | Unauthorized | Missing or invalid auth |
| 403 | Forbidden | Authenticated but not authorized |

## See Also

- [Status Codes](/tutorials/response-status-code/) — custom status codes
- [Error Codes Reference](/reference/error-codes/) — complete error code list
- [Advanced Security](/advanced/advanced-security/) — auth errors
