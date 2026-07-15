"""Tests for JustAPI alias, multi-method routing, and router inclusion hierarchy.
"""
import json
import pytest
import pytest_asyncio
from justapi import JustAPI, APIRouter, Schema
from justapi.testing import AsyncTestClient, assert_ok, assert_json

@pytest.mark.asyncio
async def test_justapi_alias_and_routing():
    # Test instantiating with JustAPI instead of JustAPIApp
    app = JustAPI()

    # Test @app.route decorator with multiple methods
    @app.route("/items", methods=["GET", "POST"])
    async def manage_items(request):
        method = request["method"]
        if method == "GET":
            return {"action": "list"}
        elif method == "POST":
            body = json.loads(request["body"])
            return {"action": "create", "data": body}

    async with AsyncTestClient(app) as c:
        # Test GET request
        resp = await c.get("/items")
        assert_ok(resp)
        assert_json(resp, {"action": "list"})

        # Test POST request
        resp = await c.post("/items", b'{"name": "test"}')
        assert_ok(resp)
        assert_json(resp, {"action": "create", "data": {"name": "test"}})

@pytest.mark.asyncio
async def test_api_router_and_inclusion():
    app = JustAPI()
    
    # Create main API router
    api_router = APIRouter(prefix="/api/v1")
    
    # Create sub-router for users
    users_router = APIRouter(prefix="/users")

    @users_router.get("/")
    async def list_users(request):
        return {"users": ["Alice", "Bob"]}

    @users_router.route("/manage", methods=["GET", "POST"])
    async def manage_user(request):
        method = request["method"]
        return {"method": method, "domain": "users"}

    # Nest users router inside api router
    api_router.include_router(users_router)

    # Include api router in app
    app.include_router(api_router)

    async with AsyncTestClient(app) as c:
        # Test nested list users endpoint
        resp = await c.get("/api/v1/users/")
        assert_ok(resp)
        assert_json(resp, {"users": ["Alice", "Bob"]})

        # Test route decorator on nested router (GET)
        resp = await c.get("/api/v1/users/manage")
        assert_ok(resp)
        assert_json(resp, {"method": "GET", "domain": "users"})

        # Test route decorator on nested router (POST)
        resp = await c.post("/api/v1/users/manage", b"")
        assert_ok(resp)
        assert_json(resp, {"method": "POST", "domain": "users"})
