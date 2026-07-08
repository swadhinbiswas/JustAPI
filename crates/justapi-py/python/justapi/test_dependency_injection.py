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
