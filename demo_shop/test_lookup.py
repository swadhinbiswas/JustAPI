"""Lookup flow check for the MVP shop (in-process, via AsyncTestClient).

Verifies the read/lookup surface: status, categories, products, sellers
(list + detail), customers (list + detail). Run:  python test_lookup.py
"""
from __future__ import annotations

import asyncio
import json
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import shop as a
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


def PJ(r):
    return json.loads(r["body"]) if r.get("body") else {}


async def main():
    async with AsyncTestClient(a.app) as c:
        print("=== status ===")
        r = await c.get("/shop/status")
        b = PJ(r)
        check("health ok", r.get("status") == 200 and b.get("health") == "ok", r)

        print("=== categories ===")
        r = await c.get("/categories?limit=3")
        b = PJ(r)
        check("categories listed", r.get("status") == 200 and b.get("items"), r)
        check("category has product_count", "product_count" in b["items"][0], b["items"][0])

        print("=== products ===")
        r = await c.get("/products?size=3&sort=price&order=desc")
        b = PJ(r)
        check("products paginated", b["total"] == 32951 and len(b["items"]) == 3, b.get("total"))
        pid0 = b["items"][0]["id"]
        r = await c.get(f"/products/{pid0}")
        check("product detail", r.get("status") == 200 and PJ(r)["id"] == pid0, r)
        r = await c.get("/products/does_not_exist")
        check("product 404", r.get("status") == 404, r)

        print("=== sellers (NEW list endpoint) ===")
        r = await c.get("/sellers?size=2")
        b = PJ(r)
        check("sellers listed (no 500)", r.get("status") == 200, r)
        check("sellers has total", isinstance(b.get("total"), int) and b["total"] > 0, b)
        check("seller row has stats cols",
              b["items"] and {"id", "orders", "gmv", "units"} <= set(b["items"][0]), b.get("items"))
        r = await c.get("/sellers?sort=units&order=desc&size=1")
        check("sellers sort=units ok", r.get("status") == 200, r)
        r = await c.get("/sellers?state=SP&size=1")
        b = PJ(r)
        check("sellers filter by state", r.get("status") == 200 and (not b["items"] or b["items"][0]["state"] == "SP"), b)
        sid = b["items"][0]["id"] if b.get("items") else None
        if sid:
            r = await c.get(f"/sellers/{sid}")
            check("seller detail", r.get("status") == 200 and PJ(r)["id"] == sid, r)
        r = await c.get("/sellers/nope_404")
        check("seller 404", r.get("status") == 404, r)

        print("=== customers (NEW list endpoint) ===")
        r = await c.get("/customers?size=2")
        b = PJ(r)
        check("customers listed", r.get("status") == 200 and b.get("total", 0) > 0, r)
        check("customer row has orders col",
              b["items"] and "orders" in b["items"][0], b.get("items"))
        r = await c.get("/customers?state=SP&size=1")
        b = PJ(r)
        check("customers filter by state",
              r.get("status") == 200 and (not b["items"] or b["items"][0]["state"] == "SP"), b)
        r = await c.get("/customers?state=ZZ")
        check("customers bad state -> empty 200",
              r.get("status") == 200 and PJ(r).get("total", 1) == 0, r)
        cid = b["items"][0]["id"] if b.get("items") else None
        if cid:
            r = await c.get(f"/customers/{cid}")
            check("customer detail", r.get("status") == 200 and PJ(r)["id"] == cid, r)
        r = await c.get("/customers/nope_404")
        check("customer 404", r.get("status") == 404, r)


if __name__ == "__main__":
    asyncio.run(main())
    print(f"\nLOOKUP RESULTS\n  {PASS} passed, {FAIL} failed")
    sys.exit(1 if FAIL else 0)
