"""Integration tests for database test helpers — in-memory SQLite, transactions.
"""
import json
import pytest
import pytest_asyncio
from justapi import JustAPIApp
from justapi.testing import (
    AsyncTestClient,
    ManagedDb,
    db_client,
    assert_ok,
    assert_status,
    assert_json,
)


# ---------------------------------------------------------------------------
# In-memory SQLite via test client
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_test_client_with_database():
    """Use the test client with an in-memory SQLite database.

    The transaction middleware auto-begins a transaction for POST,
    and commits on success (2xx).
    """
    app = JustAPIApp()
    app.set_database("sqlite://:memory:")

    async def create_user(request):
        return {"status": 201, "body": b'{"id":1}'}

    async def get_users(request):
        return {"body": b'[]'}

    app.post("/users", create_user)
    app.get("/users", get_users)

    async with AsyncTestClient(app, database="sqlite://:memory:") as c:
        resp = await c.post("/users", b'{"name":"Alice"}')
        assert resp["status"] == 201

        resp = await c.get("/users")
        assert_ok(resp)


# ---------------------------------------------------------------------------
# Transaction middleware runs (auto-begin for POST)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_transaction_middleware_runs():
    """Transaction middleware auto-begins a transaction for POST and commits on 2xx."""
    import tempfile, os

    # Use an absolute path in a temp directory
    tmpdir = tempfile.mkdtemp()
    db_path = os.path.join(tmpdir, "test.db")
    # Create the empty database file so sqlx can open it
    open(db_path, "w").close()
    try:
        app = JustAPIApp()
        app.set_database(f"sqlite://{db_path}")

        async def create_item(request):
            return {"status": 201, "body": b'{"id":1}'}

        async def fail_handler(request):
            return {"status": 400, "body": b'{"error":"bad"}'}

        app.post("/items", create_item)
        app.post("/fail", fail_handler)

        async with AsyncTestClient(app, database=f"sqlite://{db_path}") as c:
            resp = await c.post("/items", b'{"name":"Alice"}')
            assert resp["status"] == 201

            resp = await c.post("/fail", b'{}')
            assert resp["status"] == 400

        # Verify the file was created (pool was initialized)
        assert os.path.exists(db_path)
    finally:
        if os.path.exists(db_path):
            os.unlink(db_path)
        os.rmdir(tmpdir)


# ---------------------------------------------------------------------------
# TestDb context manager
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_test_db_default_url():
    db = ManagedDb()
    async with db:
        assert db.url == "sqlite://:memory:"


@pytest.mark.asyncio
async def test_test_db_custom_url():
    async with ManagedDb(url="sqlite://:memory:") as db:
        assert db.url == "sqlite://:memory:"


@pytest.mark.asyncio
async def test_test_db_run_sql():
    async with ManagedDb() as db:
        await db.run_sql(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);"
            "INSERT INTO items VALUES (1, 'test');"
        )
        assert db.url == "sqlite://:memory:"


# ---------------------------------------------------------------------------
# db_client helper
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_db_client_with_seed_sql():
    app = JustAPIApp()
    app.set_database("sqlite://:memory:")

    async def list_items(request):
        return {"body": b'[{"id":1,"name":"seed"}]'}

    app.get("/items", list_items)

    c = await db_client(
        app,
        seed_sql="CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);"
    )
    try:
        resp = await c.get("/items")
        assert_ok(resp)
    finally:
        await c._destroy_client()


# ---------------------------------------------------------------------------
# Existing test client still works without database
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_async_client_no_db():
    app = JustAPIApp()
    app.get("/ping", lambda r: {"pong": True})

    async with AsyncTestClient(app) as c:
        resp = await c.get("/ping")
        assert_ok(resp)
        assert_json(resp, {"pong": True})
