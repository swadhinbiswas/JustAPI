"""Production-shaped test suite for the Order Platform API.

Covers the paths that break real systems:

* auth/authorization (401/403), ownership isolation (no existence leaks)
* validation at every boundary (Pydantic, query params, path params)
* atomic multi-item orders — a failure on item 2 must undo item 1
* **concurrent orders never oversell** (12 buyers, 5 units -> exactly 5 x 201)
* idempotency keys, including a concurrent retry storm (one order, one decrement)
* cancel restores stock exactly once (double cancel -> 409)
* pagination/filter/sort/search edges, SQL-injection attempts
* readiness, metrics, request IDs
* the same flows on Postgres (opt-in via JUSTAPI_TEST_POSTGRES_URL)

Run:

    pytest test_platform.py -v
"""

from __future__ import annotations

import importlib.util
import json
import os
import sys
import tempfile
import threading

import pytest

# The app reads PLATFORM_DB at import time.
_TMP = tempfile.mkdtemp(prefix="order-platform-")
os.environ["PLATFORM_DB"] = os.path.join(_TMP, "platform-test.db")

_HERE = os.path.dirname(os.path.abspath(__file__))


def _load_app(module_name: str, env_overrides: dict | None = None):
    saved = {k: os.environ.get(k) for k in (env_overrides or {})}
    for key, value in (env_overrides or {}).items():
        os.environ[key] = value
    try:
        spec = importlib.util.spec_from_file_location(
            module_name, os.path.join(_HERE, "app.py")
        )
        module = importlib.util.module_from_spec(spec)
        sys.modules[module_name] = module
        spec.loader.exec_module(module)
        return module
    finally:
        for key, value in saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


platform = _load_app("order_platform_app")
api = platform.app

from justapi import JustAPITestClient  # noqa: E402

client = JustAPITestClient(api)

ADMIN_KEY = "admin-dev-key"
CUSTOMER_KEY = "customer-dev-key"
CUSTOMER2_KEY = "customer-2-dev-key"
JSON = ("Content-Type", "application/json")


def hdr(key: str) -> list:
    return [("X-API-Key", key), JSON]


def body(resp) -> dict:
    raw = bytes(resp["body"])
    return json.loads(raw) if raw else {}


def header(resp, name: str):
    headers = resp.get("headers") or {}
    items = headers.items() if isinstance(headers, dict) else headers
    for key, value in items:
        if isinstance(key, bytes):
            key = key.decode()
        if isinstance(value, bytes):
            value = value.decode()
        if str(key).lower() == name:
            return value
    return None


def post(path: str, payload: dict, key: str = CUSTOMER_KEY):
    return client.post_with(path, json.dumps(payload).encode(), hdr(key))


def patch(path: str, payload: dict, key: str = ADMIN_KEY):
    return client.patch_with(path, json.dumps(payload).encode(), hdr(key))


def get(path: str, key: str = CUSTOMER_KEY):
    return client.get_with(path, [("X-API-Key", key)])


def stock(sku: str) -> int:
    return api.db.query("SELECT stock FROM products WHERE sku = ?", [sku])[0]["stock"]


def order_count() -> int:
    return api.db.query("SELECT COUNT(*) AS n FROM orders")[0]["n"]


def create_order(items, key: str = CUSTOMER_KEY, idem: str | None = None) -> dict:
    payload = {"items": items}
    if idem is not None:
        payload["idempotency_key"] = idem
    resp = post("/orders", payload, key)
    assert resp["status"] == 201, body(resp)
    return body(resp)


def cancel(order_id: str, key: str = CUSTOMER_KEY):
    return client.post_with(f"/orders/{order_id}/cancel", b"", [("X-API-Key", key)])


def run_parallel(fn, count: int):
    results: list = []
    lock = threading.Lock()

    def worker(index: int):
        value = fn(index)
        with lock:
            results.append(value)

    threads = [threading.Thread(target=worker, args=(i,)) for i in range(count)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    return results


@pytest.fixture(autouse=True)
def _reset():
    platform.seed_database(api.db, force=True)
    yield


# ---------------------------------------------------------------------------
# Auth & authorization
# ---------------------------------------------------------------------------

def test_missing_api_key_is_401():
    resp = client.get("/products")
    assert resp["status"] == 401
    assert body(resp)["detail"] == "missing X-API-Key header"


def test_invalid_api_key_is_401():
    resp = client.get_with("/products", [("X-API-Key", "nope")])
    assert resp["status"] == 401
    assert body(resp)["detail"] == "invalid API key"


def test_customer_cannot_create_product():
    resp = post("/products", {"sku": "x", "name": "X", "price_cents": 100, "stock": 1}, CUSTOMER_KEY)
    assert resp["status"] == 403
    assert body(resp)["detail"] == "admin role required"


def test_admin_can_create_product():
    resp = post("/products", {"sku": "cable", "name": "USB Cable", "price_cents": 900, "stock": 10}, ADMIN_KEY)
    assert resp["status"] == 201
    assert body(resp)["sku"] == "cable"


# ---------------------------------------------------------------------------
# Product validation & listing
# ---------------------------------------------------------------------------

@pytest.mark.parametrize(
    "payload",
    [
        {"sku": "UPPER", "name": "x", "price_cents": 100, "stock": 1},
        {"sku": "", "name": "x", "price_cents": 100, "stock": 1},
        {"sku": "ok", "name": "", "price_cents": 100, "stock": 1},
        {"sku": "ok", "name": "x", "price_cents": 0, "stock": 1},
        {"sku": "ok", "name": "x", "price_cents": 100, "stock": -1},
        {"sku": "ok", "name": "x", "price_cents": 100},
        {"sku": "ok", "name": "x", "price_cents": "abc", "stock": 1},
    ],
)
def test_product_validation(payload):
    assert post("/products", payload, ADMIN_KEY)["status"] == 422


def test_duplicate_product_is_409():
    payload = {"sku": "keyboard", "name": "Dup", "price_cents": 100, "stock": 1}
    resp = post("/products", payload, ADMIN_KEY)
    assert resp["status"] == 409


def test_product_list_pagination_search_and_sort():
    page = body(get("/products?limit=2&offset=0"))
    assert page["total"] == 5
    assert len(page["items"]) == 2

    assert body(get("/products?q=monitor"))["total"] == 1
    assert body(get("/products?q=KEY"))["total"] == 1  # case-insensitive LIKE on SQLite

    prices = [p["price_cents"] for p in body(get("/products?sort=price&limit=100"))["items"]]
    assert prices == sorted(prices)
    prices_desc = [p["price_cents"] for p in body(get("/products?sort=price_desc&limit=100"))["items"]]
    assert prices_desc == sorted(prices_desc, reverse=True)

    assert body(get("/products?in_stock=false&limit=100"))["total"] == 5
    api.db.execute("UPDATE products SET stock = 0 WHERE sku = 'webcam'")
    assert body(get("/products?in_stock=true&limit=100"))["total"] == 4

    # Out-of-range pagination
    assert get("/products?limit=0")["status"] == 422
    assert get("/products?limit=101")["status"] == 422
    assert get("/products?offset=-1")["status"] == 422
    assert get("/products?sort=bogus")["status"] == 422
    assert body(get("/products?offset=9999"))["items"] == []


def test_get_product_missing_is_404():
    assert get("/products/nope")["status"] == 404


def test_patch_product_and_empty_patch():
    resp = patch("/products/mouse", {"price_cents": 5500})
    assert resp["status"] == 200
    assert body(resp)["price_cents"] == 5500
    assert patch("/products/mouse", {})["status"] == 400
    assert patch("/products/nope", {"price_cents": 1})["status"] == 404


def test_sql_injection_in_search_is_inert():
    resp = get("/products?q=%27%20OR%201%3D1%20--")
    assert resp["status"] == 200
    assert body(resp)["total"] == 0
    assert body(get("/products"))["total"] == 5


# ---------------------------------------------------------------------------
# Order creation: totals, stock, atomicity
# ---------------------------------------------------------------------------

def test_create_order_totals_and_stock():
    before = {"keyboard": stock("keyboard"), "mouse": stock("mouse")}
    order = create_order(
        [{"sku": "keyboard", "qty": 2}, {"sku": "mouse", "qty": 1}]
    )
    assert order["total_cents"] == 2 * 12900 + 4900
    assert order["status"] == "pending"
    assert [item["sku"] for item in order["items"]] == ["keyboard", "mouse"]
    assert stock("keyboard") == before["keyboard"] - 2
    assert stock("mouse") == before["mouse"] - 1
    events = body(get(f"/orders/{order['id']}/events"))["events"]
    assert [e["event"] for e in events] == ["created"]


def test_insufficient_stock_is_409_and_changes_nothing():
    api.db.execute("UPDATE products SET stock = 2 WHERE sku = 'monitor'")
    before_orders = order_count()
    resp = post("/orders", {"items": [{"sku": "monitor", "qty": 3}]})
    assert resp["status"] == 409
    assert "insufficient stock for monitor" in body(resp)["detail"]
    assert stock("monitor") == 2
    assert order_count() == before_orders


def test_multi_item_order_is_atomic():
    """Item 1 succeeds, item 2 is short -> item 1's decrement must roll back."""
    api.db.execute("UPDATE products SET stock = 5 WHERE sku = 'monitor'")
    api.db.execute("UPDATE products SET stock = 1 WHERE sku = 'webcam'")
    before_monitor = stock("monitor")
    before_orders = order_count()
    resp = post(
        "/orders",
        {
            "items": [
                {"sku": "monitor", "qty": 1},
                {"sku": "webcam", "qty": 2},  # only 1 in stock
            ]
        },
    )
    assert resp["status"] == 409
    assert stock("monitor") == before_monitor  # rolled back
    assert stock("webcam") == 1
    assert order_count() == before_orders


def test_unknown_sku_is_rejected_without_writes():
    before_orders = order_count()
    resp = post("/orders", {"items": [{"sku": "ghost", "qty": 1}]})
    assert resp["status"] == 409
    assert order_count() == before_orders


@pytest.mark.parametrize(
    "payload",
    [
        {"items": []},
        {"items": [{"sku": "mouse", "qty": 0}]},
        {"items": [{"sku": "mouse", "qty": -1}]},
        {"items": [{"sku": "", "qty": 1}]},
        {"items": [{"sku": "mouse", "qty": 1}], "idempotency_key": "short"},
        {"items": [{"sku": "mouse", "qty": 1}] * 21},
    ],
)
def test_order_validation(payload):
    assert post("/orders", payload)["status"] == 422


# ---------------------------------------------------------------------------
# Idempotency
# ---------------------------------------------------------------------------

def test_idempotent_replay_returns_same_order_and_decrements_once():
    before = stock("mouse")
    first = create_order([{"sku": "mouse", "qty": 2}], idem="order-key-0001")
    second = create_order([{"sku": "mouse", "qty": 2}], idem="order-key-0001")
    assert second["id"] == first["id"]
    assert stock("mouse") == before - 2
    assert order_count() == 1


def test_idempotency_keys_are_scoped_per_customer():
    first = create_order([{"sku": "mouse", "qty": 1}], CUSTOMER_KEY, idem="shared-key-0001")
    second = create_order([{"sku": "mouse", "qty": 1}], CUSTOMER2_KEY, idem="shared-key-0001")
    assert first["id"] != second["id"]


def test_concurrent_idempotent_retries_create_one_order():
    before = stock("mouse")

    def place(_index):
        local = JustAPITestClient(api)
        resp = local.post_with(
            "/orders",
            json.dumps(
                {"items": [{"sku": "mouse", "qty": 3}], "idempotency_key": "storm-key-0001"}
            ).encode(),
            hdr(CUSTOMER_KEY),
        )
        return resp["status"], body(resp).get("id")

    results = run_parallel(place, 8)
    assert all(status == 201 for status, _ in results), results
    ids = {order_id for _, order_id in results}
    assert len(ids) == 1
    assert order_count() == 1
    assert stock("mouse") == before - 3


# ---------------------------------------------------------------------------
# Concurrency: never oversell
# ---------------------------------------------------------------------------

def test_concurrent_orders_never_oversell():
    api.db.execute("UPDATE products SET stock = 5 WHERE sku = 'keyboard'")

    def place(_index):
        local = JustAPITestClient(api)
        resp = local.post_with(
            "/orders",
            json.dumps({"items": [{"sku": "keyboard", "qty": 1}]}).encode(),
            hdr(CUSTOMER_KEY),
        )
        return resp["status"]

    statuses = run_parallel(place, 12)
    assert statuses.count(201) == 5, statuses
    assert all(status in (201, 409) for status in statuses)
    assert stock("keyboard") == 0
    assert order_count() == 5
    assert api.db.query("SELECT COUNT(*) AS n FROM order_items")[0]["n"] == 5
    assert api.db.query("SELECT COUNT(*) AS n FROM order_events")[0]["n"] == 5


# ---------------------------------------------------------------------------
# Ownership & listing
# ---------------------------------------------------------------------------

def test_customers_only_see_their_own_orders():
    ada_order = create_order([{"sku": "mouse", "qty": 1}], CUSTOMER_KEY)
    grace_order = create_order([{"sku": "mouse", "qty": 1}], CUSTOMER2_KEY)

    ada_ids = {o["id"] for o in body(get("/orders", CUSTOMER_KEY))["items"]}
    grace_ids = {o["id"] for o in body(get("/orders", CUSTOMER2_KEY))["items"]}
    assert ada_ids == {ada_order["id"]}
    assert grace_ids == {grace_order["id"]}

    # No existence leak: another customer's order is indistinguishable from missing.
    assert get(f"/orders/{ada_order['id']}", CUSTOMER2_KEY)["status"] == 404
    assert get("/orders/does-not-exist", CUSTOMER_KEY)["status"] == 404


def test_admin_sees_all_orders_and_stats():
    create_order([{"sku": "mouse", "qty": 1}], CUSTOMER_KEY)
    create_order([{"sku": "webcam", "qty": 1}], CUSTOMER2_KEY)
    admin_orders = body(get("/orders", ADMIN_KEY))
    assert admin_orders["total"] == 2

    stats = body(get("/admin/stats", ADMIN_KEY))
    assert stats["orders"] == 2
    assert stats["revenue_cents"] == 4900 + 5900
    by_status = {row["status"]: row["count"] for row in stats["by_status"]}
    assert by_status == {"pending": 2}
    assert [row["sku"] for row in stats["low_stock"]] == ["webcam"]


def test_stats_excludes_cancelled_revenue():
    order = create_order([{"sku": "mouse", "qty": 2}])
    assert cancel(order["id"], CUSTOMER_KEY)["status"] == 200
    stats = body(get("/admin/stats", ADMIN_KEY))
    assert stats["orders"] == 0
    assert stats["revenue_cents"] == 0


def test_order_list_filters_and_pagination():
    for _ in range(3):
        create_order([{"sku": "mouse", "qty": 1}], CUSTOMER_KEY)
    cancelled = create_order([{"sku": "mouse", "qty": 1}], CUSTOMER_KEY)
    assert cancel(cancelled["id"], CUSTOMER_KEY)["status"] == 200

    assert body(get("/orders?status=pending", CUSTOMER_KEY))["total"] == 3
    assert body(get("/orders?status=cancelled", CUSTOMER_KEY))["total"] == 1
    assert body(get("/orders?limit=2", CUSTOMER_KEY))["items"].__len__() == 2
    assert body(get("/orders?offset=9999", CUSTOMER_KEY))["items"] == []
    assert get("/orders?status=bogus", CUSTOMER_KEY)["status"] == 422


# ---------------------------------------------------------------------------
# Cancellation
# ---------------------------------------------------------------------------

def test_cancel_restores_stock_exactly_once():
    before = stock("monitor")
    order = create_order([{"sku": "monitor", "qty": 2}])
    assert stock("monitor") == before - 2

    resp = cancel(order["id"], CUSTOMER_KEY)
    assert resp["status"] == 200
    assert body(resp)["status"] == "cancelled"
    assert stock("monitor") == before

    # Second cancel is a conflict and must not restore stock again.
    again = cancel(order["id"], CUSTOMER_KEY)
    assert again["status"] == 409
    assert stock("monitor") == before

    events = body(get(f"/orders/{order['id']}/events"))["events"]
    assert [e["event"] for e in events] == ["created", "cancelled"]


def test_cancel_foreign_order_is_404():
    order = create_order([{"sku": "mouse", "qty": 1}], CUSTOMER_KEY)
    assert cancel(order["id"], CUSTOMER2_KEY)["status"] == 404
    assert stock("mouse") == 39  # unchanged


def test_admin_can_cancel_any_order():
    order = create_order([{"sku": "mouse", "qty": 1}], CUSTOMER_KEY)
    assert cancel(order["id"], ADMIN_KEY)["status"] == 200


# ---------------------------------------------------------------------------
# Concurrency: cancel vs create
# ---------------------------------------------------------------------------

def test_concurrent_cancel_and_reorder_keep_stock_consistent():
    api.db.execute("UPDATE products SET stock = 10 WHERE sku = 'usb-c-hub'")
    order = create_order([{"sku": "usb-c-hub", "qty": 4}])

    def worker(index):
        local = JustAPITestClient(api)
        if index % 2 == 0:
            resp = local.post_with(
                f"/orders/{order['id']}/cancel", b"", [("X-API-Key", CUSTOMER_KEY)]
            )
            return ("cancel", resp["status"])
        resp = local.post_with(
            "/orders",
            json.dumps({"items": [{"sku": "usb-c-hub", "qty": 1}]}).encode(),
            hdr(CUSTOMER_KEY),
        )
        return ("order", resp["status"])

    results = run_parallel(worker, 10)
    cancels = [status for kind, status in results if kind == "cancel" and status == 200]
    assert len(cancels) == 1  # exactly one cancel wins
    # Stock invariant: 10 - 4 (initial) + 4 (cancel) - (number of successful new orders) >= 0
    new_orders = len([1 for kind, status in results if kind == "order" and status == 201])
    assert stock("usb-c-hub") == 10 - new_orders
    assert stock("usb-c-hub") >= 0


# ---------------------------------------------------------------------------
# Platform plumbing
# ---------------------------------------------------------------------------

def test_request_id_is_echoed():
    resp = get("/products")
    assert resp["status"] == 200
    assert header(resp, "x-request-id")
    assert len(header(resp, "x-request-id")) == 32


def test_ready_and_metrics():
    assert client.get("/ready")["status"] == 200
    metrics = client.get("/metrics")
    assert metrics["status"] == 200
    text = bytes(metrics["body"]).decode()
    # The TestClient bypasses the server's metrics recorder; either the live
    # exposition or the explicit not-initialised marker is acceptable here.
    assert "justapi_requests_total" in text or "metrics not yet initialised" in text
    assert header(metrics, "content-type").startswith("text/plain")


def test_error_responses_are_json():
    resp = get("/products/missing")
    assert resp["status"] == 404
    assert header(resp, "content-type") == "application/json"
    assert "detail" in body(resp)


# ---------------------------------------------------------------------------
# Postgres parity (opt-in: JUSTAPI_TEST_POSTGRES_URL)
# ---------------------------------------------------------------------------

PG_URL = os.environ.get("JUSTAPI_TEST_POSTGRES_URL")


@pytest.mark.skipif(not PG_URL, reason="set JUSTAPI_TEST_POSTGRES_URL to run")
def test_postgres_end_to_end():
    pg = _load_app("order_platform_pg", {"PLATFORM_DB": PG_URL})
    pg.seed_database(pg.app.db, force=True)
    pg_client = JustAPITestClient(pg.app)

    def call(method, path, payload=None, key=CUSTOMER_KEY):
        headers = [("X-API-Key", key), JSON]
        if method == "get":
            return pg_client.get_with(path, [("X-API-Key", key)])
        fn = pg_client.post_with if method == "post" else pg_client.patch_with
        return fn(path, json.dumps(payload).encode(), headers)

    # Admin creates a product; customer orders it atomically and idempotently.
    created = call("post", "/products", {"sku": "pg-widget", "name": "PG Widget", "price_cents": 1500, "stock": 3}, ADMIN_KEY)
    assert created["status"] == 201, bytes(created["body"])

    first = call("post", "/orders", {"items": [{"sku": "pg-widget", "qty": 2}], "idempotency_key": "pg-key-0001"})
    assert first["status"] == 201, bytes(first["body"])
    order = json.loads(bytes(first["body"]))

    replay = call("post", "/orders", {"items": [{"sku": "pg-widget", "qty": 2}], "idempotency_key": "pg-key-0001"})
    assert replay["status"] == 201
    assert json.loads(bytes(replay["body"]))["id"] == order["id"]

    remaining = pg.app.db.query("SELECT stock FROM products WHERE sku = 'pg-widget'")[0]["stock"]
    assert remaining == 1  # decremented once

    # Multi-item rollback on Postgres.
    short = call("post", "/orders", {"items": [{"sku": "pg-widget", "qty": 1}, {"sku": "mouse", "qty": 999}]})
    assert short["status"] == 409
    assert pg.app.db.query("SELECT stock FROM products WHERE sku = 'pg-widget'")[0]["stock"] == 1

    # Cancel restores stock.
    cancelled = call("post", f"/orders/{order['id']}/cancel")
    assert cancelled["status"] == 200
    assert pg.app.db.query("SELECT stock FROM products WHERE sku = 'pg-widget'")[0]["stock"] == 3

    pg.seed_database(pg.app.db, force=True)
