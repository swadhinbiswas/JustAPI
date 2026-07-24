---
title: Routing & Sub-routers
description: Organize your API with modular routers and route groups.
---

JustAPI's routing system uses a radix-trie (via `matchit`) for O(1) route matching. Routes are registered with decorators and can be organized across multiple files using `APIRouter`.

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


@app.put("/users/{user_id}")
def update_user(request, user_id: int):
    return {"user_id": user_id, "updated": True}


@app.patch("/users/{user_id}")
def patch_user(request, user_id: int):
    return {"user_id": user_id, "patched": True}


@app.delete("/users/{user_id}")
def delete_user(request, user_id: int):
    return {"deleted": user_id}
```

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
    return {"product_id": product_id, "name": f"Product {product_id}"}
```

```python
# app/routers/reviews.py
from justapi import APIRouter

router = APIRouter(prefix="/products/{product_id}/reviews", tags=["Reviews"])


@router.get("/")
def list_reviews(request, product_id: int):
    return [{"id": 1, "product_id": product_id, "rating": 5}]
```

```python
# app/main.py
from justapi import JustAPIApp
from app.routers.products import router as products_router
from app.routers.reviews import router as reviews_router

app = JustAPIApp()
app.include_router(products_router)
app.include_router(reviews_router)
```

## Route with Tags

Tags group endpoints in the auto-generated OpenAPI documentation:

```python
@router.get("/", tags=["Products"])
def list_products(request):
    ...
```

## Router-Level Middleware

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

Routes are matched in the order they are registered. More specific routes should come before parameterized routes:

```python
# Specific route first
@app.get("/users/me")
def get_current_user(request):
    return {"user": "current"}

# Parameterized route second
@app.get("/users/{user_id}")
def get_user(request, user_id: int):
    return {"user_id": user_id}
```

## Route-Lookup Cache

For high-traffic routes, JustAPI caches route lookups in a memoization table (ADR-064). This avoids radix-trie traversal on repeated requests to the same path.

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
- [Multi-Protocol APIs](/advanced/multi-protocol-apis/) — REST, GraphQL, gRPC, JSON-RPC
