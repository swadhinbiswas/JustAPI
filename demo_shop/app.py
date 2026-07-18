"""demo_shop — a realistic e-commerce API built on the JustAPI Runtime.

Uses the Olist Brazilian e-commerce dataset (demo_shop/olist.sqlite) as a
read-mostly production-like shop: products, sellers, customers, orders,
payments, reviews, geolocation, plus analytics aggregations.

Goal of this app (per project direction): exercise the framework with a
*complex, real-life* workload rather than micro-benchmarks, and surface
framework problems (serialization, DB pool, GIL, joins, pagination, errors).

Run:
    python demo_shop/app.py            # serves on 127.0.0.1:8080
    python demo_shop/app.py --port 8080
"""
from __future__ import annotations

import argparse
import json
import os
import time
from typing import Any, Optional

from justapi import (
    JustAPIApp,
    Query,
    Path,
    Schema,
    Request,
    HTTPException,
)

HERE = os.path.dirname(os.path.abspath(__file__))
DB_PATH = os.environ.get(
    "DEMO_SHOP_DB", os.path.join(HERE, "olist.sqlite")
)
assert os.path.exists(DB_PATH), f"DB not found: {DB_PATH}"


app = JustAPIApp()

# Rust-native connection pool over SQLite.
# NOTE: app.set_database(..., wal=True) HANGS the pool connect on a real file
# (framework bug — see demo_shop/README findings). Use plain pool; SQLite
# still serves reads concurrently well enough for this demo.
app.set_database(f"sqlite://{DB_PATH}")
# Connect eagerly at import time. This is a WORKAROUND for a framework deadlock:
# app.run() calls connect_database() from inside its own runtime context and
# hangs forever on a configured DB. Connecting here first makes db_pool.is_some()
# true so run() skips that call (see demo_shop/README, Finding #4).
app.connect_database()



# ---------------------------------------------------------------------------
# Small helpers
# ---------------------------------------------------------------------------
def rows_to_dicts(rows: list) -> list:
    """Convert sqlx rows (list of dicts already?) to plain dicts."""
    out = []
    for r in rows:
        if isinstance(r, dict):
            out.append(r)
        elif hasattr(r, "as_dict"):
            out.append(r.as_dict())
        else:
            out.append(dict(r))
    return out


def query_one(sql: str, params: list | None = None) -> dict | None:
    """Fetch a single row (helper over app.db.query, which has no query_one)."""
    rows = app.db.query(sql, params or [])
    rows = rows_to_dicts(rows)
    return rows[0] if rows else None


def _page(qp) -> tuple[int, int]:
    try:
        page = max(1, int(qp.get("page", "1")))
    except (TypeError, ValueError):
        page = 1
    try:
        size = max(1, min(200, int(qp.get("size", "20"))))
    except (TypeError, ValueError):
        size = 20
    return page, size


def paginate_query(base_sql: str, count_sql: str, params: list, qp) -> dict:
    page, size = _page(qp)
    offset = (page - 1) * size
    total = query_one(count_sql, params)
    total_n = int(total["c"]) if total and "c" in total else 0
    rows = app.db.query(base_sql + f" LIMIT {size} OFFSET {offset}", params)
    return {
        "page": page,
        "size": size,
        "total": total_n,
        "pages": (total_n + size - 1) // size if total_n else 0,
        "items": rows_to_dicts(rows),
    }


# ---------------------------------------------------------------------------
# Health / meta
# ---------------------------------------------------------------------------
@app.get("/shop/health")
def shop_health(request):
    n = query_one("SELECT COUNT(*) AS c FROM products")
    return {"status": "ok", "db": "sqlite", "dataset": "olist", "products": n["c"]}


@app.get("/shop/meta/tables")
def meta_tables(request):
    rows = app.db.query("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
    return {"tables": [r["name"] for r in rows]}


# ---------------------------------------------------------------------------
# Catalog: products
# ---------------------------------------------------------------------------
@app.get("/products")
def list_products(request):
    qp = request.query_params
    cat = qp.get("category")
    q = qp.get("q")
    min_price = qp.get("min_price")
    max_price = qp.get("max_price")
    sort = qp.get("sort", "price")  # price | reviews | random
    order = qp.get("order", "asc")

    wheres = []
    params: list = []
    if cat:
        wheres.append("p.product_category_name = ?")
        params.append(cat)
    if q:
        wheres.append("(p.product_category_name LIKE ? OR p.product_id LIKE ?)")
        params.append(f"%{q}%")
        params.append(f"%{q}%")
    if min_price:
        wheres.append("p.price >= ?")
        params.append(float(min_price))
    if max_price:
        wheres.append("p.price <= ?")
        params.append(float(max_price))

    where = (" WHERE " + " AND ".join(wheres)) if wheres else ""
    allowed_sort = {"price": "p.product_id", "reviews": "p.product_id", "random": "p.product_id"}
    sort_col = allowed_sort.get(sort, "p.product_id")
    order_dir = "DESC" if order == "desc" else "ASC"

    base = (
        "SELECT p.product_id, p.product_category_name, p.product_weight_g, "
        "p.product_length_cm, p.product_height_cm, p.product_width_cm "
        f"FROM products p{where} ORDER BY {sort_col} {order_dir}"
    )
    count = f"SELECT COUNT(*) AS c FROM products p{where}"
    return paginate_query(base, count, params, qp)


@app.get("/products/{product_id}")
def product_detail(request, product_id: str):
    row = query_one(
        "SELECT * FROM products WHERE product_id = ?", [product_id]
    )
    if not row:
        raise HTTPException(status_code=404, detail="product not found")
    # Attach sales stats: how many times sold, avg review of its orders.
    stats = query_one(
        "SELECT COUNT(*) AS times_ordered, COALESCE(SUM(oi.price),0) AS revenue "
        "FROM order_items oi WHERE oi.product_id = ?",
        [product_id],
    )
    row = dict(row)
    row["sales_stats"] = stats
    return row


@app.get("/products/{product_id}/reviews")
def product_reviews(request, product_id: str):
    base = (
        "SELECT r.review_id, r.review_score, r.review_comment_title, "
        "r.review_comment_message, r.review_creation_date, oi.order_id "
        "FROM order_reviews r "
        "JOIN order_items oi ON oi.order_id = r.order_id "
        "WHERE oi.product_id = ? ORDER BY r.review_creation_date DESC"
    )
    count = (
        "SELECT COUNT(*) AS c FROM order_reviews r "
        "JOIN order_items oi ON oi.order_id = r.order_id WHERE oi.product_id = ?"
    )
    return paginate_query(base, count, [product_id], request.query_params)


@app.get("/categories")
def list_categories(request):
    rows = app.db.query(
        "SELECT t.product_category_name AS name, t.product_category_name_english AS name_en, "
        "(SELECT COUNT(*) FROM products p WHERE p.product_category_name = t.product_category_name) AS product_count "
        "FROM product_category_name_translation t ORDER BY product_count DESC"
    )
    return {"items": rows_to_dicts(rows)}


# ---------------------------------------------------------------------------
# Sellers
# ---------------------------------------------------------------------------
@app.get("/sellers")
def list_sellers(request):
    base = (
        "SELECT s.seller_id, s.seller_city, s.seller_state, "
        "COUNT(DISTINCT oi.order_id) AS order_count, "
        "COUNT(oi.order_item_id) AS item_count, "
        "COALESCE(SUM(oi.price),0) AS gmv "
        "FROM sellers s "
        "LEFT JOIN order_items oi ON oi.seller_id = s.seller_id "
        "GROUP BY s.seller_id, s.seller_city, s.seller_state"
    )
    count = "SELECT COUNT(*) AS c FROM sellers"
    return paginate_query(base, count, [], request.query_params)


@app.get("/sellers/{seller_id}")
def seller_detail(request, seller_id: str):
    row = query_one("SELECT * FROM sellers WHERE seller_id = ?", [seller_id])
    if not row:
        raise HTTPException(status_code=404, detail="seller not found")
    stats = query_one(
        "SELECT COUNT(DISTINCT oi.order_id) AS orders, "
        "COALESCE(SUM(oi.price),0) AS gmv, "
        "COALESCE(AVG(r.review_score),0) AS avg_review, "
        "COUNT(r.review_id) AS reviews "
        "FROM order_items oi "
        "LEFT JOIN order_reviews r ON r.order_id = oi.order_id "
        "WHERE oi.seller_id = ?",
        [seller_id],
    )
    row = dict(row)
    row["stats"] = stats
    return row


@app.get("/sellers/{seller_id}/products")
def seller_products(request, seller_id: str):
    base = (
        "SELECT DISTINCT p.product_id, p.product_category_name "
        "FROM order_items oi JOIN products p ON p.product_id = oi.product_id "
        "WHERE oi.seller_id = ? ORDER BY p.product_id"
    )
    count = (
        "SELECT COUNT(DISTINCT oi.product_id) AS c FROM order_items oi "
        "WHERE oi.seller_id = ?"
    )
    return paginate_query(base, count, [seller_id], request.query_params)


# ---------------------------------------------------------------------------
# Customers
# ---------------------------------------------------------------------------
@app.get("/customers/{customer_id}")
def customer_detail(request, customer_id: str):
    row = query_one(
        "SELECT * FROM customers WHERE customer_id = ?", [customer_id]
    )
    if not row:
        raise HTTPException(status_code=404, detail="customer not found")
    orders = app.db.query(
        "SELECT order_id, order_status, order_purchase_timestamp, "
        "order_estimated_delivery_date FROM orders WHERE customer_id = ? "
        "ORDER BY order_purchase_timestamp DESC",
        [customer_id],
    )
    row = dict(row)
    row["orders"] = rows_to_dicts(orders)
    return row


# ---------------------------------------------------------------------------
# Orders (the core transaction aggregate)
# ---------------------------------------------------------------------------
@app.get("/orders")
def list_orders(request):
    qp = request.query_params
    status = qp.get("status")
    customer = qp.get("customer_id")
    wheres = []
    params: list = []
    if status:
        wheres.append("o.order_status = ?")
        params.append(status)
    if customer:
        wheres.append("o.customer_id = ?")
        params.append(customer)
    where = (" WHERE " + " AND ".join(wheres)) if wheres else ""
    base = (
        "SELECT o.order_id, o.customer_id, o.order_status, "
        "o.order_purchase_timestamp, o.order_estimated_delivery_date "
        f"FROM orders o{where} ORDER BY o.order_purchase_timestamp DESC"
    )
    count = f"SELECT COUNT(*) AS c FROM orders o{where}"
    return paginate_query(base, count, params, qp)


@app.get("/orders/{order_id}")
def order_detail(request, order_id: str):
    order = query_one("SELECT * FROM orders WHERE order_id = ?", [order_id])
    if not order:
        raise HTTPException(status_code=404, detail="order not found")
    order = dict(order)
    order["items"] = rows_to_dicts(
        app.db.query(
            "SELECT order_item_id, product_id, seller_id, shipping_limit_date, "
            "price, freight_value FROM order_items WHERE order_id = ? ORDER BY order_item_id",
            [order_id],
        )
    )
    order["payments"] = rows_to_dicts(
        app.db.query(
            "SELECT payment_sequential, payment_type, payment_installments, "
            "payment_value FROM order_payments WHERE order_id = ? "
            "ORDER BY payment_sequential",
            [order_id],
        )
    )
    order["reviews"] = rows_to_dicts(
        app.db.query(
            "SELECT review_id, review_score, review_comment_title, "
            "review_comment_message FROM order_reviews WHERE order_id = ?",
            [order_id],
        )
    )
    return order


@app.get("/orders/{order_id}/timeline")
def order_timeline(request, order_id: str):
    order = query_one(
        "SELECT order_purchase_timestamp, order_approved_at, "
        "order_delivered_carrier_date, order_delivered_customer_date, "
        "order_estimated_delivery_date, order_status FROM orders WHERE order_id = ?",
        [order_id],
    )
    if not order:
        raise HTTPException(status_code=404, detail="order not found")
    o = dict(order)
    est = o.get("order_estimated_delivery_date")
    deliv = o.get("order_delivered_customer_date")
    if est and deliv:
        try:
            fmt = "%Y-%m-%d %H:%M:%S"
            delta = (
                time.mktime(time.strptime(deliv, fmt))
                - time.mktime(time.strptime(est, fmt))
            )
            o["delivery_delay_days"] = round(delta / 86400.0, 2)
        except Exception:
            o["delivery_delay_days"] = None
    return o


# ---------------------------------------------------------------------------
# Reviews
# ---------------------------------------------------------------------------
@app.get("/reviews")
def list_reviews(request):
    base = (
        "SELECT r.review_id, r.order_id, r.review_score, r.review_comment_title, "
        "r.review_comment_message, r.review_creation_date "
        "FROM order_reviews r ORDER BY r.review_creation_date DESC"
    )
    count = "SELECT COUNT(*) AS c FROM order_reviews"
    return paginate_query(base, count, [], request.query_params)


@app.get("/reviews/summary")
def reviews_summary(request):
    row = query_one(
        "SELECT COUNT(*) AS total, AVG(review_score) AS avg_score, "
        "SUM(CASE WHEN review_score >= 4 THEN 1 ELSE 0 END) AS positive, "
        "SUM(CASE WHEN review_score <= 2 THEN 1 ELSE 0 END) AS negative "
        "FROM order_reviews"
    )
    return row


# ---------------------------------------------------------------------------
# Geolocation
# ---------------------------------------------------------------------------
@app.get("/geolocation/states")
def geo_states(request):
    rows = app.db.query(
        "SELECT geolocation_state AS state, COUNT(*) AS cnt, "
        "AVG(geolocation_lat) AS avg_lat, AVG(geolocation_lng) AS avg_lng "
        "FROM geolocation GROUP BY geolocation_state ORDER BY cnt DESC"
    )
    return {"items": rows_to_dicts(rows)}


# ---------------------------------------------------------------------------
# Analytics / aggregations (the heavy, join-heavy endpoints)
# ---------------------------------------------------------------------------
@app.get("/analytics/sales-by-category")
def analytics_sales_by_category(request):
    rows = app.db.query(
        "SELECT p.product_category_name AS category, "
        "COUNT(oi.order_item_id) AS units, "
        "COALESCE(SUM(oi.price),0) AS revenue, "
        "COALESCE(SUM(oi.freight_value),0) AS freight "
        "FROM order_items oi JOIN products p ON p.product_id = oi.product_id "
        "GROUP BY p.product_category_name ORDER BY revenue DESC"
    )
    return {"items": rows_to_dicts(rows)}


@app.get("/analytics/top-sellers")
def analytics_top_sellers(request):
    qp = request.query_params
    try:
        limit = max(1, min(100, int(qp.get("limit", "20"))))
    except (TypeError, ValueError):
        limit = 20
    rows = app.db.query(
        "SELECT s.seller_id, s.seller_state, "
        "COUNT(DISTINCT oi.order_id) AS orders, "
        "COUNT(oi.order_item_id) AS items, "
        "COALESCE(SUM(oi.price),0) AS gmv, "
        "COALESCE(AVG(r.review_score),0) AS avg_review "
        "FROM sellers s "
        "JOIN order_items oi ON oi.seller_id = s.seller_id "
        "LEFT JOIN order_reviews r ON r.order_id = oi.order_id "
        "GROUP BY s.seller_id, s.seller_state "
        "ORDER BY gmv DESC LIMIT ?",
        [limit],
    )
    return {"items": rows_to_dicts(rows)}


@app.get("/analytics/delivery-performance")
def analytics_delivery(request):
    rows = app.db.query(
        "SELECT order_status, COUNT(*) AS cnt, "
        "COALESCE(AVG(CAST(julianday(order_delivered_customer_date) - julianday(order_purchase_timestamp) AS REAL)),0) AS avg_days "
        "FROM orders WHERE order_delivered_customer_date IS NOT NULL "
        "GROUP BY order_status"
    )
    return {"items": rows_to_dicts(rows)}


@app.get("/analytics/monthly-revenue")
def analytics_monthly(request):
    rows = app.db.query(
        "SELECT substr(o.order_purchase_timestamp,1,7) AS month, "
        "COUNT(DISTINCT o.order_id) AS orders, "
        "COALESCE(SUM(oi.price),0) AS revenue "
        "FROM orders o JOIN order_items oi ON oi.order_id = o.order_id "
        "GROUP BY month ORDER BY month"
    )
    return {"items": rows_to_dicts(rows)}


# ---------------------------------------------------------------------------
# Search (full-text-ish across products + sellers)
# ---------------------------------------------------------------------------
@app.get("/search")
def search(request):
    qp = request.query_params
    q = qp.get("q", "")
    kind = qp.get("kind", "all")  # all | product | seller
    if not q:
        raise HTTPException(status_code=400, detail="missing ?q=")
    like = f"%{q}%"
    result = {"products": [], "sellers": []}
    if kind in ("all", "product"):
        result["products"] = rows_to_dicts(
            app.db.query(
                "SELECT product_id, product_category_name FROM products "
                "WHERE product_id LIKE ? OR product_category_name LIKE ? LIMIT 25",
                [like, like],
            )
        )
    if kind in ("all", "seller"):
        result["sellers"] = rows_to_dicts(
            app.db.query(
                "SELECT seller_id, seller_city, seller_state FROM sellers "
                "WHERE seller_city LIKE ? OR seller_state LIKE ? LIMIT 25",
                [like, like],
            )
        )
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8080)
    args = parser.parse_args()
    app.run(f"{args.host}:{args.port}")


if __name__ == "__main__":
    main()
