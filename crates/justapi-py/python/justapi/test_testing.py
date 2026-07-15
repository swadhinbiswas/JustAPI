"""Integration tests for justapi.testing — AsyncTestClient and assertion helpers.
"""
import json
import pytest
import pytest_asyncio
from justapi import JustAPIApp, Schema
from justapi.testing import (
    AsyncTestClient,
    assert_ok,
    assert_status,
    assert_json,
    assert_header,
    transaction_test_db,
)


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest_asyncio.fixture
async def app():
    app = JustAPIApp()

    async def hello_handler(request):
        name = request["path_params"]["name"]
        return {"message": f"Hello {name}!"}

    async def echo_handler(request):
        return {
            "status": 200,
            "headers": [(b"content-type", b"application/json")],
            "body": request["body"],
        }

    async def created_handler(request):
        return {"status": 201, "body": b'{"id":1}'}

    async def delete_handler(request):
        return {"status": 204}

    app.get("/hello/{name}", hello_handler)
    app.post("/echo", echo_handler)
    app.put("/echo", echo_handler)
    app.delete("/resource/{id}", delete_handler)

    return app


@pytest_asyncio.fixture
async def client(app):
    async with AsyncTestClient(app) as c:
        yield c


# ---------------------------------------------------------------------------
# AsyncTestClient
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_async_get(client):
    resp = await client.get("/hello/world")
    assert_ok(resp)
    assert_json(resp, {"message": "Hello world!"})


@pytest.mark.asyncio
async def test_async_get_path_params(client):
    resp = await client.get("/hello/JustAPI")
    assert_ok(resp)
    data = json.loads(bytes(resp["body"]))
    assert data == {"message": "Hello JustAPI!"}


@pytest.mark.asyncio
async def test_async_post(client):
    payload = json.dumps({"hello": "world"}).encode()
    resp = await client.post("/echo", payload)
    assert_ok(resp)
    assert bytes(resp["body"]) == payload


@pytest.mark.asyncio
async def test_async_put(client):
    payload = json.dumps({"updated": True}).encode()
    resp = await client.put("/echo", payload)
    assert resp["status"] == 200
    assert bytes(resp["body"]) == payload


@pytest.mark.asyncio
async def test_async_delete(client):
    resp = await client.delete("/resource/42")
    assert resp["status"] == 204


@pytest.mark.asyncio
async def test_async_404(client):
    resp = await client.get("/nonexistent")
    assert resp["status"] == 404


@pytest.mark.asyncio
async def test_async_wrong_method(client):
    resp = await client.get("/echo")
    assert resp["status"] == 405


# ---------------------------------------------------------------------------
# Assertion helpers
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_assert_status(client):
    resp = await client.get("/hello/world")
    assert_ok(resp)
    resp = await client.delete("/resource/1")
    assert_status(resp, 204)


@pytest.mark.asyncio
async def test_assert_json(client):
    resp = await client.get("/hello/test")
    assert_json(resp, {"message": "Hello test!"})


@pytest.mark.asyncio
async def test_assert_header(client):
    resp = await client.get("/hello/world")
    assert_header(resp, "content-type", "application/json")
    # Test existence-only check
    assert_header(resp, "content-type")


@pytest.mark.asyncio
async def test_assert_ok_passes():
    assert_ok({"status": 200, "body": b"ok"})


@pytest.mark.asyncio
async def test_assert_status_passes():
    assert_status({"status": 201, "body": b'{"id":1}'}, 201)


# ---------------------------------------------------------------------------
# Setup / teardown hooks
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_setup_teardown_hooks():
    app = JustAPIApp()
    app.get("/ping", lambda r: {"pong": True})

    events = []

    async with AsyncTestClient(app).on_setup(
        lambda c: events.append("setup")
    ).on_teardown(
        lambda c: events.append("teardown")
    ) as c:
        resp = await c.get("/ping")
        assert_ok(resp)

    assert events == ["setup", "teardown"]


# ---------------------------------------------------------------------------
# Schema validation through async client
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_async_schema_validation():
    class UserSchema(Schema):
        name: str
        email: str
        age: int | None = None

    app = JustAPIApp()

    async def create_user(request):
        body = json.loads(request["body"])
        return {
            "status": 201,
            "body": json.dumps({"id": 1, **body}).encode(),
            "headers": [(b"content-type", b"application/json")],
        }

    app.post("/users", create_user, schema=UserSchema)

    async with AsyncTestClient(app) as c:
        # Valid payload
        payload = json.dumps({
            "name": "Alice", "email": "alice@example.com", "age": 30
        }).encode()
        resp = await c.post("/users", payload)
        assert resp["status"] == 201

        # Missing required field
        payload = json.dumps({"name": "Alice"}).encode()
        resp = await c.post("/users", payload)
        assert resp["status"] == 422


# ---------------------------------------------------------------------------
# transaction_test_db
# ---------------------------------------------------------------------------


def test_transaction_test_db_default():
    url = transaction_test_db()
    assert url == "sqlite://:memory:"


def test_transaction_test_db_custom():
    url = transaction_test_db("postgres://localhost/test")
    assert url == "postgres://localhost/test"
