"""Server-path concurrent-write durability test (P2.2 regression guard).

Uses the async test client with many concurrent coroutines issuing INSERTs,
then asserts every row persisted. This exercises the same `app.db.query`
write path a real server handler uses, under genuine concurrency — without
the single-worker GIL-pool TCP stall that a raw `oha -c 50` against the
blocking server would hit.
"""
import os
import tempfile

import pytest

from justapi import JustAPIApp
from justapi.testing import AsyncTestClient


@pytest.mark.asyncio
async def test_concurrent_writes_via_test_client():
    tmpdir = tempfile.mkdtemp()
    db_path = os.path.join(tmpdir, "svc_concurrent.db")
    open(db_path, "w").close()
    try:
        app = JustAPIApp()
        app.set_database(f"sqlite://{db_path}")

        @app.post("/items")
        async def create(request):
            body = request.json()
            # Fire-and-forget style: the write must persist under concurrency.
            app.db.query(
                "INSERT INTO items(name, qty) VALUES (?, ?)",
                [body["name"], body["qty"]],
            )
            return {"ok": True}

        @app.get("/items/{id}")
        async def read(request):
            row = app.db.query(
                "SELECT * FROM items WHERE id = ?",
                [int(request["path_params"]["id"])],
            )
            return row[0] if row else {"status": 404}

        async with AsyncTestClient(app, database=f"sqlite://{db_path}") as c:
            # Create the table on the same file pool the handlers use.
            app.db.query(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, qty INTEGER)"
            )
            import asyncio

            async def insert_one(v):
                return await c.post(
                    "/items", f'{{"name":"w{v}","qty":{v}}}'.encode()
                )

            results = await asyncio.gather(
                *(insert_one(v) for v in range(100)),
                return_exceptions=True,
            )
            errors = [r for r in results if isinstance(r, Exception)]
            assert not errors, f"{len(errors)} inserts errored: {errors[:3]}"

            # Count persisted rows directly.
            rows = app.db.query("SELECT COUNT(*) AS c FROM items")
            # 100 concurrent inserts, all must persist (P2.2 regression:
            # old py.detach+block_on path silently dropped ~49/50 writes).
            assert rows[0]["c"] == 100, f"expected 100 rows, got {rows[0]['c']}"
    finally:
        if os.path.exists(db_path):
            os.unlink(db_path)
        os.rmdir(tmpdir)
