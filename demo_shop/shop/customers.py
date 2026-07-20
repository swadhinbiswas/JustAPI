"""shop.customers — customer registration and lookup."""
from __future__ import annotations

from justapi import HTTPException

from .config import app, db
from .db import q, q1, paginate
from .schemas import CustomerIn, validate


@app.get("/customers")
def list_customers(request):
    qp = request.query_params
    q = qp.get("q")
    state = qp.get("state")
    sort = qp.get("sort", "id")  # id | city | state
    order = qp.get("order", "asc")

    wheres, params = [], []
    if q:
        wheres.append("(c.id LIKE ? OR c.email LIKE ? OR c.city LIKE ?)")
        params += [f"%{q}%", f"%{q}%", f"%{q}%"]
    if state:
        wheres.append("c.state = ?")
        params.append(state)
    where = (" WHERE " + " AND ".join(wheres)) if wheres else ""

    allowed = {"id": "c.id", "city": "c.city", "state": "c.state"}
    sort_col = allowed.get(sort, "c.id")
    order_dir = "DESC" if order == "desc" else "ASC"

    base = (
        "SELECT c.id, c.unique_id, c.city, c.state, c.email, "
        "(SELECT COUNT(*) FROM orders o WHERE o.customer_id = c.id) AS orders "
        f"FROM customers c{where} ORDER BY {sort_col} {order_dir}"
    )
    count = f"SELECT COUNT(*) AS c FROM customers c{where}"
    return paginate(base, count, params, qp)


@app.get("/customers/{customer_id}")
def customer_detail(request, customer_id: str):
    row = q1("SELECT * FROM customers WHERE id = ?", [customer_id])
    if not row:
        raise HTTPException(status_code=404, detail="customer not found")
    row["orders"] = q(
        "SELECT id, status AS order_status, total_amount, created_at FROM orders "
        "WHERE customer_id = ? ORDER BY created_at DESC",
        [customer_id],
    )
    return row


@app.post("/customers", status_code=201, body_schema=CustomerIn)
def create_customer(request):
    d = validate(CustomerIn, request.json())
    db().execute(
        "INSERT INTO customers (id, unique_id, city, state, email) "
        "VALUES (?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET "
        "city=excluded.city, state=excluded.state, email=excluded.email",
        [d.id, d.unique_id, d.city, d.state, d.email],
    )
    return q1("SELECT * FROM customers WHERE id = ?", [d.id])
