"""shop.catalog — categories, products (CRUD), and seller read endpoints."""
from __future__ import annotations

from typing import Optional

from justapi import HTTPException

from .config import app, db
from .db import q, q1, paginate
from .schemas import CategoryIn, ProductIn, ProductPatch, validate, slugify


@app.get("/shop/status")
def shop_health(request):
    # NOTE: the framework treats a response dict containing a "status" key as a
    # status wrapper and emits an empty body — avoid that key in JSON bodies.
    n = q1("SELECT COUNT(*) AS c FROM products")
    return {"health": "ok", "db": "sqlite", "products": n["c"]}


@app.get("/categories")
def list_categories(request):
    rows = q(
        "SELECT c.id, c.slug, c.name_pt, c.name_en, "
        "(SELECT COUNT(*) FROM products p WHERE p.category_id = c.id) AS product_count "
        "FROM categories c ORDER BY product_count DESC"
    )
    return {"items": rows}


@app.post("/categories", status_code=201, body_schema=CategoryIn)
def create_category(request):
    d = validate(CategoryIn, request.json())
    slug = d.slug or slugify(d.name_pt)
    try:
        db().execute(
            "INSERT INTO categories (slug, name_pt, name_en) VALUES (?,?,?)",
            [slug, d.name_pt, d.name_en],
        )
    except Exception as e:
        if "UNIQUE" in str(e):
            raise HTTPException(status_code=409, detail=f"category slug '{slug}' already exists")
        raise HTTPException(status_code=400, detail=str(e))
    return q1("SELECT * FROM categories WHERE slug = ?", [slug])


@app.get("/products")
def list_products(request):
    qp = request.query_params
    cat = qp.get("category")          # category slug
    q = qp.get("q")
    min_price = qp.get("min_price")
    max_price = qp.get("max_price")
    sort = qp.get("sort", "id")       # id | price | stock | name
    order = qp.get("order", "asc")

    wheres, params = [], []
    if cat:
        wheres.append("c.slug = ?")
        params.append(cat)
    if q:
        wheres.append("(p.name_pt LIKE ? OR p.id LIKE ?)")
        params += [f"%{q}%", f"%{q}%"]
    if min_price is not None:
        wheres.append("p.price >= ?")
        params.append(float(min_price))
    if max_price is not None:
        wheres.append("p.price <= ?")
        params.append(float(max_price))

    where = (" WHERE " + " AND ".join(wheres)) if wheres else ""
    allowed = {"id": "p.id", "price": "p.price", "stock": "p.stock", "name": "p.name_pt"}
    sort_col = allowed.get(sort, "p.id")
    order_dir = "DESC" if order == "desc" else "ASC"

    base = (
        "SELECT p.id, p.name_pt, p.price, p.stock, p.active, "
        "c.id AS category_id, c.name_en AS category "
        f"FROM products p LEFT JOIN categories c ON c.id = p.category_id{where} "
        f"ORDER BY {sort_col} {order_dir}"
    )
    count = f"SELECT COUNT(*) AS c FROM products p LEFT JOIN categories c ON c.id = p.category_id{where}"
    return paginate(base, count, params, qp)


@app.get("/products/{product_id}")
def product_detail(request, product_id: str):
    row = q1(
        "SELECT p.*, c.name_en AS category FROM products p "
        "LEFT JOIN categories c ON c.id = p.category_id WHERE p.id = ?",
        [product_id],
    )
    if not row:
        raise HTTPException(status_code=404, detail="product not found")
    return row


@app.post("/products", status_code=201, body_schema=ProductIn)
def create_product(request):
    d = validate(ProductIn, request.json())
    try:
        db().execute(
            "INSERT INTO products (id, category_id, name_pt, weight_g, length_cm, "
            "height_cm, width_cm, price, stock) VALUES (?,?,?,?,?,?,?,?,?)",
            [d.id, d.category_id, d.name_pt, d.weight_g,
             d.length_cm, d.height_cm, d.width_cm, d.price, d.stock],
        )
    except Exception as e:
        if "UNIQUE" in str(e):
            raise HTTPException(status_code=409, detail=f"product '{d.id}' exists")
        raise HTTPException(status_code=400, detail=str(e))
    return q1("SELECT * FROM products WHERE id = ?", [d.id])


@app.patch("/products/{product_id}", body_schema=ProductPatch)
def update_product(request, product_id: str):
    cur = q1("SELECT id FROM products WHERE id = ?", [product_id])
    if not cur:
        raise HTTPException(status_code=404, detail="product not found")
    d = validate(ProductPatch, request.json())
    fields, params = [], []
    if d.price is not None:
        fields.append("price = ?"); params.append(d.price)
    if d.stock is not None:
        fields.append("stock = ?"); params.append(d.stock)
    if d.active is not None:
        fields.append("active = ?"); params.append(d.active)
    if not fields:
        return cur
    db().execute(f"UPDATE products SET {', '.join(fields)} WHERE id = ?", params + [product_id])
    return q1("SELECT * FROM products WHERE id = ?", [product_id])


@app.delete("/products/{product_id}", status_code=204)
def delete_product(request, product_id: str):
    res = db().execute("DELETE FROM products WHERE id = ?", [product_id])
    if int(res) == 0:
        raise HTTPException(status_code=404, detail="product not found")
    return None


@app.get("/sellers")
def list_sellers(request):
    qp = request.query_params
    q = qp.get("q")
    state = qp.get("state")
    sort = qp.get("sort", "gmv")  # gmv | units | orders
    order = qp.get("order", "desc")

    wheres, params = [], []
    if q:
        wheres.append("s.id LIKE ?")
        params.append(f"%{q}%")
    if state:
        wheres.append("s.state = ?")
        params.append(state)
    where = (" WHERE " + " AND ".join(wheres)) if wheres else ""

    allowed = {
        "gmv": "COALESCE(SUM(oi.unit_price*oi.quantity),0)",
        "units": "COUNT(oi.product_id)",
        "orders": "COUNT(DISTINCT oi.order_id)",
    }
    sort_col = allowed.get(sort, allowed["gmv"])
    order_dir = "ASC" if order == "asc" else "DESC"

    base = (
        "SELECT s.id, s.city, s.state, "
        "COUNT(DISTINCT oi.order_id) AS orders, "
        "COALESCE(SUM(oi.unit_price*oi.quantity),0) AS gmv, "
        "COUNT(oi.product_id) AS units "
        f"FROM sellers s LEFT JOIN order_items oi ON oi.seller_id = s.id{where} "
        "GROUP BY s.id, s.city, s.state "
        f"ORDER BY {sort_col} {order_dir}"
    )
    count = "SELECT COUNT(*) AS c FROM sellers s" + where
    return paginate(base, count, params, qp)


@app.get("/sellers/{seller_id}")
def seller_detail(request, seller_id: str):
    row = q1("SELECT * FROM sellers WHERE id = ?", [seller_id])
    if not row:
        raise HTTPException(status_code=404, detail="seller not found")
    row["stats"] = q1(
        "SELECT COUNT(DISTINCT oi.order_id) AS orders, "
        "COALESCE(SUM(oi.unit_price*oi.quantity),0) AS gmv, "
        "COUNT(oi.product_id) AS units "
        "FROM order_items oi WHERE oi.seller_id = ?",
        [seller_id],
    )
    return row
