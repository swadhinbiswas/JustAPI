# API Reference

Welcome to the JustAPI API Reference. This document details the core classes and methods available in the framework.

## `JustAPIApp`

The `JustAPIApp` is the core of your application. It acts as the registry for all your routes, middleware, and plugins.

### Initialization

```python
from justapi import JustAPIApp

app = JustAPIApp()
```

### Methods

- `get(path, dependencies=None)`: Register a GET route.
- `post(path, body_schema=None, schema=None, dependencies=None)`: Register a POST route.
- `put(path, body_schema=None, schema=None, dependencies=None)`: Register a PUT route.
- `patch(path, body_schema=None, schema=None, dependencies=None)`: Register a PATCH route.
- `delete(path, dependencies=None)`: Register a DELETE route.
- `include_router(router, prefix="")`: Include routes from an `APIRouter`.
- `run(addr)`: Start the server on the given address (e.g., `"127.0.0.1:8000"`).
- `add_exception_handler(exc_class, handler)`: Register a custom exception handler.

**Example: Basic Routing**
```python
@app.get("/users/{user_id}")
async def get_user(user_id: int):
    return {"user_id": user_id}
```

## `APIRouter`

`APIRouter` is used to group related routes together. It helps in building modular, large-scale applications.

### Methods

It supports the same routing methods as `JustAPIApp`: `get`, `post`, `put`, `patch`, and `delete`.

**Example: Using APIRouter**
```python
from justapi import APIRouter

user_router = APIRouter()

@user_router.get("/")
async def list_users():
    return [{"name": "Alice"}, {"name": "Bob"}]

app.include_router(user_router, prefix="/users")
```

## Dependency Injection (DI) & Parameters

Extract variables directly from the request seamlessly:

- `Path(...)`: Extract variables explicitly from the URL path.
- `Query(...)`: Extract values from query parameters.
- `Header(...)`: Extract values from HTTP headers.
- `Cookie(...)`: Extract from cookies.
- `Body(...)`: Extract from JSON body.
- `Depends(dependency)`: Inject a dependency function.

**Example: Using Depends**
```python
from justapi import Depends

def get_db_session():
    return "db_session"

@app.get("/items/")
async def read_items(db=Depends(get_db_session)):
    return {"db": db}
```

## File Uploads & Forms

JustAPI provides robust parsing for multipart and url-encoded forms:

- `File(...)`: Extract a file from `multipart/form-data`.
- `Form(...)`: Extract a form field.
- `UploadFile`: Represents an uploaded file with methods like `.read()`.

## Responses

JustAPI provides multiple response classes optimized for zero-copy data transfer from Rust to Python:

- `Response(content, status_code, headers)`: Base response class.
- `JSONResponse(content)`: Automatically serializes a Python dictionary to JSON.
- `HTMLResponse(content)`: Sets the `Content-Type` header to `text/html`.
- `PlainTextResponse(content)`: Sets the `Content-Type` header to `text/plain`.
- `RedirectResponse(url, status_code=307)`: Returns an HTTP redirect.
- `StreamingResponse(generator)`: Emits streaming bodies (e.g., for large files or Server-Sent Events).

## Exceptions

- `HTTPException(status_code: int, detail: str)`: Raise this to return an HTTP error (e.g., 404 Not Found).
- `RequestValidationError(errors: list)`: Raised automatically when Pydantic validation fails.
