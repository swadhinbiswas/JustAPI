"""End-to-end tests for the expense tracker example.

Runs against the real JustAPI pipeline through ``JustAPITestClient`` (native
middleware, validation, routing — no server needed). A fresh SQLite file in a
temp directory is used per test session.

    pytest test_app.py -v
"""

from __future__ import annotations

import importlib.util
import json
import os
import sys
import tempfile
import time
from datetime import date

import pytest

# Configure the database *before* loading the app module.
_TMP_DIR = tempfile.mkdtemp(prefix="expenses-example-")
os.environ["EXPENSES_DB"] = os.path.join(_TMP_DIR, "test.db")

_HERE = os.path.dirname(os.path.abspath(__file__))


def _load_example_app():
    spec = importlib.util.spec_from_file_location(
        "expense_analytics_app", os.path.join(_HERE, "app.py")
    )
    module = importlib.util.module_from_spec(spec)
    sys.modules["expense_analytics_app"] = module
    spec.loader.exec_module(module)
    return module


_app_module = _load_example_app()
api = _app_module.app

from justapi import JustAPITestClient  # noqa: E402

client = JustAPITestClient(api)

JSON_HEADERS = [("Content-Type", "application/json")]


# --------------------------------------------------------------------------
# helpers
# --------------------------------------------------------------------------

def body(resp) -> dict:
    raw = bytes(resp["body"])
    return json.loads(raw) if raw else {}


def post(path: str, payload: dict):
    return client.post_with(path, json.dumps(payload).encode(), JSON_HEADERS)


def patch(path: str, payload: dict):
    return client.patch_with(path, json.dumps(payload).encode(), JSON_HEADERS)


def make_expense(**overrides) -> dict:
    payload = {
        "description": "Coffee",
        "amount": 4.5,
        "category": "food",
        "spent_on": "2026-01-15",
    }
    payload.update(overrides)
    resp = post("/expenses", payload)
    assert resp["status"] == 201, body(resp)
    return body(resp)


@pytest.fixture(autouse=True)
def _clean_tables():
    api.db.execute("DELETE FROM expenses")
    api.db.execute("DELETE FROM audit_log")
    yield


# --------------------------------------------------------------------------
# meta + docs
# --------------------------------------------------------------------------

def test_root_metadata():
    resp = client.get("/")
    assert resp["status"] == 200
    assert body(resp)["name"] == "Expense Tracker API"


def test_openapi_documents_the_api():
    resp = client.get("/openapi.json")
    assert resp["status"] == 200
    spec = body(resp)
    for path in ("/expenses", "/expenses/{expense_id}", "/analytics/summary"):
        assert path in spec["paths"], f"{path} missing from OpenAPI"
    assert "post" in spec["paths"]["/expenses"]
    schemas = spec.get("components", {}).get("schemas", {})
    assert "ExpenseCreate" in schemas
    amount = schemas["ExpenseCreate"]["properties"]["amount"]
    assert amount["type"] == "number"


# --------------------------------------------------------------------------
# CRUD
# --------------------------------------------------------------------------

def test_create_and_get():
    created = make_expense(description="Groceries", amount=42.75)
    assert created["id"] > 0
    assert created["description"] == "Groceries"
    assert created["amount"] == 42.75
    assert created["category"] == "food"
    assert created["spent_on"] == "2026-01-15"

    fetched = client.get(f"/expenses/{created['id']}")
    assert fetched["status"] == 200
    assert body(fetched) == created


def test_category_is_normalized():
    created = make_expense(category="  FOOD  ")
    assert created["category"] == "food"


def test_create_validation_errors():
    base = {"description": "x", "amount": 1.0, "category": "food", "spent_on": "2026-01-15"}

    for field, bad in [
        ("amount", -5),
        ("amount", 0),
        ("amount", 2_000_000),
        ("description", ""),
        ("category", ""),
        ("spent_on", "15/01/2026"),
    ]:
        payload = dict(base)
        payload[field] = bad
        resp = post("/expenses", payload)
        assert resp["status"] == 422, f"{field}={bad!r} -> {resp['status']}"

    missing = {"description": "x", "amount": 1.0, "category": "food"}
    assert post("/expenses", missing)["status"] == 422  # spent_on required


def test_get_missing_expense_is_404():
    resp = client.get("/expenses/999999")
    assert resp["status"] == 404
    assert "not found" in body(resp)["detail"]


def test_list_filters_and_pagination():
    make_expense(description="A", amount=10.0, category="food", spent_on="2026-01-10")
    make_expense(description="B", amount=50.0, category="transport", spent_on="2026-02-10")
    make_expense(description="C", amount=90.0, category="food", spent_on="2026-03-10")

    # No filters: all three, newest first.
    resp = client.get("/expenses")
    page = body(resp)
    assert resp["status"] == 200
    assert page["total"] == 3
    assert [item["description"] for item in page["items"]] == ["C", "B", "A"]

    # Category filter.
    assert body(client.get("/expenses?category=food"))["total"] == 2

    # Amount range.
    assert body(client.get("/expenses?min_amount=20&max_amount=60"))["total"] == 1

    # Date range (inclusive).
    assert body(client.get("/expenses?from_date=2026-01-10&to_date=2026-02-10"))["total"] == 2

    # Pagination.
    page = body(client.get("/expenses?limit=2&offset=2"))
    assert len(page["items"]) == 1 and page["items"][0]["description"] == "A"


def test_list_query_constraints():
    assert client.get("/expenses?limit=0")["status"] == 422
    assert client.get("/expenses?limit=500")["status"] == 422
    assert client.get("/expenses?offset=-1")["status"] == 422
    assert client.get("/expenses?from_date=10-01-2026")["status"] == 422


def test_patch_updates_only_given_fields():
    created = make_expense(description="Lunch", amount=12.0)

    resp = patch(f"/expenses/{created['id']}", {"amount": 15.5})
    assert resp["status"] == 200
    updated = body(resp)
    assert updated["amount"] == 15.5
    assert updated["description"] == "Lunch"  # untouched
    assert updated["category"] == "food"

    # Multi-field update, including a date string.
    resp = patch(
        f"/expenses/{created['id']}",
        {"category": "Dining", "spent_on": "2026-02-01"},
    )
    updated = body(resp)
    assert updated["category"] == "dining"
    assert updated["spent_on"] == "2026-02-01"


def test_patch_edge_cases():
    created = make_expense()
    assert patch(f"/expenses/{created['id']}", {})["status"] == 400
    assert patch(f"/expenses/{created['id']}", {"amount": -1})["status"] == 422
    assert patch("/expenses/999999", {"amount": 1})["status"] == 404


def test_delete():
    created = make_expense()
    resp = client.delete(f"/expenses/{created['id']}")
    assert resp["status"] == 204
    assert bytes(resp["body"]) == b""
    assert client.get(f"/expenses/{created['id']}")["status"] == 404
    assert client.delete(f"/expenses/{created['id']}")["status"] == 404


# --------------------------------------------------------------------------
# analytics
# --------------------------------------------------------------------------

def test_summary_math():
    make_expense(amount=10.0, category="food", spent_on="2026-01-05")
    make_expense(amount=30.0, category="transport", spent_on="2026-02-05")
    make_expense(amount=20.0, category="food", spent_on="2026-03-05")

    summary = body(client.get("/analytics/summary"))
    assert summary["count"] == 3
    assert summary["total"] == 60.0
    assert summary["average"] == 20.0
    assert summary["smallest"] == 10.0
    assert summary["largest"] == 30.0
    assert summary["first_day"] == "2026-01-05"
    assert summary["last_day"] == "2026-03-05"

    food = body(client.get("/analytics/summary?category=food"))
    assert food["count"] == 2
    assert food["total"] == 30.0


def test_summary_on_empty_db():
    summary = body(client.get("/analytics/summary"))
    assert summary["count"] == 0
    assert summary["total"] == 0


def test_by_category_shares():
    make_expense(amount=75.0, category="food")
    make_expense(amount=25.0, category="transport")

    report = body(client.get("/analytics/by-category"))
    categories = {row["category"]: row for row in report["categories"]}
    assert report["grand_total"] == 100.0
    assert categories["food"]["total"] == 75.0
    assert categories["food"]["share_pct"] == 75.0
    assert categories["transport"]["share_pct"] == 25.0
    assert sum(row["share_pct"] for row in report["categories"]) == 100.0


def test_monthly_series():
    make_expense(amount=10.0, spent_on="2026-01-05")
    make_expense(amount=20.0, spent_on="2026-01-20")
    make_expense(amount=5.0, spent_on="2026-02-01")

    months = body(client.get("/analytics/monthly"))["months"]
    assert months == [
        {"month": "2026-01", "count": 2, "total": 30.0},
        {"month": "2026-02", "count": 1, "total": 5.0},
    ]


def test_trend_includes_today():
    make_expense(amount=33.0, spent_on=date.today().isoformat())
    make_expense(amount=99.0, spent_on="2000-01-01")  # far outside the window

    report = body(client.get("/analytics/trend?days=30"))
    assert report["days"] == 30
    assert len(report["points"]) == 1
    assert report["points"][0]["day"] == date.today().isoformat()
    assert report["points"][0]["total"] == 33.0

    assert client.get("/analytics/trend?days=0")["status"] == 422
    assert client.get("/analytics/trend?days=366")["status"] == 422


def test_top_expenses():
    for amount in (5.0, 90.0, 40.0, 70.0):
        make_expense(amount=amount)

    top = body(client.get("/analytics/top?n=2"))["top"]
    assert [row["amount"] for row in top] == [90.0, 70.0]
    assert client.get("/analytics/top?n=0")["status"] == 422
    assert client.get("/analytics/top?n=21")["status"] == 422


# --------------------------------------------------------------------------
# SSE + background tasks + health
# --------------------------------------------------------------------------

def test_sse_summary_stream():
    make_expense(amount=12.5)
    resp = client.get("/stream/summary")
    assert resp["status"] == 200
    text = bytes(resp["body"]).decode()
    events = [line for line in text.splitlines() if line.startswith("data: ")]
    assert len(events) == 5
    payload = json.loads(events[0][len("data: "):])
    assert payload["count"] == 1
    assert payload["total"] == 12.5


def test_background_tasks_write_audit_log():
    created = make_expense()
    patch(f"/expenses/{created['id']}", {"amount": 6.0})
    client.delete(f"/expenses/{created['id']}")

    deadline = time.time() + 2
    while time.time() < deadline:
        rows = api.db.query(
            "SELECT action FROM audit_log WHERE expense_id = ? ORDER BY id",
            [created["id"]],
        )
        if len(rows) == 3:
            break
        time.sleep(0.05)
    assert [row["action"] for row in rows] == ["create", "update", "delete"]


def test_ready_reports_database_healthy():
    resp = client.get("/ready")
    assert resp["status"] == 200
    report = body(resp)
    assert report.get("status") in ("ready", "ok") or "database" in json.dumps(report)
