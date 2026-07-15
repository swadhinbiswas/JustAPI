"""Tests for the schema-backed native Rust fast path (``native=True``).

When a route is registered with ``native=True`` and a ``schema``, justapi
serves it entirely in Rust: the request body is validated against the JSON
schema and, on success, echoed back as the response — no Python handler is
invoked and no ``Request`` object is built. This removes the entire
Python<->Rust handler-call boundary for those routes.
"""
import json

import pytest

from justapi import JustAPIApp, Schema, Depends
from justapi.testing import AsyncTestClient, assert_status


class UserSchema(Schema):
    name: str
    email: str
    age: int | None = None


@pytest.mark.asyncio
async def test_native_post_echoes_validated_body():
    app = JustAPIApp()

    @app.post("/users", schema=UserSchema, native=True)
    def create_user(request):
        raise AssertionError("handler must not run in native mode")

    async with AsyncTestClient(app) as c:
        r = await c.post(
            "/users", body=json.dumps({"name": "Ada", "email": "a@x.io"}).encode()
        )
        assert_status(r, 200)
        assert json.loads(r["body"]) == {"name": "Ada", "email": "a@x.io"}


@pytest.mark.asyncio
async def test_native_post_rejects_invalid_body():
    app = JustAPIApp()

    @app.post("/users", schema=UserSchema, native=True)
    def create_user(request):
        raise AssertionError("handler must not run in native mode")

    async with AsyncTestClient(app) as c:
        r = await c.post("/users", body=json.dumps({"name": "Ada"}).encode())
        assert_status(r, 422)
        assert r["headers"].get("content-type") == "application/problem+json"


@pytest.mark.asyncio
async def test_native_without_schema_falls_through_to_python():
    app = JustAPIApp()
    seen = {}

    @app.post("/echo", native=True)
    def echo(request):
        seen["called"] = True
        body = json.loads(request["body"])
        return {"body": json.dumps(body).encode("utf-8")}

    async with AsyncTestClient(app) as c:
        r = await c.post("/echo", body=json.dumps({"hi": 1}).encode())
        assert_status(r, 200)
        assert seen.get("called") is True
        assert json.loads(r["body"]) == {"hi": 1}


@pytest.mark.asyncio
async def test_native_put_echoes_validated_body():
    app = JustAPIApp()

    @app.put("/users", schema=UserSchema, native=True)
    def put_user(request):
        raise AssertionError("handler must not run in native mode")

    payload = json.dumps({"name": "Ada", "email": "a@x.io"}).encode()
    async with AsyncTestClient(app) as c:
        r = await c.put("/users", payload)
        assert_status(r, 200)
        assert json.loads(r["body"]) == {"name": "Ada", "email": "a@x.io"}


QUERY_SCHEMA = {
    "type": "object",
    "properties": {"name": {"type": "string"}, "age": {"type": "integer", "minimum": 0}},
    "required": ["name"],
}


@pytest.mark.asyncio
async def test_native_get_validates_query_and_echoes():
    app = JustAPIApp()

    @app.get("/search", query_schema=QUERY_SCHEMA, native=True)
    def search():
        raise AssertionError("handler must not run in native mode")

    async with AsyncTestClient(app) as c:
        r = await c.get("/search?name=Ada&age=30")
        assert_status(r, 200)
        # Query values are coerced to JSON scalars (age -> int) before echo.
        assert json.loads(r["body"]) == {"name": "Ada", "age": 30}


@pytest.mark.asyncio
async def test_native_get_rejects_invalid_query():
    app = JustAPIApp()

    @app.get("/search", query_schema=QUERY_SCHEMA, native=True)
    def search():
        raise AssertionError("handler must not run in native mode")

    async with AsyncTestClient(app) as c:
        r = await c.get("/search?age=-5")
        assert_status(r, 422)
        assert r["headers"].get("content-type") == "application/problem+json"


def test_native_with_route_dependencies_rejected():
    app = JustAPIApp()

    def auth_dep():
        return {"user": "x"}

    with pytest.raises(ValueError):

        @app.post("/users", schema=UserSchema, dependencies=[Depends(auth_dep)], native=True)
        def create_user(request):
            raise AssertionError("must not be registered")


def test_native_with_route_middleware_rejected():
    app = JustAPIApp()

    def mw(request, call_next):
        return call_next(request)

    with pytest.raises(ValueError):

        @app.post("/users", schema=UserSchema, middlewares=[mw], native=True)
        def create_user(request):
            raise AssertionError("must not be registered")


def test_native_with_app_level_dependencies_rejected():
    def auth_dep():
        return {"user": "x"}

    app = JustAPIApp(dependencies=[Depends(auth_dep)])

    with pytest.raises(ValueError):

        @app.post("/users", schema=UserSchema, native=True)
        def create_user(request):
            raise AssertionError("must not be registered")

