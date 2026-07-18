"""Smoke test for the demo_shop e-commerce API.

Validates that every route registers and returns sane data against the real
Olist SQLite dataset, using the framework's AsyncTestClient (which sets up the
DB runtime). Run:

    .venv/bin/python -m pytest crates/justapi-py/python/justapi/../../../../../demo_shop/test_app.py -q
or simply:
    .venv/bin/python demo_shop/test_app.py
"""
import asyncio
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))  # repo root

from justapi.testing import AsyncTestClient  # noqa: E402

import demo_shop.app as shop  # noqa: E402

DB_URL = f"sqlite://{shop.DB_PATH}"


async def main():
    async with AsyncTestClient(shop.app, database=DB_URL) as c:
        checks = []

        def chk(name, resp, expect_status=200):
            ok = resp.get("status") == expect_status
            checks.append((name, ok, resp.get("status")))
            return resp

        chk("shop/health", await c.get("/shop/health"))
        chk("shop/meta/tables", await c.get("/shop/meta/tables"))
        chk("products", await c.get("/products?size=3"))
        chk("products filter", await c.get("/products?category=bed_bath_table&size=2"))
        chk("products search", await c.get("/products?q=bed&size=2"))
        chk("categories", await c.get("/categories"))
        chk("sellers", await c.get("/sellers?size=3"))
        chk("reviews", await c.get("/reviews?size=3"))
        chk("reviews/summary", await c.get("/reviews/summary"))
        chk("geo/states", await c.get("/geolocation/states"))
        chk("analytics/sales-by-category", await c.get("/analytics/sales-by-category"))
        chk("analytics/top-sellers", await c.get("/analytics/top-sellers?limit=5"))
        chk("analytics/delivery", await c.get("/analytics/delivery-performance"))
        chk("analytics/monthly", await c.get("/analytics/monthly-revenue"))
        chk("search", await c.get("/search?q=sao"))

        # Find a real product_id + order_id + seller_id to exercise detail routes.
        prods = (await c.get("/products?size=1")).get("body")
        import json as _json
        prods = _json.loads(prods) if isinstance(prods, (bytes, str)) else prods
        pid = prods["items"][0]["product_id"]
        chk("product detail", await c.get(f"/products/{pid}"))
        chk("product reviews", await c.get(f"/products/{pid}/reviews?size=2"))

        sell = _json.loads((await c.get("/sellers?size=1")).get("body"))
        sid = sell["items"][0]["seller_id"]
        chk("seller detail", await c.get(f"/sellers/{sid}"))
        chk("seller products", await c.get(f"/sellers/{sid}/products?size=2"))

        # order id from analytics top-sellers join path: pull one from orders list
        ords = _json.loads((await c.get("/orders?size=1")).get("body"))
        oid = ords["items"][0]["order_id"]
        chk("order detail", await c.get(f"/orders/{oid}"))
        chk("order timeline", await c.get(f"/orders/{oid}/timeline"))

        # 404 behavior
        chk("product 404", await c.get("/products/does-not-exist"), expect_status=404)
        chk("search missing q", await c.get("/search"), expect_status=400)

        print("\n=== SMOKE RESULTS ===")
        failed = 0
        for name, ok, status in checks:
            print(f"  {'PASS' if ok else 'FAIL'}  {name}  (status {status})")
            if not ok:
                failed += 1
        print(f"\n{failed} failed of {len(checks)}")
        return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
