"""Real DB-backed CRUD benchmark server — Robyn.

Apples-to-apples against crud_justapi.py (python mode): a Python handler that
issues a single-row INSERT/SELECT/UPDATE/DELETE against a SQLite file with WAL
and a 10-connection pool. Robyn serves sync handlers; we use a small pooled
sqlite3 connection per request (Robyn is single-process threaded, so a module
level connection is acceptable for the benchmark).

Run:
    python benchmarks/crud_robyn.py 8083
"""
import os
import sqlite3
import sys

from robyn import Robyn, jsonify

PORT = sys.argv[1] if len(sys.argv) > 1 else "8083"

HERE = os.path.dirname(os.path.abspath(__file__))
DB_PATH = os.path.join(HERE, "crud_bench.sqlite")
if os.path.exists(DB_PATH):
    for ext in ("", "-wal", "-shm"):
        try:
            os.remove(DB_PATH + ext)
        except FileNotFoundError:
            pass

conn = sqlite3.connect(DB_PATH, check_same_thread=False)
conn.execute("PRAGMA journal_mode=WAL")
conn.execute("PRAGMA busy_timeout=5000")
conn.execute("CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, qty INTEGER NOT NULL)")
conn.execute("INSERT INTO items(name, qty) VALUES (?, ?)", ("seed", 1))
conn.commit()

app = Robyn(__file__)


@app.post("/items")
def create(request):
    body = request.json()
    conn.execute("INSERT INTO items(name, qty) VALUES (?, ?)", (body["name"], body["qty"]))
    conn.commit()
    return jsonify({"ok": True})


@app.get("/items/:id")
def read(request, id):
    row = conn.execute("SELECT * FROM items WHERE id = ?", (int(id),)).fetchone()
    if row is None:
        return jsonify({"error": "not found"})
    return jsonify({"id": row[0], "name": row[1], "qty": row[2]})


@app.put("/items/:id")
def update(request, id):
    body = request.json()
    conn.execute("UPDATE items SET name=?, qty=? WHERE id=?", (body["name"], body["qty"], int(id)))
    conn.commit()
    return jsonify({"ok": True})


@app.delete("/items/:id")
def delete(request, id):
    conn.execute("DELETE FROM items WHERE id = ?", (int(id),))
    conn.commit()
    return jsonify({"ok": True})


if __name__ == "__main__":
    app.start(host="127.0.0.1", port=int(PORT))
