import pytest
import pytest_asyncio
from justapi import JustAPIApp, Depends
from justapi.testing import assert_ok, assert_json


# ---------------------------------------------------------------------------
# Async generator dependency
# ---------------------------------------------------------------------------

@pytest_asyncio.fixture
async def async_gen_app():
    app = JustAPIApp()
    cleanup_log = []
    setup_log = []

    async def get_db():
        setup_log.append("db_open")
        try:
            yield {"conn": "db_1"}
        finally:
            cleanup_log.append("db_close")

    @app.get("/users")
    async def get_users(db=Depends(get_db)):
        return {"db": db, "setup": list(setup_log), "cleanup": list(cleanup_log)}

    return app, cleanup_log, setup_log


@pytest.mark.asyncio
async def test_async_generator_dep(async_gen_app):
    app, cleanup_log, setup_log = async_gen_app
    from justapi.testing import AsyncTestClient

    async with AsyncTestClient(app) as c:
        r = await c.get("/users")
        assert_ok(r)
        assert_json(r, {"db": {"conn": "db_1"}, "setup": ["db_open"], "cleanup": []})

    assert "db_open" in setup_log
    assert "db_close" in cleanup_log


# ---------------------------------------------------------------------------
# Sync generator dependency
# ---------------------------------------------------------------------------

@pytest_asyncio.fixture
async def sync_gen_app():
    app = JustAPIApp()
    cleanup_log = []

    def get_session():
        sess = {"session_id": "abc"}
        try:
            yield sess
        finally:
            cleanup_log.append("session_closed")

    @app.get("/session")
    def get_session_route(session=Depends(get_session)):
        return {"session": session}

    return app, cleanup_log


@pytest.mark.asyncio
async def test_sync_generator_dep(sync_gen_app):
    app, cleanup_log = sync_gen_app
    from justapi.testing import AsyncTestClient

    async with AsyncTestClient(app) as c:
        r = await c.get("/session")
        assert_ok(r)
        assert_json(r, {"session": {"session_id": "abc"}})

    assert "session_closed" in cleanup_log


# ---------------------------------------------------------------------------
# Generator dependency with exception in handler — cleanup still runs
# ---------------------------------------------------------------------------

@pytest_asyncio.fixture
async def gen_dep_exc_app():
    app = JustAPIApp()
    cleanup_log = []

    async def get_db():
        try:
            yield {"conn": "db_1"}
        finally:
            cleanup_log.append("db_close")

    @app.get("/broken")
    async def broken(db=Depends(get_db)):
        raise RuntimeError("boom")

    return app, cleanup_log


@pytest.mark.asyncio
async def test_generator_dep_cleanup_on_error(gen_dep_exc_app):
    app, cleanup_log = gen_dep_exc_app
    from justapi.testing import AsyncTestClient

    async with AsyncTestClient(app) as c:
        r = await c.get("/broken")
        assert r["status"] == 500

    assert "db_close" in cleanup_log


# ---------------------------------------------------------------------------
# Nested generator dependencies
# ---------------------------------------------------------------------------

@pytest_asyncio.fixture
async def nested_gen_app():
    app = JustAPIApp()
    cleanup_log = []

    async def get_db():
        try:
            yield {"conn": "db_1"}
        finally:
            cleanup_log.append("db_close")

    async def get_repo(db=Depends(get_db)):
        try:
            yield {"repo": "users", "db": db}
        finally:
            cleanup_log.append("repo_close")

    @app.get("/nested")
    async def nested(repo=Depends(get_repo)):
        return {"repo": repo}

    return app, cleanup_log


@pytest.mark.asyncio
async def test_nested_generator_deps(nested_gen_app):
    app, cleanup_log = nested_gen_app
    from justapi.testing import AsyncTestClient

    async with AsyncTestClient(app) as c:
        r = await c.get("/nested")
        assert_ok(r)
        assert_json(r, {"repo": {"repo": "users", "db": {"conn": "db_1"}}})

    assert "db_close" in cleanup_log
    assert "repo_close" in cleanup_log
    assert cleanup_log.index("repo_close") < cleanup_log.index("db_close")


# ---------------------------------------------------------------------------
# Cached generator dependencies (use_cache=True)
# ---------------------------------------------------------------------------

@pytest_asyncio.fixture
async def cached_gen_app():
    app = JustAPIApp()
    call_count = 0
    cleanup_log = []

    async def get_config():
        nonlocal call_count
        call_count += 1
        try:
            yield {"mode": "test"}
        finally:
            cleanup_log.append("config_close")

    @app.get("/cached")
    async def cached(c1=Depends(get_config, use_cache=True), c2=Depends(get_config, use_cache=True)):
        return {"c1": c1, "c2": c2, "call_count": call_count}

    return app, call_count, cleanup_log


@pytest.mark.asyncio
async def test_cached_generator_dep(cached_gen_app):
    app, call_count, cleanup_log = cached_gen_app
    from justapi.testing import AsyncTestClient

    async with AsyncTestClient(app) as c:
        r = await c.get("/cached")
        assert_ok(r)
        body = r["body"]
        import json
        data = json.loads(bytes(body))
        assert data["c1"]["mode"] == "test"
        assert data["c2"]["mode"] == "test"

    assert "config_close" in cleanup_log


# ---------------------------------------------------------------------------
# use_cache=False — generator called fresh each time
# ---------------------------------------------------------------------------

@pytest_asyncio.fixture
async def no_cache_gen_app():
    app = JustAPIApp()
    call_count = 0
    cleanup_log = []

    async def get_config():
        nonlocal call_count
        call_count += 1
        try:
            yield {"mode": "test"}
        finally:
            cleanup_log.append(f"config_close_{call_count}")

    @app.get("/nocache")
    async def nocache(c1=Depends(get_config, use_cache=False), c2=Depends(get_config, use_cache=False)):
        return {"c1": c1, "c2": c2, "call_count": call_count}

    return app, call_count, cleanup_log


@pytest.mark.asyncio
async def test_no_cache_generator_dep(no_cache_gen_app):
    app, call_count, cleanup_log = no_cache_gen_app
    from justapi.testing import AsyncTestClient

    async with AsyncTestClient(app) as c:
        r = await c.get("/nocache")
        assert_ok(r)
        import json
        data = json.loads(bytes(r["body"]))
        assert data["call_count"] == 2

    assert len(cleanup_log) == 2
