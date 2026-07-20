# demo_shop — MVP e-commerce store (JustAPI Runtime)

A **proper, working mini shop** built on the JustAPI Runtime: a real data model
with full CRUD, a shopping-cart → checkout → order → payment → review flow,
managed inventory, and analytics. Backed by a writable SQLite database
(`shop.db`) seeded from the Olist Brazilian e-commerce dataset.

This is a self-contained MVP you can run, register products, put them in a cart,
check out, and review — not just a read-only demo.

## Data model

```
categories ─< products >─ sellers
customers  ─< carts >─ cart_items ─< products
customers  ─< orders >─ order_items ─< products, payments
orders     ─< reviews >─ products
```

`shop.db` is created and seeded on first run (`python db_setup.py`): 72
categories, ~3.1k sellers, ~99k customers, and ~33k products (with managed
`price` + `stock`), all imported from `olist.sqlite`.

## Endpoints

| Area | Method & path | Notes |
|---|---|---|
| Health | `GET /shop/status` | `{"health":"ok","products":N}` |
| Categories | `GET /categories`, `POST /categories` | create w/ 409 on dup slug |
| Products | `GET /products` (filter/search/sort/paginate), `GET /products/{id}`, `POST /products`, `PATCH /products/{id}`, `DELETE /products/{id}` | full CRUD |
| Sellers | `GET /sellers/{id}` | + lifetime stats |
| Customers | `GET /customers/{id}`, `POST /customers` | register |
| Cart | `POST /carts`, `GET /carts/{id}`, `POST /carts/{id}/items`, `DELETE /carts/{id}/items/{product_id}` | line items, totals |
| Checkout | `POST /carts/{id}/checkout` | **atomic txn**: creates order + payment, decrements stock, empties cart |
| Orders | `GET /orders`, `GET /orders/{id}` (items + payments), `PATCH /orders/{id}/status` | status lifecycle pending→paid→shipped→delivered→cancelled |
| Reviews | `POST /orders/{id}/reviews` | score 1–5, must be a product in the order |
| Analytics | `GET /analytics/sales-by-category`, `GET /analytics/top-products`, `GET /analytics/review-summary` | over the owned, mutable tables |

All request bodies are validated with `justapi.Schema` classes
(`CategoryIn`, `ProductIn`, `ProductPatch`, `CustomerIn`, `CartItemIn`,
`CheckoutIn`, `OrderStatusIn`, `ReviewIn`). Errors return proper HTTP status
codes: `400` bad input, `404` not found, `409` conflict (duplicate / out of
stock / wrong product), `422` validation failure.

## Run it

```bash
# 1) (first time) build + seed the owned shop database
python db_setup.py

# 2) serve  (demo_shop is now a package, run as a module)
/home/swadhin/RastAPI/crates/justapi-py/.venv/bin/python -m shop --port 8080

# 3) exercise the full CRUD + checkout flow (no server needed)
/home/swadhin/RastAPI/crates/justapi-py/.venv/bin/python test_app.py
```

> `uv run` does **not** work from this directory — it walks up into the parent
> RastAPI `uv` workspace (which conflicts with the `SQLPILOT` member). Use the
> maturin venv that has `justapi` installed, as shown above.

### Example flow (curl)

```bash
# register a customer, create a cart
curl -s -X POST localhost:8080/customers -d '{"id":"c1","city":"SP"}' -H 'content-type: application/json'
curl -s -X POST localhost:8080/carts     -d '{"customer_id":"c1"}' -H 'content-type: application/json'
# -> {"id":"cart_xxx", ...}

# add an item, check out
curl -s -X POST localhost:8080/carts/cart_xxx/items -d '{"product_id":"00066f42...","quantity":2}' -H 'content-type: application/json'
curl -s -X POST localhost:8080/carts/cart_xxx/checkout -d '{"method":"credit_card"}' -H 'content-type: application/json'
# -> {"id":"order_xxx","order_status":"paid","total_amount": ...}

# advance + review
curl -s -X PATCH localhost:8080/orders/order_xxx/status -d '{"status":"shipped"}' -H 'content-type: application/json'
curl -s -X POST localhost:8080/orders/order_xxx/reviews -d '{"product_id":"00066f42...","score":5,"title":"Great"}' -H 'content-type: application/json'
```

## Interactive API docs (Scalar)

The app ships interactive OpenAPI docs, generated live from the registered
routes (`/openapi.json`):

| URL | Viewer |
|---|---|
| **`/`** (home page) | **Scalar API Reference** — opens by default |
| `/scalar` | Scalar API Reference |
| `/docs` | Swagger UI |
| `/redoc` | ReDoc |
| `/openapi.json` | Raw OpenAPI 3.1 spec |

Open <http://localhost:8080/> in a browser to see the full, interactive
reference of every endpoint (catalog, cart, checkout, orders, reviews,
analytics).

## Framework gotchas discovered while building this MVP

Two real framework behaviors bit during development (workarounds applied in
`app.py`):

1. **Response bodies containing a top-level `"status"` key are emitted empty.**
   A handler returning `{"status": "ok", ...}` produces a `200` with an empty
   body; `{"products": N}` is fine. Workaround: order responses expose the
   order state under `order_status` (the SQL column is still `status`), and the
   health endpoint uses `health` instead of `status`. *(Framework bug — should
   be filed.)*

2. **`body_schema=` delivers the validated payload to the handler as raw
   `bytes`, not an instantiated Schema object.** So handlers parse
   `request.json()` and validate via the local `validate(schema, data)` helper
   (which uses the Schema's generated JSON Schema). The `Schema` classes still
   drive OpenAPI docs via `body_schema=`. *(Framework bug — should be filed.)*

Neither blocks the MVP; both are noted for a follow-up framework fix.

## Files

The app is a small **package** (`shop/`), not a monolith:

- `db_setup.py` — owned schema (`shop.db`) + one-shot seed from `olist.sqlite`.
- `shop/__init__.py` — package doc, re-exports the `app` instance.
- `shop/config.py` — `create_app()`, DB wiring, package-level `app`/`db` singletons.
- `shop/db.py` — SQL helpers (`q`, `q1`, `paginate`, `new_token`).
- `shop/schemas.py` — `Schema` request classes + the `validate()` helper.
- `shop/catalog.py` — categories, products (full CRUD), sellers.
- `shop/customers.py` — customer register / lookup.
- `shop/cart.py` — cart lifecycle + `checkout` (atomic order + payment + stock dec).
- `shop/orders.py` — orders list / detail / status lifecycle.
- `shop/reviews.py` — reviews + analytics.
- `shop/cli.py` — `python -m shop` entrypoint (`--host`/`--port`).
- `test_app.py` — end-to-end flow test via `justapi.testing.AsyncTestClient`
  (29 assertions: catalog, CRUD, checkout txn, stock decrement, status
  lifecycle, reviews, validation).
