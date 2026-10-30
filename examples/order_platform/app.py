"""Order Platform — a production-shaped JustAPI application.

Domain: customers order products; stock moves atomically with the order.

What this example exercises (and the test suite proves):

* **API-key auth with roles** via dependencies (`X-API-Key`), 401/403 paths.
* **Interactive transactions** (`with app.db.begin() as tx`) for the order
  flow: check-and-decrement stock, write the order + items + event, commit or
  roll back as a unit — including multi-item orders where the second item is
  out of stock (the first item's decrement must not survive).
* **Idempotency keys** with a unique index: a retried POST returns the original
  order instead of double-decrementing stock, even under a race.
* **Pagination / filtering / sorting** with validated query parameters.
* **Ownership checks** (customers only see their own orders; admins see all).
* **Cancel** restoring stock exactly once, with a 409 on repeat.
* Request IDs, secure headers, timeouts, health/readiness, metrics.

Runs on SQLite (default) and Postgres with the same code: all ids/timestamps
are TEXT and placeholders are `?`, which the pool type-pins per driver.

    python seed.py
    python app.py            # http://127.0.0.1:8000/docs

    pytest test_platform.py -v
"""

from __future__ import annotations

import os
import uuid
from datetime import datetime, timezone

from pydantic import BaseModel, Field

from justapi import (
    BackgroundTasks,
    Depends,
    HTTPException,
    JustAPIApp,
    Query,
)
from justapi.logging import init_json_logging, init_logging

DB_URL = os.environ.get("PLATFORM_DB", "platform.db")
IS_POSTGRES = DB_URL.startswith(("postgres://", "postgresql://"))

if os.environ.get("LOG_FORMAT", "text").strip().lower() == "json":
    init_json_logging()
else:
    init_logging(level=os.environ.get("LOG_LEVEL", "info"))

# Identical DDL on SQLite and Postgres: TEXT ids (UUIDs) and TEXT timestamps
# avoid dialect-specific types (and the Any driver cannot decode timestamptz).
SCHEMA = """
CREATE TABLE IF NOT EXISTS customers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    api_key TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS products (
    sku TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    price_cents INTEGER NOT NULL,
    stock INTEGER NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS orders (
    id TEXT PRIMARY KEY,
    customer_id TEXT NOT NULL,
    status TEXT NOT NULL,
    total_cents INTEGER NOT NULL,
    idempotency_key TEXT,
    created_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_idempotency
    ON orders (customer_id, idempotency_key);
CREATE TABLE IF NOT EXISTS order_items (
    order_id TEXT NOT NULL,
    sku TEXT NOT NULL,
    qty INTEGER NOT NULL,
    unit_price_cents INTEGER NOT NULL,
    PRIMARY KEY (order_id, sku)
);
CREATE TABLE IF NOT EXISTS order_events (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL,
    event TEXT NOT NULL,
    at TEXT NOT NULL
);
"""


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


def new_id() -> str:
    return uuid.uuid4().hex


# ---------------------------------------------------------------------------
# Models
# ---------------------------------------------------------------------------

class CreateProduct(BaseModel):
    sku: str = Field(min_length=1, max_length=40, pattern=r"^[a-z0-9-]+$")
    name: str = Field(min_length=1, max_length=120)
    price_cents: int = Field(gt=0, le=100_000_000)
    stock: int = Field(ge=0, le=10_000_000)


class UpdateProduct(BaseModel):
    name: str | None = Field(default=None, min_length=1, max_length=120)
    price_cents: int | None = Field(default=None, gt=0, le=100_000_000)
    stock: int | None = Field(default=None, ge=0, le=10_000_000)


class OrderLine(BaseModel):
    sku: str = Field(min_length=1, max_length=40)
    qty: int = Field(ge=1, le=1000)


class CreateOrder(BaseModel):
    items: list[OrderLine] = Field(min_length=1, max_length=20)
    idempotency_key: str | None = Field(default=None, min_length=8, max_length=64)


# ---------------------------------------------------------------------------
# App
# ---------------------------------------------------------------------------

app = JustAPIApp(
    title="Order Platform API",
    version="1.0.0",
    description=(
        "Customers, products and atomic orders — a production-shaped JustAPI "
        "example with API-key auth, interactive transactions, idempotency and "
        "inventory invariants."
    ),
)
app.set_database(
    DB_URL if IS_POSTGRES else f"sqlite:{DB_URL}",
    init_sql=SCHEMA,
    pragmas=["journal_mode=WAL", "synchronous=NORMAL"],
)
app.enable_secure_headers()
app.enable_timeout(15)
app.enable_request_id()
app.register_health_check("database", lambda: len(app.db.query("SELECT 1")) == 1)


# ---------------------------------------------------------------------------
# Auth (API key -> customer, with roles)
# ---------------------------------------------------------------------------

def _authenticate(request) -> dict:
    key = request.headers.get("x-api-key")
    if not key:
        raise HTTPException(status_code=401, detail="missing X-API-Key header")
    rows = app.db.query("SELECT * FROM customers WHERE api_key = ?", [key])
    if not rows:
        raise HTTPException(status_code=401, detail="invalid API key")
    return rows[0]


def customer(request) -> dict:
    """Dependency: any authenticated customer."""
    return _authenticate(request)


def admin(request) -> dict:
    """Dependency: authenticated customer with the admin role."""
    user = _authenticate(request)
    if user["role"] != "admin":
        raise HTTPException(status_code=403, detail="admin role required")
    return user


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _product_or_404(sku: str) -> dict:
    rows = app.db.query("SELECT * FROM products WHERE sku = ?", [sku])
    if not rows:
        raise HTTPException(status_code=404, detail=f"product {sku} not found")
    return rows[0]


def _order_payload(order: dict) -> dict:
    items = app.db.query(
        "SELECT sku, qty, unit_price_cents FROM order_items WHERE order_id = ? ORDER BY sku",
        [order["id"]],
    )
    return {
        "id": order["id"],
        "customer_id": order["customer_id"],
        "status": order["status"],
        "total_cents": order["total_cents"],
        "created_at": order["created_at"],
        "items": items,
    }


def _event(tx, order_id: str, event: str) -> None:
    tx.execute(
        "INSERT INTO order_events (id, order_id, event, at) VALUES (?, ?, ?, ?)",
        [new_id(), order_id, event, now_iso()],
    )


# ---------------------------------------------------------------------------
# Products
# ---------------------------------------------------------------------------

@app.get("/products", tags=["products"], summary="List products")
def list_products(
    q: str = Query(None, max_length=80, description="Search name or SKU"),
    in_stock: bool = Query(None, description="Only products with stock > 0"),
    sort: str = Query("name", pattern=r"^(name|price|price_desc|stock)$"),
    limit: int = Query(20, ge=1, le=100),
    offset: int = Query(0, ge=0),
    user=Depends(customer),
):
    where, params = [], []
    if q:
        where.append("(name LIKE ? OR sku LIKE ?)")
        like = f"%{q}%"
        params.extend([like, like])
    if in_stock:
        where.append("stock > 0")
    clause = f" WHERE {' AND '.join(where)}" if where else ""
    order_by = {
        "name": "name ASC",
        "price": "price_cents ASC",
        "price_desc": "price_cents DESC",
        "stock": "stock DESC",
    }[sort]
    rows = app.db.query(
        f"SELECT * FROM products{clause} ORDER BY {order_by} LIMIT ? OFFSET ?",
        [*params, limit, offset],
    )
    total = app.db.query(f"SELECT COUNT(*) AS n FROM products{clause}", params)[0]["n"]
    return {"items": rows, "total": total, "limit": limit, "offset": offset}


@app.get("/products/{sku}", tags=["products"], summary="Get a product")
def get_product(sku: str, user=Depends(customer)):
    return _product_or_404(sku)


@app.post("/products", status_code=201, tags=["products"], summary="Create a product (admin)")
def create_product(body: CreateProduct, user=Depends(admin)):
    existing = app.db.query("SELECT sku FROM products WHERE sku = ?", [body.sku])
    if existing:
        raise HTTPException(status_code=409, detail=f"product {body.sku} already exists")
    app.db.execute(
        "INSERT INTO products (sku, name, price_cents, stock, updated_at) "
        "VALUES (?, ?, ?, ?, ?)",
        [body.sku, body.name, body.price_cents, body.stock, now_iso()],
    )
    return _product_or_404(body.sku)


@app.patch("/products/{sku}", tags=["products"], summary="Update price/stock (admin)")
def update_product(sku: str, body: UpdateProduct, user=Depends(admin)):
    _product_or_404(sku)
    fields = body.model_dump(exclude_unset=True)
    if not fields:
        raise HTTPException(status_code=400, detail="no fields to update")
    assignments = ", ".join(f"{column} = ?" for column in fields)
    app.db.execute(
        f"UPDATE products SET {assignments}, updated_at = ? WHERE sku = ?",
        [*fields.values(), now_iso(), sku],
    )
    return _product_or_404(sku)


# ---------------------------------------------------------------------------
# Orders
# ---------------------------------------------------------------------------

@app.post("/orders", status_code=201, tags=["orders"], summary="Create an order (atomic)")
def create_order(
    body: CreateOrder,
    user=Depends(customer),
    background_tasks: BackgroundTasks = None,
):
    # Sort lines by SKU so concurrent orders take row locks in the same order
    # (prevents deadlocks on multi-item orders).
    lines = sorted(body.items, key=lambda line: line.sku)

    # Fast idempotent replay (the unique index still guards the race).
    if body.idempotency_key:
        existing = app.db.query(
            "SELECT * FROM orders WHERE customer_id = ? AND idempotency_key = ?",
            [user["id"], body.idempotency_key],
        )
        if existing:
            return _order_payload(existing[0])

    order_id = new_id()
    try:
        with app.db.begin() as tx:
            total = 0
            for line in lines:
                rows = tx.query(
                    "UPDATE products SET stock = stock - ?, updated_at = ? "
                    "WHERE sku = ? AND stock >= ? RETURNING price_cents",
                    [line.qty, now_iso(), line.sku, line.qty],
                )
                if not rows:
                    raise HTTPException(
                        status_code=409,
                        detail=f"insufficient stock for {line.sku}",
                    )
                unit_price = rows[0]["price_cents"]
                total += unit_price * line.qty
                tx.execute(
                    "INSERT INTO order_items (order_id, sku, qty, unit_price_cents) "
                    "VALUES (?, ?, ?, ?)",
                    [order_id, line.sku, line.qty, unit_price],
                )
            tx.execute(
                "INSERT INTO orders "
                "(id, customer_id, status, total_cents, idempotency_key, created_at) "
                "VALUES (?, ?, ?, ?, ?, ?)",
                [order_id, user["id"], "pending", total, body.idempotency_key, now_iso()],
            )
            _event(tx, order_id, "created")
    except HTTPException:
        raise  # rollback already happened; surface 404/409
    except RuntimeError as exc:
        # Idempotency race: the unique index rejected the second insert.
        if body.idempotency_key and ("unique" in str(exc).lower() or "duplicate" in str(exc).lower()):
            existing = app.db.query(
                "SELECT * FROM orders WHERE customer_id = ? AND idempotency_key = ?",
                [user["id"], body.idempotency_key],
            )
            if existing:
                return _order_payload(existing[0])
        raise

    order = app.db.query("SELECT * FROM orders WHERE id = ?", [order_id])[0]
    return _order_payload(order)


@app.get("/orders", tags=["orders"], summary="List orders (own orders; admin sees all)")
def list_orders(
    status: str = Query(None, pattern=r"^(pending|cancelled)$"),
    limit: int = Query(20, ge=1, le=100),
    offset: int = Query(0, ge=0),
    user=Depends(customer),
):
    where, params = [], []
    if user["role"] != "admin":
        where.append("customer_id = ?")
        params.append(user["id"])
    if status:
        where.append("status = ?")
        params.append(status)
    clause = f" WHERE {' AND '.join(where)}" if where else ""
    rows = app.db.query(
        f"SELECT * FROM orders{clause} ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
        [*params, limit, offset],
    )
    total = app.db.query(f"SELECT COUNT(*) AS n FROM orders{clause}", params)[0]["n"]
    return {
        "items": [_order_payload(row) for row in rows],
        "total": total,
        "limit": limit,
        "offset": offset,
    }


@app.get("/orders/{order_id}", tags=["orders"], summary="Get an order (owner or admin)")
def get_order(order_id: str, user=Depends(customer)):
    rows = app.db.query("SELECT * FROM orders WHERE id = ?", [order_id])
    if not rows:
        raise HTTPException(status_code=404, detail="order not found")
    order = rows[0]
    if user["role"] != "admin" and order["customer_id"] != user["id"]:
        # Do not leak the existence of other customers' orders.
        raise HTTPException(status_code=404, detail="order not found")
    return _order_payload(order)


@app.post("/orders/{order_id}/cancel", tags=["orders"], summary="Cancel an order (restores stock)")
def cancel_order(order_id: str, user=Depends(customer)):
    with app.db.begin() as tx:
        # Take the write lock with the status transition FIRST: on SQLite a
        # read-then-write transaction can hit an immediate SQLITE_BUSY when two
        # cancels race for the upgrade (busy_timeout does not retry that case).
        # The conditional UPDATE also makes "exactly one cancel wins" atomic.
        changed = tx.execute(
            "UPDATE orders SET status = 'cancelled' WHERE id = ? AND status = 'pending'",
            [order_id],
        )
        rows = tx.query("SELECT * FROM orders WHERE id = ?", [order_id])
        if not rows:
            raise HTTPException(status_code=404, detail="order not found")
        order = rows[0]
        if user["role"] != "admin" and order["customer_id"] != user["id"]:
            # Rollback undoes the status change; do not leak existence.
            raise HTTPException(status_code=404, detail="order not found")
        if changed == 0:
            raise HTTPException(status_code=409, detail="order is not pending")
        for item in tx.query("SELECT sku, qty FROM order_items WHERE order_id = ?", [order_id]):
            tx.execute(
                "UPDATE products SET stock = stock + ?, updated_at = ? WHERE sku = ?",
                [item["qty"], now_iso(), item["sku"]],
            )
        _event(tx, order_id, "cancelled")
    order = app.db.query("SELECT * FROM orders WHERE id = ?", [order_id])[0]
    return _order_payload(order)


@app.get("/orders/{order_id}/events", tags=["orders"], summary="Order audit trail")
def order_events(order_id: str, user=Depends(customer)):
    order = app.db.query("SELECT * FROM orders WHERE id = ?", [order_id])
    if not order or (user["role"] != "admin" and order[0]["customer_id"] != user["id"]):
        raise HTTPException(status_code=404, detail="order not found")
    return {
        "events": app.db.query(
            "SELECT event, at FROM order_events WHERE order_id = ? ORDER BY at, id",
            [order_id],
        )
    }


# ---------------------------------------------------------------------------
# Admin analytics
# ---------------------------------------------------------------------------

@app.get("/admin/stats", tags=["admin"], summary="Revenue, order counts, low stock")
def stats(user=Depends(admin)):
    orders = app.db.query(
        "SELECT COUNT(*) AS orders, COALESCE(SUM(total_cents), 0) AS revenue_cents "
        "FROM orders WHERE status != 'cancelled'"
    )[0]
    by_status = app.db.query(
        "SELECT status, COUNT(*) AS count FROM orders GROUP BY status ORDER BY status"
    )
    low_stock = app.db.query(
        "SELECT sku, name, stock FROM products WHERE stock < 5 ORDER BY stock ASC, sku ASC LIMIT 10"
    )
    return {
        "orders": orders["orders"],
        "revenue_cents": orders["revenue_cents"],
        "by_status": by_status,
        "low_stock": low_stock,
    }


# ---------------------------------------------------------------------------
# Demo data (shared by seed.py and the test suite)
# ---------------------------------------------------------------------------

CUSTOMERS = [
    ("admin", "Platform Admin", "admin-dev-key", "admin"),
    ("cust-ada", "Ada Lovelace", "customer-dev-key", "customer"),
    ("cust-grace", "Grace Hopper", "customer-2-dev-key", "customer"),
]

PRODUCTS = [
    ("keyboard", "Mechanical Keyboard", 12900, 25),
    ("mouse", "Wireless Mouse", 4900, 40),
    ("monitor", "27-inch Monitor", 34900, 8),
    ("usb-c-hub", "USB-C Hub", 7900, 15),
    ("webcam", "1080p Webcam", 5900, 3),
]


def seed_database(db, force: bool = False) -> dict:
    """Insert the demo customers/products idempotently. Returns what was added."""
    if force:
        for table in ("order_items", "order_events", "orders", "products", "customers"):
            db.execute(f"DELETE FROM {table}")
    created = {"customers": 0, "products": 0}
    for customer_id, name, api_key, role in CUSTOMERS:
        if not db.query("SELECT id FROM customers WHERE id = ?", [customer_id]):
            db.execute(
                "INSERT INTO customers (id, name, api_key, role) VALUES (?, ?, ?, ?)",
                [customer_id, name, api_key, role],
            )
            created["customers"] += 1
    for sku, name, price_cents, stock in PRODUCTS:
        if not db.query("SELECT sku FROM products WHERE sku = ?", [sku]):
            db.execute(
                "INSERT INTO products (sku, name, price_cents, stock, updated_at) "
                "VALUES (?, ?, ?, ?, ?)",
                [sku, name, price_cents, stock, now_iso()],
            )
            created["products"] += 1
    return created


if __name__ == "__main__":
    app.run(f"127.0.0.1:{os.environ.get('PORT', '8000')}")
