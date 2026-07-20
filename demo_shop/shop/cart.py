"""shop.cart — shopping cart lifecycle and checkout (cart -> order + payment)."""
from __future__ import annotations

from justapi import HTTPException

from .config import app, db
from .db import q, q1, new_token
from .schemas import CartItemIn, CheckoutIn, validate


@app.post("/carts", status_code=201)
def create_cart(request):
    body = request.json() or {}
    customer_id = body.get("customer_id")
    if customer_id:
        if not q1("SELECT id FROM customers WHERE id = ?", [customer_id]):
            raise HTTPException(status_code=404, detail="customer not found")
    cart_id = new_token("cart")
    db().execute("INSERT INTO carts (id, customer_id) VALUES (?,?)", [cart_id, customer_id])
    return _cart_view(cart_id)


@app.get("/carts/{cart_id}")
def get_cart(request, cart_id: str):
    return _cart_view(cart_id)


@app.post("/carts/{cart_id}/items", body_schema=CartItemIn)
def add_cart_item(request, cart_id: str):
    d = validate(CartItemIn, request.json())
    if not q1("SELECT id FROM carts WHERE id = ?", [cart_id]):
        raise HTTPException(status_code=404, detail="cart not found")
    prod = q1("SELECT id, stock, active FROM products WHERE id = ?", [d.product_id])
    if not prod:
        raise HTTPException(status_code=404, detail="product not found")
    if not prod["active"]:
        raise HTTPException(status_code=409, detail="product not available")
    if d.quantity <= 0:
        raise HTTPException(status_code=422, detail="quantity must be > 0")
    if d.quantity > prod["stock"]:
        raise HTTPException(status_code=409, detail=f"only {prod['stock']} in stock")
    db().execute(
        "INSERT INTO cart_items (cart_id, product_id, quantity) VALUES (?,?,?) "
        "ON CONFLICT(cart_id, product_id) DO UPDATE SET quantity=excluded.quantity",
        [cart_id, d.product_id, d.quantity],
    )
    db().execute("UPDATE carts SET updated_at = datetime('now') WHERE id = ?", [cart_id])
    return _cart_view(cart_id)


@app.delete("/carts/{cart_id}/items/{product_id}", status_code=204)
def remove_cart_item(request, cart_id: str, product_id: str):
    res = db().execute(
        "DELETE FROM cart_items WHERE cart_id = ? AND product_id = ?",
        [cart_id, product_id],
    )
    if int(res) == 0:
        raise HTTPException(status_code=404, detail="item not in cart")
    return None


def _cart_view(cart_id: str) -> dict:
    cart = q1("SELECT * FROM carts WHERE id = ?", [cart_id])
    if not cart:
        raise HTTPException(status_code=404, detail="cart not found")
    items = q(
        "SELECT ci.product_id, p.name_pt, p.price, ci.quantity, "
        "(p.price * ci.quantity) AS line_total "
        "FROM cart_items ci JOIN products p ON p.id = ci.product_id "
        "WHERE ci.cart_id = ? ORDER BY ci.product_id",
        [cart_id],
    )
    total = sum((it["line_total"] or 0) for it in items)
    return {
        "id": cart["id"],
        "customer_id": cart["customer_id"],
        "updated_at": cart["updated_at"],
        "items": items,
        "item_count": len(items),
        "total": round(total, 2),
    }


@app.post("/carts/{cart_id}/checkout", body_schema=CheckoutIn)
def checkout(request, cart_id: str):
    d = validate(CheckoutIn, request.json())
    cart = q1("SELECT * FROM carts WHERE id = ?", [cart_id])
    if not cart:
        raise HTTPException(status_code=404, detail="cart not found")
    items = q(
        "SELECT ci.product_id, p.name_pt, p.price, p.stock, ci.quantity "
        "FROM cart_items ci JOIN products p ON p.id = ci.product_id "
        "WHERE ci.cart_id = ?",
        [cart_id],
    )
    if not items:
        raise HTTPException(status_code=422, detail="cart is empty")
    # Validate stock for every line before touching anything.
    for it in items:
        if it["quantity"] > it["stock"]:
            raise HTTPException(
                status_code=409,
                detail=f"product {it['product_id']} only has {it['stock']} in stock",
            )

    customer_id = cart["customer_id"]
    if not customer_id:
        raise HTTPException(status_code=422, detail="cart has no customer_id; set one first")

    order_id = new_token("order")
    total = round(sum((it["price"] * it["quantity"]) for it in items), 2)

    # One atomic transaction: create order, items, payment, decrement stock.
    stmts = [
        ("INSERT INTO orders (id, customer_id, status, total_amount) VALUES (?,?,?,?)",
         [order_id, customer_id, "paid", total]),
        ("INSERT INTO payments (order_id, method, installments, amount) VALUES (?,?,?,?)",
         [order_id, d.method, d.installments, total]),
    ]
    for it in items:
        stmts.append((
            "INSERT INTO order_items (order_id, product_id, quantity, unit_price, freight) "
            "VALUES (?,?,?,?,?)",
            [order_id, it["product_id"], it["quantity"], it["price"], 0.0],
        ))
        stmts.append((
            "UPDATE products SET stock = stock - ? WHERE id = ?",
            [it["quantity"], it["product_id"]],
        ))
    stmts.append(("UPDATE orders SET paid_at = datetime('now') WHERE id = ?", [order_id]))
    # Empty the cart after a successful purchase.
    stmts.append(("DELETE FROM cart_items WHERE cart_id = ?", [cart_id]))

    try:
        db().transaction(stmts)
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"checkout failed: {e}")

    return q1(
        "SELECT id, customer_id, status AS order_status, total_amount, created_at, paid_at "
        "FROM orders WHERE id = ?",
        [order_id],
    )
