# Order Platform — a production-shaped API

Customers order products; stock moves **atomically** with the order. This is
the most demanding JustAPI example: it exercises the runtime the way a real
service does.

## What it proves

| Concern | Mechanism | Test |
|---|---|---|
| Auth & roles | API-key dependency, 401/403 | `test_missing_api_key_is_401`, `test_customer_cannot_create_product` |
| **Atomic orders** | `with app.db.begin() as tx` (interactive transaction) | `test_multi_item_order_is_atomic` |
| **No oversell** | conditional `UPDATE ... WHERE stock >= ? RETURNING` | `test_concurrent_orders_never_oversell` (12 buyers / 5 units) |
| **Idempotency** | unique `(customer_id, idempotency_key)` + replay | `test_concurrent_idempotent_retries_create_one_order` |
| Ownership isolation | customer sees only own orders; 404 (not 403) for foreign ids | `test_customers_only_see_their_own_orders` |
| Cancel correctness | stock restored exactly once; 409 on repeat | `test_cancel_restores_stock_exactly_once` |
| Pagination/filter/sort | validated query params | `test_product_list_pagination_search_and_sort` |
| Injection safety | parameterised SQL everywhere | `test_sql_injection_in_search_is_inert` |
| Postgres parity | same code; `?` placeholders type-pinned | `test_postgres_end_to_end` |

## Run it

```bash
python seed.py          # demo customers + products (idempotent)
python app.py           # http://127.0.0.1:8000 — docs at /docs

# Demo API keys (development only — rotate in a real deployment):
#   admin-dev-key        admin
#   customer-dev-key     Ada Lovelace
#   customer-2-dev-key   Grace Hopper
```

```bash
# Browse products
curl -H 'X-API-Key: customer-dev-key' 'http://127.0.0.1:8000/products?sort=price&limit=5'

# Place an order (atomic; 409 when stock is short)
curl -H 'X-API-Key: customer-dev-key' -H 'Content-Type: application/json' \
  -d '{"items":[{"sku":"keyboard","qty":2},{"sku":"mouse","qty":1}],"idempotency_key":"demo-key-0001"}' \
  http://127.0.0.1:8000/orders

# Cancel (restores stock exactly once)
curl -X POST -H 'X-API-Key: customer-dev-key' \
  http://127.0.0.1:8000/orders/<order-id>/cancel

# Admin analytics
curl -H 'X-API-Key: admin-dev-key' http://127.0.0.1:8000/admin/stats
```

## Test it

```bash
pytest test_platform.py -v                      # SQLite (default)
JUSTAPI_TEST_POSTGRES_URL=postgres://postgres:pass@127.0.0.1:5433/postgres \
  pytest test_platform.py -v                    # + Postgres parity
```

The suite includes true concurrency tests: 12 simultaneous buyers against 5
units produce exactly five `201`s and never a negative stock; ten simultaneous
retries of one idempotency key produce one order and one decrement. The same
scenario is verified against `justapi serve --workers 4` on both SQLite and
Postgres.

## Endpoints

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/products` | customer | `q`, `in_stock`, `sort=name\|price\|price_desc\|stock`, `limit`, `offset` |
| GET | `/products/{sku}` | customer | 404 when missing |
| POST | `/products` | admin | 409 duplicate, 422 validation |
| PATCH | `/products/{sku}` | admin | price/stock; 400 empty body |
| POST | `/orders` | customer | atomic; 409 insufficient stock; idempotency key |
| GET | `/orders` | customer | own orders; admin sees all; `status`, pagination |
| GET | `/orders/{id}` | owner/admin | foreign ids → 404 (no existence leak) |
| POST | `/orders/{id}/cancel` | owner/admin | restores stock; 409 if not pending |
| GET | `/orders/{id}/events` | owner/admin | audit trail |
| GET | `/admin/stats` | admin | revenue (excludes cancelled), by-status, low stock |
| GET | `/ready`, `/metrics` | — | readiness (DB probe) and Prometheus metrics |

## Design notes

- **Interactive transactions.** `app.db.begin()` returns a transaction bound to
  one pooled connection; the `with` block commits on success and rolls back on
  any exception. Stock is reserved with a single conditional `UPDATE ... WHERE
  stock >= ? RETURNING price_cents`, so the check and the decrement cannot race.
- **Lock ordering.** Order lines are sorted by SKU before updating so
  concurrent multi-item orders take row locks in the same order (no deadlocks).
- **Idempotency race.** Two concurrent retries can both pass the initial
  lookup; the unique index rejects the second insert, the transaction rolls
  back, and the handler returns the winner's order.
- **Portable schema.** TEXT UUIDs and TEXT timestamps keep the DDL identical on
  SQLite and Postgres (the `Any` driver cannot decode `timestamptz`); `?`
  placeholders are rewritten and type-pinned per driver.

## Docker

```bash
docker build -t order-platform .
docker run --rm -p 8000:8000 -v order-data:/data order-platform
docker exec -u appuser <container> python seed.py
# or: docker compose up --build
```
