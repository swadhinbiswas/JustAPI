"""shop.reviews — customer reviews and shop analytics."""
from __future__ import annotations

from justapi import HTTPException

from .config import app, db
from .db import q, q1
from .schemas import ReviewIn, validate


@app.post("/orders/{order_id}/reviews", status_code=201, body_schema=ReviewIn)
def create_review(request, order_id: str):
    d = validate(ReviewIn, request.json())
    if d.score < 1 or d.score > 5:
        raise HTTPException(status_code=422, detail="score must be 1..5")
    order = q1("SELECT * FROM orders WHERE id = ?", [order_id])
    if not order:
        raise HTTPException(status_code=404, detail="order not found")
    if not q1(
        "SELECT 1 FROM order_items WHERE order_id = ? AND product_id = ?",
        [order_id, d.product_id],
    ):
        raise HTTPException(status_code=409, detail="product was not part of this order")
    db().execute(
        "INSERT INTO reviews (order_id, product_id, customer_id, score, title, comment) "
        "VALUES (?,?,?,?,?,?)",
        [order_id, d.product_id, order["customer_id"], d.score, d.title, d.comment],
    )
    # NOTE: last_insert_rowid() is per-connection and unreliable across a pooled
    # connection, so re-select the row we just wrote by its natural keys.
    return q1(
        "SELECT * FROM reviews WHERE order_id = ? AND product_id = ? "
        "ORDER BY id DESC LIMIT 1",
        [order_id, d.product_id],
    )


@app.get("/analytics/sales-by-category")
def analytics_sales_by_category(request):
    rows = q(
        "SELECT c.name_en AS category, COUNT(oi.product_id) AS units, "
        "COALESCE(SUM(oi.unit_price * oi.quantity),0) AS revenue "
        "FROM order_items oi "
        "JOIN products p ON p.id = oi.product_id "
        "LEFT JOIN categories c ON c.id = p.category_id "
        "GROUP BY c.id ORDER BY revenue DESC"
    )
    return {"items": rows}


@app.get("/analytics/top-products")
def analytics_top_products(request):
    qp = request.query_params
    try:
        limit = max(1, min(100, int(qp.get("limit", 20))))
    except (TypeError, ValueError):
        limit = 20
    rows = q(
        "SELECT p.id, p.name_pt, COUNT(oi.order_id) AS times_ordered, "
        "COALESCE(SUM(oi.quantity),0) AS units, "
        "COALESCE(SUM(oi.unit_price*oi.quantity),0) AS revenue "
        "FROM order_items oi JOIN products p ON p.id = oi.product_id "
        "GROUP BY p.id ORDER BY revenue DESC LIMIT ?",
        [limit],
    )
    return {"items": rows}


@app.get("/analytics/review-summary")
def analytics_review_summary(request):
    return q1(
        "SELECT COUNT(*) AS total_reviews, "
        "COALESCE(AVG(score),0) AS avg_score, "
        "SUM(CASE WHEN score >= 4 THEN 1 ELSE 0 END) AS positive, "
        "SUM(CASE WHEN score <= 2 THEN 1 ELSE 0 END) AS negative "
        "FROM reviews"
    ) or {"total_reviews": 0, "avg_score": 0, "positive": 0, "negative": 0}
