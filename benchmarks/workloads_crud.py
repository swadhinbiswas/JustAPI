"""Step C CRUD benchmark: Rust-native INSERT/SELECT/UPDATE/DELETE vs Python.

Run the Rust-native version with the route methods carrying crud_table /
crud_columns (served by crud_dispatch_bytes), and the Python version with
explicit handlers using the db_pool. Both target the same SQLite table.

Usage (Rust-native):
    python benchmarks/workloads_crud.py native 8080
Usage (Python):
    python benchmarks/workloads_crud.py python 8080
"""
import sys
import json

from justapi import JustAPIApp, Database

MODE = sys.argv[1] if len(sys.argv) > 1 else "native"
PORT = sys.argv[2] if len(sys.argv) > 2 else "8080"

DB_URL = "sqlite://:memory:"
INIT_SQL = (
    "CREATE TABLE items ("
    "id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, qty INTEGER NOT NULL)"
)

app = JustAPIApp()
app.set_database(Database(DB_URL, max_connections=1), init_sql=INIT_SQL)

COLUMNS = ["name", "qty"]


if MODE != "native":
    raise SystemExit("only 'native' mode is supported for Step C (no Python DB-exec API yet)")


# Rust-native: op inferred from HTTP method, served entirely in Rust.
app.post("/items", crud_table="items", crud_columns=COLUMNS)
app.get("/items/{id}", crud_table="items", crud_columns=COLUMNS)
app.put("/items/{id}", crud_table="items", crud_columns=COLUMNS)
app.delete("/items/{id}", crud_table="items", crud_columns=COLUMNS)


if __name__ == "__main__":
    app.run(f"127.0.0.1:{PORT}")
