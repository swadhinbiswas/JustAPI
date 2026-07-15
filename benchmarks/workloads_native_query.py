"""Benchmark: native query-param fast path (GET + query_schema + native=True).

Validates the query string in Rust and echoes parsed params as JSON, with no
GIL/Python hop. Mirrors the body fast path but for GET routes.
"""
from justapi import JustAPIApp

app = JustAPIApp()

QUERY_SCHEMA = {
    "type": "object",
    "properties": {
        "name": {"type": "string"},
        "age": {"type": "integer", "minimum": 0},
    },
    "required": ["name"],
}


@app.get("/search", native=True, query_schema=QUERY_SCHEMA)
def search():
    # Never runs on the fast path (request is validated + echoed in Rust).
    return {"echo": True}


@app.get("/search_gil")
def search_gil(name: str = "", age: int = 0):
    return {"name": name, "age": age}


if __name__ == "__main__":
    app.run("127.0.0.1:8263")
