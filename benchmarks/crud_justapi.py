"""Real DB-backed CRUD benchmark server — JustAPI (Python-handler path).

This is the apples-to-apples comparison against FastAPI+SQLAlchemy and Robyn:
the handler is plain Python that calls `app.db.query(...)` (sqlx-backed pool),
exactly like a typical FastAPI route calls the SQLAlchemy session.

Run:
    python benchmarks/crud_justapi.py python 8081
    python benchmarks/crud_justapi.py native 8081   # Rust-native fast path

DB: SQLite file, WAL, 10-connection pool (matches the baseline fixtures).
"""
import os
import sqlite3
import sys

from justapi import JustAPIApp

MODE = sys.argv[1] if len(sys.argv) > 1 else "python"
PORT = sys.argv[2] if len(sys.argv) > 2 else "8081"

HERE = os.path.dirname(os.path.abspath(__file__))
DB_PATH = os.path.join(HERE, "crud_bench.sqlite")
for ext in ("", "-wal", "-shm"):
    try:
        os.remove(DB_PATH + ext)
    except FileNotFoundError:
        pass

# Create + seed the SQLite file with raw sqlite3 (exactly like demo_shop's
# db_setup.py), then let JustAPI connect via a plain URL string — the proven
# working path (unconditional WAL + busy_timeout pragmas are applied in Rust).
con = sqlite3.connect(DB_PATH)
con.execute("PRAGMA journal_mode=WAL")
con.execute("PRAGMA busy_timeout=5000")
con.execute(
    "CREATE TABLE items ("
    "id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, qty INTEGER NOT NULL)"
)
con.execute("INSERT INTO items(name, qty) VALUES (?, ?)", ("seed", 1))
con.commit()
con.close()

app = JustAPIApp()
app.set_database(f"sqlite://{DB_PATH}")


if MODE == "native":
    # Rust-native CRUD: op inferred from HTTP method, served entirely in Rust.
    app.post("/items", crud_table="items", crud_columns=["name", "qty"])
    app.get("/items/{id}", crud_table="items", crud_columns=["name", "qty"])
    app.put("/items/{id}", crud_table="items", crud_columns=["name", "qty"])
    app.delete("/items/{id}", crud_table="items", crud_columns=["name", "qty"])
else:
    @app.post("/items")
    def create(request):
        body = request.json()
        app.db.query("INSERT INTO items(name, qty) VALUES (?, ?)", [body["name"], body["qty"]])
        return {"ok": True}

    @app.get("/items/{id}")
    def read(request):
        row = app.db.query("SELECT * FROM items WHERE id = ?", [int(request["path_params"]["id"])])
        if not row:
            return {"status": 404, "body": '{"error":"not found"}'}
        return row[0]

    @app.put("/items/{id}")
    def update(request):
        body = request.json()
        app.db.query("UPDATE items SET name=?, qty=? WHERE id=?", [body["name"], body["qty"], int(request["path_params"]["id"])])
        return {"ok": True}

    @app.delete("/items/{id}")
    def delete(request):
        app.db.query("DELETE FROM items WHERE id = ?", [int(request["path_params"]["id"])])
        return {"ok": True}


if __name__ == "__main__":
    app.run(f"127.0.0.1:{PORT}")
