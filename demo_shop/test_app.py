"""End-to-end test for the MVP shop using justapi's AsyncTestClient.

The AsyncTestClient drives the real HTTP + routing + body_schema validation
stack (the same path `app.run()` uses), so this verifies the API exactly as a
client would see it. Run:  python test_app.py
"""
from __future__ import annotations

import asyncio
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import shop as a
from db_setup import open_shop
from justapi.testing import AsyncTestClient

PASS, FAIL = 0, 0


def check(name, cond, detail=""):
    global PASS, FAIL
    if cond:
        PASS += 1
        print(f"  PASS  {name}")
    else:
        FAIL += 1
        print(f"  FAIL  {name}  {detail}")


def reset_mutations():
    con = open_shop()
    for t in ("reviews", "payments", "order_items", "orders",
              "cart_items", "carts"):
        con.execute(f"DELETE FROM {t}")
    con.execute("DELETE FROM products WHERE id LIKE 'test_%'")
    con.execute("DELETE FROM categories WHERE slug LIKE 'test-%'")
    con.execute("DELETE FROM customers WHERE id LIKE 'test_%'")
    con.commit()
    con.close()


async def main():
    reset_mutations()  # start from a clean slate (idempotent)
    async with AsyncTestClient(a.app) as c:
        J = lambda v: json.dumps(v).encode()
        def PJ(r):
            return json.loads(r["body"]) if r.get("body") else {}

        print("=== catalog / read ===")
        r = await c.get("/shop/status")
        check("health ok", r.get("status") == 200, r)
        r = await c.get("/categories")
        check("categories listed", r.get("status") == 200 and PJ(r)["items"], r)
        r = await c.get("/products?size=3&sort=price&order=desc")
        body = PJ(r)
        check("products paginated", body["total"] == 32951 and len(body["items"]) == 3, body.get("total"))
        check("products sorted desc", body["items"][0]["price"] >= body["items"][1]["price"], body["items"][:2])
        pid0 = body["items"][0]["id"]
        r = await c.get(f"/products/{pid0}")
        check("product detail", r.get("status") == 200 and PJ(r)["id"] == pid0, r)
        r = await c.get("/products/does_not_exist")
        check("product 404", r.get("status") == 404, r)

        print("=== category create (CRUD) ===")
        r = await c.post("/categories", J({"slug": "test-cat", "name_pt": "Teste", "name_en": "Test"}))
        check("category created", r.get("status") in (200, 201), r)
        r = await c.post("/categories", J({"slug": "test-cat", "name_pt": "X"}))
        check("category dup 409", r.get("status") == 409, r)

        print("=== product create / patch / delete (CRUD) ===")
        r = await c.post("/products", J({"id": "test_prod_1", "category_id": 1, "name_pt": "Widget", "price": 12.5, "stock": 10}))
        check("product created", r.get("status") in (200, 201) and PJ(r)["price"] == 12.5, r)
        r = await c.patch("/products/test_prod_1", J({"price": 9.99, "stock": 5}))
        check("product patched", PJ(r)["price"] == 9.99 and PJ(r)["stock"] == 5, r)
        r = await c.delete("/products/test_prod_1")
        check("product deleted 204", r.get("status") in (200, 204), r)
        r = await c.get("/products/test_prod_1")
        check("product gone 404", r.get("status") == 404, r)
        await c.post("/products", J({"id": "test_prod_1", "category_id": 1, "name_pt": "Widget", "price": 12.5, "stock": 10}))

        print("=== customer create ===")
        r = await c.post("/customers", J({"id": "test_cust_1", "city": "Sao Paulo", "state": "SP", "email": "a@b.com"}))
        check("customer created", r.get("status") in (200, 201) and PJ(r)["id"] == "test_cust_1", r)

        print("=== cart -> checkout -> order ===")
        r = await c.post("/carts", J({"customer_id": "test_cust_1"}))
        cart_id = PJ(r)["id"]
        check("cart created", r.get("status") in (200, 201) and PJ(r)["total"] == 0.0, r)
        r = await c.post(f"/carts/{cart_id}/items", J({"product_id": "test_prod_1", "quantity": 2}))
        check("item added", r.get("status") == 200 and PJ(r)["total"] == 25.0, r)
        r = await c.post(f"/carts/{cart_id}/checkout", J({"method": "credit_card", "installments": 1}))
        check("order created + paid", r.get("status") == 200 and PJ(r)["order_status"] == "paid" and PJ(r)["total_amount"] == 25.0, r)
        oid = PJ(r)["id"]
        stock_now = PJ(await c.get("/products/test_prod_1"))["stock"]
        check("stock decremented 10->8", stock_now == 8, stock_now)
        cart_after = PJ(await c.get(f"/carts/{cart_id}"))
        check("cart emptied", cart_after["item_count"] == 0, cart_after)
        od = PJ(await c.get(f"/orders/{oid}"))
        check("order has item", len(od["items"]) == 1, od.get("items"))
        check("order has payment", len(od["payments"]) == 1 and od["payments"][0]["amount"] == 25.0, od.get("payments"))

        print("=== order status transitions ===")
        r = await c.patch(f"/orders/{oid}/status", J({"status": "shipped"}))
        check("shipped", PJ(r)["order_status"] == "shipped", r)
        r = await c.patch(f"/orders/{oid}/status", J({"status": "delivered"}))
        check("delivered", PJ(r)["order_status"] == "delivered", r)
        r = await c.patch(f"/orders/{oid}/status", J({"status": "nope"}))
        check("invalid status 422", r.get("status") == 422, r)

        print("=== reviews ===")
        r = await c.post(f"/orders/{oid}/reviews", J({"product_id": "test_prod_1", "score": 5, "title": "Great", "comment": "loved it"}))
        check("review created", r.get("status") in (200, 201) and PJ(r)["score"] == 5, r)
        r = await c.post(f"/orders/{oid}/reviews", J({"product_id": "other", "score": 5}))
        check("review rejects product-not-in-order 409", r.get("status") == 409, r)
        r = await c.get("/analytics/review-summary")
        check("review summary", PJ(r)["total_reviews"] == 1 and PJ(r)["avg_score"] == 5.0, r)
        r = await c.get("/analytics/sales-by-category")
        check("sales-by-category revenue", any(x["revenue"] > 0 for x in PJ(r)["items"]), PJ(r)["items"][:1])

        print("=== validation ===")
        r = await c.post("/products", J({"id": "x"}))  # missing required price
        check("create product missing price 422/400", r.get("status") in (400, 422), r)
        empty = PJ(await c.post("/carts", J({"customer_id": "test_cust_1"})))["id"]
        r = await c.post(f"/carts/{empty}/checkout", J({"method": "boleto"}))
        check("checkout empty cart 422", r.get("status") == 422, r)

        reset_mutations()
        print(f"\nSMOKE RESULTS\n  {PASS} passed, {FAIL} failed")
        raise SystemExit(1 if FAIL else 0)


if __name__ == "__main__":
    asyncio.run(main())
