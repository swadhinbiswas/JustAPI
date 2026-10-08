"""Middleware ordering must match Starlette/FastAPI (last added = outermost).

Starlette empirically (0.52.x): add A then B -> request runs B, A, handler,
A, B. JustAPI followed the opposite order before Phase 53; this test locks the
parity behaviour.
"""
import pytest
from justapi import JustAPIApp
from justapi.testing import AsyncTestClient


@pytest.mark.asyncio
async def test_middleware_order_last_added_is_outermost():
    app = JustAPIApp()
    order = []

    @app.middleware("http")
    async def mw_a(request, call_next):
        order.append("A-req")
        resp = await call_next(request)
        order.append("A-resp")
        return resp

    @app.middleware("http")
    async def mw_b(request, call_next):
        order.append("B-req")
        resp = await call_next(request)
        order.append("B-resp")
        return resp

    @app.get("/x")
    def x():
        order.append("handler")
        return {"ok": True}

    async with AsyncTestClient(app) as c:
        resp = await c.get("/x")
        assert resp["status"] == 200

    assert order == ["B-req", "A-req", "handler", "A-resp", "B-resp"]


@pytest.mark.asyncio
async def test_route_middleware_wraps_app_middleware():
    """Route-level middleware is appended last, so it is outermost of all."""
    app = JustAPIApp()
    order = []

    @app.middleware("http")
    async def app_mw(request, call_next):
        order.append("app-req")
        resp = await call_next(request)
        order.append("app-resp")
        return resp

    async def route_mw(request, call_next):
        order.append("route-req")
        resp = await call_next(request)
        order.append("route-resp")
        return resp

    @app.get("/y", middlewares=[route_mw])
    def y():
        order.append("handler")
        return {"ok": True}

    async with AsyncTestClient(app) as c:
        resp = await c.get("/y")
        assert resp["status"] == 200

    assert order == ["route-req", "app-req", "handler", "app-resp", "route-resp"]
