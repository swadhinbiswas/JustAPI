---
title: Routing & Sub-routers
description: Organize your API with modular routers, sub-routers, and programmatic route registration in JustAPI — a FastAPI alternative with Rust-powered routing.
keywords: routing, sub-routers, APIRouter, include_router, modular routes, JustAPI, FastAPI alternative, add_api_route, url_for, named routes
---

JustAPI's routing system uses a radix-trie (via `matchit`) for O(1) route matching. Routes can be registered with decorators, programmatically, or organized across multiple files using `APIRouter`.

## HTTP Method Decorators

```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/users")
def list_users(request):
    return [{"id": 1, "name": "Alice"}]

@app.post("/users")
def create_user(request):
    return {"message": "User created"}
```

All standard HTTP methods are supported: `@app.get()`, `@app.post()`, `@app.put()`, `@app.patch()`, `@app.delete()`, `@app.head()`, `@app.options()`, `@app.trace()`, and `@app.websocket()`.

## Programmatic Route Registration (Non-Decorator)

Register routes without decorators using `add_api_route()`:

```python
def read_item(request, item_id: int):
    return {"item_id": item_id, "name": "Item"}

app.add_api_route("/items/{item_id}", read_item, methods=["GET"])
app.add_api_websocket_route("/ws", my_ws_handler)
```

This is useful when importing handler functions from other modules or when routes need to be registered conditionally.

## Using `APIRouter` for Modular Routes

Group related routes in separate files:

```python
# app/routers/products.py
from justapi import APIRouter

router = APIRouter(prefix="/products", tags=["Products"])

@router.get("/")
def list_products(request):
    return [{"id": 1, "name": "Rust Book"}]

@router.get("/{product_id}")
def get_product(request, product_id: int):
    return {"product_id": product_id}
```

```python
# app/main.py
from justapi import JustAPIApp
from app.routers.products import router as products_router

app = JustAPIApp()
app.include_router(products_router, prefix="/api/v1")
```

## Sub-Routers (Nested Routers)

APIRouter instances can include other APIRouter instances, creating nested route hierarchies:

```python
from justapi import APIRouter

# Create a sub-router for user-related routes
users_router = APIRouter(prefix="/users")

@users_router.get("/{user_id}")
def get_user(request, user_id: int):
    return {"user_id": user_id}

@users_router.get("/{user_id}/orders")
def get_user_orders(request, user_id: int):
    return [{"order_id": 1, "user_id": user_id}]

# Create an admin router and include the users sub-router
admin_router = APIRouter(prefix="/admin", tags=["Admin"])
admin_router.include_router(users_router)

# Include the admin router in the app
app.include_router(admin_router, prefix="/api/v1")
# Final URL: /api/v1/admin/users/{user_id}
```

## Sub-Application Mounting

Use `app.mount()` to mount APIRouters or static directories:

```python
# Mount an APIRouter as a sub-app
app.mount("/api/v1", users_router)

# Mount a static directory
app.mount("/static", "static", name="static")
```

## Named Routes & URL Building

Use the `name` parameter and `url_for()` to build URLs dynamically:

```python
@app.get("/items/{item_id}", name="get_item")
def read_item(request, item_id: int):
    ...

# Build URL for a named route
url = app.url_for("get_item", item_id=42)
# Returns: "/items/42"
```

Named routes work with APIRouter too, and URLs are correctly resolved even with nested prefixes.

## Router-Level Dependencies

```python
from justapi import APIRouter, Depends, HTTPException, Header

admin_router = APIRouter(prefix="/admin", tags=["Admin"])

def require_admin(authorization: str = Header(...)):
    if authorization != "Bearer admin-token":
        raise HTTPException(403, "Admin access required")

@admin_router.get("/users", dependencies=[Depends(require_admin)])
def list_all_users(request):
    return [{"id": 1, "name": "Alice"}]

app.include_router(admin_router)
```

## Route Ordering

Routes are matched in order of registration. Specific routes before parameterized:

```python
@app.get("/users/me")      # Specific route first
def get_current_user(request):
    return {"user": "current"}

@app.get("/users/{user_id}")  # Parameterized route second
def get_user(request, user_id: int):
    return {"user_id": user_id}
```

## OpenAPI Documentation

All registered routes are automatically documented:

| URL | Description |
|---|---|
| `/docs` | Swagger UI |
| `/redoc` | ReDoc |
| `/scalar` | Scalar API Reference |
| `/openapi.json` | Raw OpenAPI 3.1 spec |

## Next Steps

- [API Reference: Routing](/api-reference/routing/) — Complete routing API
- [API Reference: APIRouter](/api-reference/apirouter/) — Router reference
- [API Reference: JustAPIApp](/api-reference/justapiapp/) — App configuration
- [Multi-Protocol APIs](/advanced/multi-protocol-apis/) — REST, GraphQL, gRPC, JSON-RPC
