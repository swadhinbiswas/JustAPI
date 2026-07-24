"""Step D CRUD benchmark against Postgres (Rust-native path, no GIL)."""
import sys
from justapi import JustAPIApp, Database

PORT = sys.argv[1] if len(sys.argv) > 1 else "8099"
# Pass via CLI arg or JUSTAPI_PG_URL env var — never commit live credentials
import os
PGURL = sys.argv[2] if len(sys.argv) > 2 else os.environ.get("JUSTAPI_PG_URL", "")

INIT = (
    "DROP TABLE IF EXISTS justapi_bench_items;"
    "CREATE TABLE justapi_bench_items ("
    "id BIGSERIAL PRIMARY KEY, name TEXT NOT NULL, qty INTEGER NOT NULL);"
)

app = JustAPIApp()
app.set_database(
    Database(PGURL, max_connections=20, init_sql=INIT),
)
COLUMNS = ["name", "qty"]
app.post("/items", crud_table="justapi_bench_items", crud_columns=COLUMNS)
app.get("/items/{id}", crud_table="justapi_bench_items", crud_columns=COLUMNS)
app.put("/items/{id}", crud_table="justapi_bench_items", crud_columns=COLUMNS)
app.delete("/items/{id}", crud_table="justapi_bench_items", crud_columns=COLUMNS)

if __name__ == "__main__":
    app.run(f"127.0.0.1:{PORT}")
