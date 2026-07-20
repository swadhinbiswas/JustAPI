"""shop.db — thin SQL helpers over the justapi Database pool.

Wraps `app.db.query`/`execute` and adds pagination + a token generator. All
queries use bound parameters (injection-safe).
"""
from __future__ import annotations

import secrets
from typing import Any, Optional

from .config import db


def q(sql: str, params: list | None = None) -> list[dict]:
    rows = db().query(sql, params or [])
    return [dict(r) for r in rows]


def q1(sql: str, params: list | None = None) -> Optional[dict]:
    rows = q(sql, params)
    return rows[0] if rows else None


def _page(qp: dict) -> tuple[int, int]:
    try:
        page = max(1, int(qp.get("page", 1)))
    except (TypeError, ValueError):
        page = 1
    try:
        size = max(1, min(200, int(qp.get("size", 20))))
    except (TypeError, ValueError):
        size = 20
    return page, size


def paginate(base_sql: str, count_sql: str, params: list, qp: dict) -> dict:
    page, size = _page(qp)
    total = q1(count_sql, params)
    total_n = int(total["c"]) if total and "c" in total else 0
    offset = (page - 1) * size
    rows = q(base_sql + f" LIMIT {size} OFFSET {offset}", params)
    return {
        "page": page,
        "size": size,
        "total": total_n,
        "pages": (total_n + size - 1) // size if total_n else 0,
        "items": rows,
    }


def new_token(prefix: str) -> str:
    return f"{prefix}_{secrets.token_hex(12)}"
