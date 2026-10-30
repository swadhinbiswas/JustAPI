"""Expense Tracker with Analytics — a complete JustAPI example application.

Shows a realistic, production-shaped API built entirely with JustAPI:

* SQLite persistence through the Rust-native pool (``app.set_database``),
  with the schema applied via ``init_sql`` on every pooled connection.
* Pydantic request models with field constraints (the flagship FastAPI
  pattern: ``def create(item: ExpenseCreate)``).
* Query parameters with ``Query(...)`` constraints (``ge``/``le``/``pattern``).
* CRUD endpoints with proper status codes (201/200/204/404/400/422).
* Analytics endpoints: aggregates, group-by, time series, top-N — computed in
  SQLite, returned as JSON.
* Server-Sent Events (``@app.sse``) for a live summary feed.
* Background tasks (audit log written after the response).
* Health/readiness checks (``/health``, ``/ready``) wired to the database.

Run it::

    cd examples/expense_analytics
    python seed.py          # optional: ~6 months of demo data
    python app.py           # http://127.0.0.1:8000  (docs at /docs)

Test it::

    pytest test_app.py -v
"""

from __future__ import annotations

import asyncio
import json
import os
from datetime import date

from pydantic import BaseModel, Field

from justapi import BackgroundTasks, HTTPException, JustAPIApp, Query
from justapi.logging import init_json_logging, init_logging

DB_PATH = os.environ.get("EXPENSES_DB", "expenses.db")
IS_POSTGRES = DB_PATH.startswith(("postgres://", "postgresql://"))

# Structured logging for production: LOG_FORMAT=json emits one JSON object per
# event (ready for Loki/CloudWatch/Datadog); the default is human-readable text.
if os.environ.get("LOG_FORMAT", "text").strip().lower() == "json":
    init_json_logging()
else:
    init_logging(level=os.environ.get("LOG_LEVEL", "info"))

# Applied by the pool once at startup; `IF NOT EXISTS` makes it idempotent.
# `app.db.query(...)` accepts `?` placeholders on every driver (sqlx rewrites
# them for Postgres), so only DDL and date functions differ between dialects.
if IS_POSTGRES:
    SCHEMA = """
CREATE TABLE IF NOT EXISTS expenses (
    id SERIAL PRIMARY KEY,
    description TEXT NOT NULL,
    amount DOUBLE PRECISION NOT NULL CHECK (amount > 0),
    category TEXT NOT NULL,
    spent_on TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (now()::text)
);
CREATE INDEX IF NOT EXISTS idx_expenses_category ON expenses(category);
CREATE INDEX IF NOT EXISTS idx_expenses_spent_on ON expenses(spent_on);
CREATE TABLE IF NOT EXISTS audit_log (
    id SERIAL PRIMARY KEY,
    action TEXT NOT NULL,
    expense_id INTEGER,
    at TEXT NOT NULL DEFAULT (now()::text)
);
"""
else:
    SCHEMA = """
CREATE TABLE IF NOT EXISTS expenses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    description TEXT NOT NULL,
    amount REAL NOT NULL CHECK (amount > 0),
    category TEXT NOT NULL,
    spent_on TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_expenses_category ON expenses(category);
CREATE INDEX IF NOT EXISTS idx_expenses_spent_on ON expenses(spent_on);
CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    action TEXT NOT NULL,
    expense_id INTEGER,
    at TEXT NOT NULL DEFAULT (datetime('now'))
);
"""


# --------------------------------------------------------------------------
# Request models (validated by Pydantic; violations become 422 responses)
# --------------------------------------------------------------------------

class ExpenseCreate(BaseModel):
    description: str = Field(min_length=1, max_length=200)
    amount: float = Field(gt=0, le=1_000_000, description="Amount spent, > 0")
    category: str = Field(min_length=1, max_length=50)
    spent_on: date = Field(description="ISO date, e.g. 2026-09-30")


class ExpenseUpdate(BaseModel):
    description: str | None = Field(default=None, min_length=1, max_length=200)
    amount: float | None = Field(default=None, gt=0, le=1_000_000)
    category: str | None = Field(default=None, min_length=1, max_length=50)
    spent_on: date | None = None


# --------------------------------------------------------------------------
# App setup
# --------------------------------------------------------------------------

app = JustAPIApp(
    title="Expense Tracker API",
    version="1.0.0",
    description=(
        "Track expenses and analyze spending — a complete JustAPI example "
        "application with SQLite, CRUD, analytics, SSE and background tasks."
    ),
)

app.set_database(
    DB_PATH if IS_POSTGRES else f"sqlite:{DB_PATH}",
    init_sql=SCHEMA,
    pragmas=["journal_mode=WAL", "synchronous=NORMAL"],
)
app.enable_secure_headers()
app.enable_timeout(30)
app.enable_request_id()  # x-request-id: exposed to handlers, echoed, logged
app.register_health_check("database", lambda: len(app.db.query("SELECT 1")) == 1)


# --------------------------------------------------------------------------
# Helpers
# --------------------------------------------------------------------------

def _fetch_or_404(expense_id: int) -> dict:
    rows = app.db.query("SELECT * FROM expenses WHERE id = ?", [expense_id])
    if not rows:
        raise HTTPException(status_code=404, detail=f"expense {expense_id} not found")
    return rows[0]


def _normalize(column: str, value):
    """Canonicalise a column value before it hits SQLite."""
    if column == "category":
        return str(value).strip().lower()
    if column == "spent_on":
        return str(value)
    return value


def _record_audit(action: str, expense_id: int | None) -> None:
    """Runs as a background task after the response has been sent.

    When there is no expense id the column is omitted rather than bound as
    NULL: a NULL parameter is typed as text by the driver, which Postgres
    rejects for an integer column.
    """
    if expense_id is None:
        app.db.execute("INSERT INTO audit_log (action) VALUES (?)", [action])
    else:
        app.db.execute(
            "INSERT INTO audit_log (action, expense_id) VALUES (?, ?)",
            [action, expense_id],
        )


def _where_clause(
    category: str | None,
    min_amount: float | None,
    max_amount: float | None,
    from_date: str | None,
    to_date: str | None,
) -> tuple[str, list]:
    """Build a parameterised WHERE clause (never string-interpolate values)."""
    conditions: list[str] = []
    params: list = []
    if category:
        conditions.append("category = ?")
        params.append(category.strip().lower())
    if min_amount is not None:
        conditions.append("amount >= ?")
        params.append(min_amount)
    if max_amount is not None:
        conditions.append("amount <= ?")
        params.append(max_amount)
    if from_date:
        conditions.append("spent_on >= ?")
        params.append(from_date)
    if to_date:
        conditions.append("spent_on <= ?")
        params.append(to_date)
    clause = f" WHERE {' AND '.join(conditions)}" if conditions else ""
    return clause, params


# --------------------------------------------------------------------------
# Meta
# --------------------------------------------------------------------------

@app.get("/", tags=["meta"], summary="Service metadata")
def root():
    return {
        "name": "Expense Tracker API",
        "version": "1.0.0",
        "docs": "/docs",
        "endpoints": [
            "POST /expenses",
            "GET /expenses",
            "GET /expenses/{id}",
            "PATCH /expenses/{id}",
            "DELETE /expenses/{id}",
            "GET /analytics/summary",
            "GET /analytics/by-category",
            "GET /analytics/monthly",
            "GET /analytics/trend",
            "GET /analytics/top",
            "GET /stream/summary",
        ],
    }


# --------------------------------------------------------------------------
# CRUD
# --------------------------------------------------------------------------

@app.post("/expenses", status_code=201, tags=["expenses"], summary="Create an expense")
def create_expense(item: ExpenseCreate, background_tasks: BackgroundTasks):
    rows = app.db.query(
        "INSERT INTO expenses (description, amount, category, spent_on) "
        "VALUES (?, ?, ?, ?) RETURNING *",
        [
            item.description,
            item.amount,
            _normalize("category", item.category),
            _normalize("spent_on", item.spent_on),
        ],
    )
    expense = rows[0]
    background_tasks.add_task(_record_audit, "create", expense["id"])
    return expense


@app.get("/expenses", tags=["expenses"], summary="List expenses with filters")
def list_expenses(
    category: str = Query(None, max_length=50, description="Filter by category"),
    min_amount: float = Query(None, ge=0, description="Minimum amount"),
    max_amount: float = Query(None, ge=0, description="Maximum amount"),
    from_date: str = Query(None, pattern=r"^\d{4}-\d{2}-\d{2}$", description="YYYY-MM-DD"),
    to_date: str = Query(None, pattern=r"^\d{4}-\d{2}-\d{2}$", description="YYYY-MM-DD"),
    limit: int = Query(50, ge=1, le=200),
    offset: int = Query(0, ge=0),
):
    clause, params = _where_clause(category, min_amount, max_amount, from_date, to_date)
    items = app.db.query(
        f"SELECT * FROM expenses{clause} ORDER BY spent_on DESC, id DESC LIMIT ? OFFSET ?",
        [*params, limit, offset],
    )
    total = app.db.query(f"SELECT COUNT(*) AS n FROM expenses{clause}", params)[0]["n"]
    return {"items": items, "total": total, "limit": limit, "offset": offset}


@app.get("/expenses/{expense_id}", tags=["expenses"], summary="Get one expense")
def get_expense(expense_id: int):
    return _fetch_or_404(expense_id)


@app.patch("/expenses/{expense_id}", tags=["expenses"], summary="Partially update an expense")
def update_expense(expense_id: int, update: ExpenseUpdate, background_tasks: BackgroundTasks):
    _fetch_or_404(expense_id)
    fields = update.model_dump(exclude_unset=True)
    if not fields:
        raise HTTPException(status_code=400, detail="no fields to update")
    assignments = ", ".join(f"{column} = ?" for column in fields)
    params = [_normalize(column, value) for column, value in fields.items()]
    app.db.execute(
        f"UPDATE expenses SET {assignments} WHERE id = ?",
        [*params, expense_id],
    )
    background_tasks.add_task(_record_audit, "update", expense_id)
    return _fetch_or_404(expense_id)


@app.delete("/expenses/{expense_id}", tags=["expenses"], summary="Delete an expense")
def delete_expense(expense_id: int, background_tasks: BackgroundTasks):
    _fetch_or_404(expense_id)
    app.db.execute("DELETE FROM expenses WHERE id = ?", [expense_id])
    background_tasks.add_task(_record_audit, "delete", expense_id)
    return {"status": 204}


# --------------------------------------------------------------------------
# Analytics
# --------------------------------------------------------------------------

@app.get("/analytics/summary", tags=["analytics"], summary="Totals and extremes")
def analytics_summary(category: str = Query(None, max_length=50)):
    clause, params = _where_clause(category, None, None, None, None)
    row = app.db.query(
        f"""SELECT COUNT(*) AS count,
                   COALESCE(SUM(amount), 0) AS total,
                   COALESCE(AVG(amount), 0) AS average,
                   COALESCE(MIN(amount), 0) AS smallest,
                   COALESCE(MAX(amount), 0) AS largest,
                   MIN(spent_on) AS first_day,
                   MAX(spent_on) AS last_day
            FROM expenses{clause}""",
        params,
    )[0]
    row["total"] = round(row["total"], 2)
    row["average"] = round(row["average"], 2)
    return row


@app.get("/analytics/by-category", tags=["analytics"], summary="Spend grouped by category")
def analytics_by_category():
    rows = app.db.query(
        """SELECT category,
                  COUNT(*) AS count,
                  SUM(amount) AS total,
                  AVG(amount) AS average
           FROM expenses
           GROUP BY category
           ORDER BY SUM(amount) DESC"""
    )
    for row in rows:
        row["total"] = round(row["total"], 2)
        row["average"] = round(row["average"], 2)
    grand_total = round(sum(row["total"] for row in rows), 2)
    for row in rows:
        row["share_pct"] = round(100.0 * row["total"] / grand_total, 1) if grand_total else 0.0
    return {"categories": rows, "grand_total": grand_total}


@app.get("/analytics/monthly", tags=["analytics"], summary="Monthly spend series")
def analytics_monthly():
    rows = app.db.query(
        """SELECT substr(spent_on, 1, 7) AS month,
                  COUNT(*) AS count,
                  SUM(amount) AS total
           FROM expenses
           GROUP BY month
           ORDER BY month"""
    )
    for row in rows:
        row["total"] = round(row["total"], 2)
    return {"months": rows}


@app.get("/analytics/trend", tags=["analytics"], summary="Daily spend for the last N days")
def analytics_trend(days: int = Query(30, ge=1, le=365)):
    if IS_POSTGRES:
        points = app.db.query(
            """SELECT spent_on AS day,
                      SUM(amount) AS total,
                      COUNT(*) AS count
               FROM expenses
               WHERE spent_on >= (CURRENT_DATE - (? * INTERVAL '1 day'))::text
               GROUP BY day
               ORDER BY day""",
            [days - 1],
        )
    else:
        points = app.db.query(
            """SELECT spent_on AS day,
                      SUM(amount) AS total,
                      COUNT(*) AS count
               FROM expenses
               WHERE spent_on >= date('now', ?)
               GROUP BY day
               ORDER BY day""",
            [f"-{days - 1} days"],
        )
    for point in points:
        point["total"] = round(point["total"], 2)
    return {"days": days, "points": points}


@app.get("/analytics/top", tags=["analytics"], summary="Top N expenses by amount")
def analytics_top(n: int = Query(5, ge=1, le=20)):
    rows = app.db.query(
        "SELECT * FROM expenses ORDER BY amount DESC, id ASC LIMIT ?", [n]
    )
    return {"top": rows}


# --------------------------------------------------------------------------
# Live feed (Server-Sent Events)
# --------------------------------------------------------------------------

@app.sse("/stream/summary", tags=["analytics"], summary="Live summary snapshots (SSE)")
async def summary_stream():
    """Push a fresh summary snapshot five times, then close."""
    for _ in range(5):
        row = app.db.query(
            "SELECT COUNT(*) AS count, COALESCE(SUM(amount), 0) AS total FROM expenses"
        )[0]
        row["total"] = round(row["total"], 2)
        yield f"data: {json.dumps(row)}\n\n"
        await asyncio.sleep(0.5)


if __name__ == "__main__":
    app.run(f"127.0.0.1:{os.environ.get('PORT', '8000')}")
