import os
import sys
import time
import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "crates", "justapi-py"))
from justapi import JustAPIApp, Database  # noqa: E402

PG_URL = (
    "postgres://avnadmin:<SECRET_PASSWORD>"
    "@dexmorgan-examhallorhell-5f8c.d.aivencloud.com:11100/defaultdb?sslmode=require"
)

import threading

results = {}


def make_app():
    app = JustAPIApp()
    app.set_database(Database(PG_URL), init_sql="DROP TABLE IF EXISTS dbpool_demo; CREATE TABLE dbpool_demo (id SERIAL PRIMARY KEY, name TEXT, qty INT)")
    return app


def test_dbpool_roundtrip():
    app = make_app()

    @app.get("/seed")
    def seed(req):
        db = app.db
        db.execute("INSERT INTO dbpool_demo (name, qty) VALUES ($1, $2)", ["alpha", 10])
        db.execute("INSERT INTO dbpool_demo (name, qty) VALUES ($1, $2)", ["beta", 3])
        rows = db.query("SELECT * FROM dbpool_demo ORDER BY id")
        assert isinstance(rows, list)
        return {"rows": rows}

    @app.get("/insert")
    def do_insert(req):
        db = app.db
        row = db.insert("dbpool_demo", {"name": "gamma", "qty": 7})
        return {"inserted": row}

    @app.get("/txn")
    def do_txn(req):
        db = app.db
        res = db.transaction([
            ("UPDATE dbpool_demo SET qty = qty + 1 WHERE name = $1", ["alpha"]),
            ("SELECT SUM(qty) AS total FROM dbpool_demo", None),
        ])
        return {"txn": res}

    @app.get("/dbhealth")
    def do_health(req):
        db = app.db
        ok = db.health()
        return {"ok": ok}

    @app.get("/stream")
    def do_stream(req):
        db = app.db
        chunks = db.query_stream("SELECT id FROM dbpool_demo ORDER BY id", None, 2)
        return {"chunks": len(chunks), "first": chunks[0] if chunks else []}

    @app.get("/blob")
    def do_blob(req):
        db = app.db
        from justapi import DbParam
        db.execute("CREATE TABLE IF NOT EXISTS blobs (id SERIAL PRIMARY KEY, data BYTEA)")
        db.execute("INSERT INTO blobs (data) VALUES (?)", [DbParam.bytes(b"\x01\x02\x03")])
        row = db.query("SELECT data FROM blobs ORDER BY id DESC LIMIT 1")
        return {"data": row[0]["data"]}

    import threading

    srv = threading.Thread(target=lambda: app.run("127.0.0.1:8123"), daemon=True)
    srv.start()
    time.sleep(2.0)

    import urllib.request

    def get(path):
        with urllib.request.urlopen("http://127.0.0.1:8123" + path, timeout=10) as r:
            return r.read().decode()

    assert "rows" in get("/seed")
    ins = get("/insert")
    assert "gamma" in ins
    txn = get("/txn")
    assert "total" in txn
    assert get("/dbhealth")
    st = get("/stream")
    assert "chunks" in st
    bl = get("/blob")
    assert "data" in bl
    print("DBPOOL SMOKE OK", get("/seed"))


if __name__ == "__main__":
    test_dbpool_roundtrip()
