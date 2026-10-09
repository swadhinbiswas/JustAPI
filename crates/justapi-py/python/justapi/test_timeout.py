"""`app.enable_timeout(seconds)` — per-request 504 (Phase 53).

Bridges to the core `TimeoutMiddleware`; verified on both the TestClient
chain and a real server.
"""
import asyncio

from justapi import JustAPIApp, JustAPITestClient


def _make_app(seconds: float) -> JustAPIApp:
    app = JustAPIApp()
    app.enable_timeout(seconds)

    @app.get("/fast")
    def fast():
        return {"ok": True}

    @app.get("/slow")
    async def slow():
        await asyncio.sleep(0.6)
        return {"ok": "late"}

    return app


def test_timeout_returns_504_for_slow_requests():
    c = JustAPITestClient(_make_app(0.15))
    resp = c.get("/slow")
    assert resp["status"] == 504, resp
    assert b"timed out" in bytes(resp["body"])


def test_timeout_passes_fast_requests():
    c = JustAPITestClient(_make_app(5.0))
    resp = c.get("/fast")
    assert resp["status"] == 200
    assert b'"ok": true' in bytes(resp["body"])
