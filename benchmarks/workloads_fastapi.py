"""FastAPI benchmark workload for Uvicorn+FastAPI baseline.

Usage:
    uvicorn benchmarks.workloads_fastapi:app --host 127.0.0.1 --port 8080 --workers 4

This is the "Uvicorn+FastAPI" baseline from PROMPT.md Section 10.2.
For raw ASGI baseline (no framework overhead), use workloads.py instead.
"""

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

app = FastAPI()


@app.get("/hello")
async def hello():
    return {"message": "hello, world"}


@app.post("/echo")
async def echo(request: Request):
    body = await request.json()
    return JSONResponse(content=body)
