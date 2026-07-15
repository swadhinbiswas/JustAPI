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


