import pytest
import json
from justapi import JustAPIApp, Depends
from justapi.testing import AsyncTestClient

def common_parameters(q: str | None = None, skip: int = 0, limit: int = 100):
    return {"q": q, "skip": skip, "limit": limit}

async def verify_token(token: str = None):
    if token != "supersecret":
        raise ValueError("Invalid token")
    return {"user": "admin"}

@pytest.mark.asyncio
async def test_dependency_injection():
    app = JustAPIApp()

    # Test extracting path and query params, and injecting dependencies
    async def read_items(
        item_id: int, 
        commons: dict = Depends(common_parameters), 
        user: dict = Depends(verify_token)
    ):
        return {
            "item_id": item_id,
            "commons": commons,
            "user": user
        }

    app.get("/items/{item_id}", read_items)

    async with AsyncTestClient(app) as client:
        # Test valid request
        resp = await client.get("/items/42?q=foo&skip=10&token=supersecret")
        assert resp["status"] == 200, f"Expected 200, got {resp['status']}. Body: {resp.get('body', b'').decode()}"
        data = json.loads(resp["body"].decode())
        assert data["item_id"] == 42
        assert data["commons"] == {"q": "foo", "skip": 10, "limit": 100}
        assert data["user"] == {"user": "admin"}

        # Test invalid token (dependency throws)
        # Note: JustAPI by default catches exceptions in handlers and returns 500,
        # unless custom exception handlers are added. We can assert it fails.
        resp = await client.get("/items/42?q=foo&skip=10&token=bad")
        assert resp["status"] == 500


@pytest.mark.asyncio
async def test_async_callable_instance_dependency():
    """Regression: a `Depends`/`Security` on a callable INSTANCE whose
    `__call__` is async (e.g. `OAuth2PasswordBearer(...)`) must be awaited.
    `inspect.iscoroutinefunction(instance)` is False, so the old code invoked
    the dependency and dropped the unawaited coroutine (RuntimeWarning), and
    the injected value was the coroutine, not its result.
    """
    from justapi import Security
    from justapi.auth import OAuth2PasswordBearer

    app = JustAPIApp()
    oauth = OAuth2PasswordBearer(tokenUrl="token")

    @app.get("/users/me")
    async def read_me(token: str = Security(oauth)):
        return {"token": token}

    async with AsyncTestClient(app) as client:
        # No auth header -> 401 (dependency raises HTTPException)
        resp = await client.get("/users/me")
        assert resp["status"] == 401, f"expected 401, got {resp['status']}"


def test_gil_pool_survives_fork():
    """Regression: a forked child must rebuild the GIL pool (its worker
    threads do not survive fork(2)). Before the fix, a child that inherited
    an initialized pool would block forever on send -> 504 timeouts for every
    Python-handler request.
    """
    import multiprocessing
    import os
    import tempfile
    import time
    import urllib.request
    from urllib.error import HTTPError

    # Warm the GIL pool in the parent first (via a test client), so the fork
    # reproduces the contamination.
    app = JustAPIApp()

    @app.get("/warm")
    def warm(req):
        return {"ok": True}

    from justapi import JustAPITestClient
    JustAPITestClient(app).get("/warm")

    port = 8099

    def run_server():
        app2 = JustAPIApp()

        @app2.get("/ping")
        def ping(req):
            return {"ok": True}

        app2.run(f"127.0.0.1:{port}")

    p = multiprocessing.Process(target=run_server)
    p.start()
    try:
        deadline = time.time() + 15
        status = None
        while time.time() < deadline:
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/ping", timeout=2) as r:
                    status = r.status
                    break
            except Exception:
                time.sleep(0.3)
        assert status == 200, f"forked child GIL pool hung: got {status}"
    finally:
        p.terminate()
        p.join()
