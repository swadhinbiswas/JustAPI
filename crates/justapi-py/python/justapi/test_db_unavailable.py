"""Database outages must be retryable (503) — not 500 — and catchable.

The pool bounds connection acquisition with ``request_acquire_timeout``
(3 s by default). When the database is unreachable, ``app.db.*`` raises
``DatabaseUnavailable``; the request layer maps it to
``503 Service Unavailable`` + ``Retry-After`` so load balancers and clients
can back off instead of treating it as a code defect.
"""

from __future__ import annotations

import pytest

from justapi import DatabaseUnavailable, JustAPIApp, JustAPITestClient

# Port 1 is never listening: connection refused, no external dependency.
UNREACHABLE = "postgres://nobody:wrong@127.0.0.1:1/nope"


def _app_with_unreachable_db() -> JustAPIApp:
    app = JustAPIApp()
    app.set_database(UNREACHABLE, request_acquire_timeout=0.2, acquire_timeout=0.2)
    return app


def _header(resp, name: str):
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


def test_db_outage_returns_503_with_retry_after():
    app = _app_with_unreachable_db()

    @app.get("/needs-db")
    def needs_db():
        return {"rows": app.db.query("SELECT 1")}

    client = JustAPITestClient(app)
    resp = client.get("/needs-db")

    assert resp["status"] == 503, bytes(resp["body"])
    assert bytes(resp["body"]) == b'{"detail":"database unavailable"}'
    assert _header(resp, "retry-after") == "5"
    assert _header(resp, "content-type") == "application/json"


def test_database_unavailable_is_catchable_for_graceful_degradation():
    app = _app_with_unreachable_db()

    @app.get("/degrade")
    def degrade():
        try:
            return {"rows": app.db.query("SELECT 1")}
        except DatabaseUnavailable:
            return {"cached": True}

    client = JustAPITestClient(app)
    resp = client.get("/degrade")
    assert resp["status"] == 200
    assert bytes(resp["body"]) == b'{"cached": true}'


def test_no_database_configured_still_returns_none():
    app = JustAPIApp()
    assert app.db is None


def test_health_check_marks_unreachable_db_unhealthy():
    app = _app_with_unreachable_db()
    app.register_health_check("database", lambda: len(app.db.query("SELECT 1")) == 1)

    client = JustAPITestClient(app)
    resp = client.get("/ready")
    assert resp["status"] == 503
