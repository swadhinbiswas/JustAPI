"""shop.orders — order listing, detail, and status lifecycle."""
from __future__ import annotations

from justapi import HTTPException

from .config import app, db
from .db import q, q1, paginate
from .schemas import OrderStatusIn, validate

STATUSES = {"pending", "paid", "shipped", "delivered", "cancelled"}


@app.get("/orders")
def list_orders(request):
    qp = request.query_params
    wheres, params = [], []
    if qp.get("customer_id"):
        wheres.append("customer_id = ?"); params.append(qp["customer_id"])
    if qp.get("status"):
        wheres.append("status = ?"); params.append(qp["status"])
    where = (" WHERE " + " AND ".join(wheres)) if wheres else ""
    base = (
        "SELECT id, customer_id, status AS order_status, total_amount, created_at, paid_at "
        f"FROM orders{where} ORDER BY created_at DESC"
    )
    count = f"SELECT COUNT(*) AS c FROM orders{where}"
    return paginate(base, count, params, qp)


@app.get("/orders/{order_id}")
def order_detail(request, order_id: str):
    order = q1(
        "SELECT id, customer_id, status AS order_status, total_amount, "
        "shipping_amount, created_at, paid_at, shipped_at, delivered_at "
        "FROM orders WHERE id = ?",
        [order_id],
    )
    if not order:
        raise HTTPException(status_code=404, detail="order not found")
    order = dict(order)
    order["items"] = q(
        "SELECT oi.product_id, p.name_pt, oi.quantity, oi.unit_price, oi.freight, "
        "oi.seller_id FROM order_items oi JOIN products p ON p.id = oi.product_id "
        "WHERE oi.order_id = ? ORDER BY oi.product_id",
        [order_id],
    )
    order["payments"] = q(
        "SELECT id, method, installments, amount, created_at FROM payments "
        "WHERE order_id = ? ORDER BY id",
        [order_id],
    )
    return order


@app.patch("/orders/{order_id}/status", body_schema=OrderStatusIn)
def update_order_status(request, order_id: str):
    d = validate(OrderStatusIn, request.json())
    if d.status not in STATUSES:
        raise HTTPException(
            status_code=422,
            detail=f"invalid status; must be one of {sorted(STATUSES)}",
        )
    order = q1("SELECT id, status AS order_status FROM orders WHERE id = ?", [order_id])
    if not order:
        raise HTTPException(status_code=404, detail="order not found")
    if order["order_status"] == "cancelled":
        raise HTTPException(status_code=409, detail="order already cancelled")
    ts_field = {
        "paid": "paid_at",
        "shipped": "shipped_at",
        "delivered": "delivered_at",
    }.get(d.status)
    if ts_field:
        db().execute(
            f"UPDATE orders SET status = ?, {ts_field} = datetime('now') WHERE id = ?",
            [d.status, order_id],
        )
    else:
        db().execute("UPDATE orders SET status = ? WHERE id = ?", [d.status, order_id])
    return q1(
        "SELECT id, status AS order_status, created_at, paid_at, shipped_at, delivered_at "
        "FROM orders WHERE id = ?",
        [order_id],
    )
