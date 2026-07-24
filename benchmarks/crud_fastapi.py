"""Real DB-backed CRUD benchmark server — FastAPI + SQLAlchemy (async).

Apples-to-apples against crud_justapi.py (python mode): a Python handler that
issues a single-row INSERT/SELECT/UPDATE/DELETE against a SQLite file with WAL
and a 10-connection pool.

Run:
    python benchmarks/crud_fastapi.py 8082
"""
import os
import sys

from fastapi import FastAPI, Request
from sqlalchemy import Column, Integer, String, create_engine, text
from sqlalchemy.ext.asyncio import create_async_engine, async_sessionmaker
from sqlalchemy.orm import declarative_base, sessionmaker

PORT = sys.argv[1] if len(sys.argv) > 1 else "8082"

HERE = os.path.dirname(os.path.abspath(__file__))
DB_PATH = os.path.join(HERE, "crud_bench.sqlite")
if os.path.exists(DB_PATH):
    for ext in ("", "-wal", "-shm"):
        try:
            os.remove(DB_PATH + ext)
        except FileNotFoundError:
            pass

# sqlite+aiosqlite for async; WAL + busy_timeout via connect args.
DATABASE_URL = f"sqlite+aiosqlite:///{DB_PATH}"
engine = create_async_engine(
    DATABASE_URL,
    pool_size=10,
    max_overflow=0,
    connect_args={"check_same_thread": False},
)
SessionLocal = async_sessionmaker(engine, expire_on_commit=False)

Base = declarative_base()


class Item(Base):
    __tablename__ = "items"
    id = Column(Integer, primary_key=True, autoincrement=True)
    name = Column(String, nullable=False)
    qty = Column(Integer, nullable=False)


app = FastAPI()


@app.on_event("startup")
async def on_startup():
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.drop_all)
        await conn.run_sync(Base.metadata.create_all)
        await conn.execute(text("PRAGMA journal_mode=WAL"))
        await conn.execute(text("PRAGMA busy_timeout=5000"))
        await conn.execute(text("INSERT INTO items(name, qty) VALUES (:n, :q)"), {"n": "seed", "q": 1})


@app.post("/items")
async def create(request: Request):
    body = await request.json()
    async with SessionLocal() as s:
        await s.execute(text("INSERT INTO items(name, qty) VALUES (:n, :q)"), {"n": body["name"], "q": body["qty"]})
        await s.commit()
    return {"ok": True}


@app.get("/items/{id}")
async def read(id: int):
    async with SessionLocal() as s:
        row = (await s.execute(text("SELECT * FROM items WHERE id = :id"), {"id": id})).mappings().first()
    if row is None:
        return {"error": "not found"}, 404
    return dict(row)


@app.put("/items/{id}")
async def update(id: int, request: Request):
    body = await request.json()
    async with SessionLocal() as s:
        await s.execute(
            text("UPDATE items SET name=:n, qty=:q WHERE id=:id"),
            {"n": body["name"], "q": body["qty"], "id": id},
        )
        await s.commit()
    return {"ok": True}


@app.delete("/items/{id}")
async def delete(id: int):
    async with SessionLocal() as s:
        await s.execute(text("DELETE FROM items WHERE id = :id"), {"id": id})
        await s.commit()
    return {"ok": True}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="127.0.0.1", port=int(PORT), workers=1)
