import pytest
import time
from justapi import JustAPIApp, JustAPITestClient, BackgroundTasks

def test_background_tasks():
    app = JustAPIApp()
    
    result = []
    def bg_task(msg):
        time.sleep(0.1)
        result.append(msg)
        
    @app.get("/task")
    def add_task(background_tasks: BackgroundTasks):
        background_tasks.add_task(bg_task, "hello from bg")
        return {"status": "ok"}
        
    client = JustAPITestClient(app)
    response = client.get("/task")
    assert response["status"] == 200
    assert len(result) == 0 # Hasn't finished yet or ran in background
    
    # wait for background task
    time.sleep(0.3)
    assert len(result) == 1
    assert result[0] == "hello from bg"


def test_response_background():
    """Response(background=...) must run after the response is returned."""
    from justapi import JustAPIApp, JustAPITestClient, JSONResponse, BackgroundTasks

    done = []

    def bg(msg):
        time.sleep(0.1)
        done.append(msg)

    app = JustAPIApp()

    @app.get("/r")
    def handler():
        bt = BackgroundTasks()
        bt.add_task(bg, "via-response")
        return JSONResponse({"ok": True}, background=bt)

    client = JustAPITestClient(app)
    resp = client.get("/r")
    assert resp["status"] == 200
    assert b"true" in bytes(resp["body"])
    # background runs after the response
    assert len(done) == 0
    time.sleep(0.3)
    assert done == ["via-response"]


def test_async_background_task():
    import asyncio

    ran = []

    def bg():
        async def coro():
            await asyncio.sleep(0.05)
            ran.append("async")

        return coro()

    app = JustAPIApp()

    @app.get("/a")
    def handler(background_tasks: BackgroundTasks):
        background_tasks.add_task(bg)
        return {"ok": True}

    client = JustAPITestClient(app)
    resp = client.get("/a")
    assert resp["status"] == 200
    time.sleep(0.3)
    assert ran == ["async"]


def test_background_stats():
    import justapi

    stats = justapi.BackgroundTasks.stats()
    assert set(stats.keys()) == {
        "submitted",
        "active",
        "completed",
        "failed",
        "dropped",
        "async",
    }
    assert isinstance(stats["completed"], int)




@pytest.mark.asyncio
async def test_async_handler_interleaves_concurrent_requests():
    """Regression: async handlers must not serialize through the GIL worker.

    Before the fix, the GIL worker blocked on `future.result()`, so N
    concurrent async requests each waiting `await asyncio.sleep(x)` completed
    one-at-a-time (1ms-sleep handler ~= 800 RPS ceiling). Now the future is
    awaited on a spawn_blocking thread while the worker dispatches others.
    """
    import asyncio
    import time
    from justapi import JustAPIApp
    from justapi.testing import AsyncTestClient

    app = JustAPIApp()

    @app.get("/slow")
    async def slow(req):
        await asyncio.sleep(0.05)
        return {"ok": True}

    async with AsyncTestClient(app) as client:
        start = time.monotonic()
        results = await asyncio.gather(
            *(client.get("/slow") for _ in range(20)), return_exceptions=True
        )
        elapsed = time.monotonic() - start

        errors = [r for r in results if isinstance(r, Exception) or r.get("status") != 200]
        assert not errors, f"{len(errors)} requests failed: {errors[:2]}"
        # 20 requests × 50ms serialized would take >= 1.0s. Interleaved, they
        # should finish well under that (allow generous CI margin).
        assert elapsed < 0.8, f"async handlers serialized: 20×50ms took {elapsed:.2f}s"


@pytest.mark.asyncio
async def test_async_callback_path_concurrent_correctness():
    """Stress the callback-driven async resolution: many concurrent async
    handlers (mix of success and exception) must all complete correctly.
    Guards the `_DoneNotifier` add_done_callback path (ADR-086)."""
    import asyncio
    from justapi import JustAPIApp, HTTPException
    from justapi.testing import AsyncTestClient

    app = JustAPIApp()

    @app.get("/ok")
    async def ok(req):
        await asyncio.sleep(0.01)
        return {"ok": True}

    @app.get("/err")
    async def err(req):
        await asyncio.sleep(0.01)
        raise HTTPException(status_code=404, detail="gone")

    async with AsyncTestClient(app) as client:
        results = await asyncio.gather(
            *(client.get("/ok") for _ in range(30)),
            *(client.get("/err") for _ in range(30)),
            return_exceptions=True,
        )
        oks = [r for r in results if isinstance(r, dict) and r.get("status") == 200]
        errs = [r for r in results if isinstance(r, dict) and r.get("status") == 404]
        assert len(oks) == 30, f"expected 30 success, got {len(oks)}"
        assert len(errs) == 30, f"expected 30 errors, got {len(errs)}"
