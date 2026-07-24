"""Regression test for P1.1 / BUG-3 — `body_schema` routes must receive the
parsed/validated body as a Python object, not raw bytes.

When a route is registered with a `justapi.Schema` body_schema, the dispatch
layer parses the JSON body on the fast path and attaches it to the `Request`
object, so `request.json()` and `request["body"]` return the parsed dict
(already validated). Legacy callable schemas keep the original behaviour.
"""
import json
import sys

import pytest

from justapi import JustAPIApp, Schema, Request
from justapi.testing import AsyncTestClient


class CreateProduct(Schema):
    name: str
    price: float
    stock: int = 0


def captured():
    state = {}

    async def create(req: Request):
        # `request.json()` should already be the validated/parsed dict, not raw
        # bytes. It must equal the posted payload (minus server-side mutation).
        body = req.json()
        state["json_type"] = type(body).__name__
        state["json_value"] = body
        # `request["body"]` should also return the parsed dict for schema routes.
        state["getitem_type"] = type(req["body"]).__name__
        state["validated_body"] = req.validated_body
        return {"ok": True, "name": body["name"], "price": body["price"]}

    return create, state


@pytest.mark.asyncio
async def test_body_schema_delivers_parsed_dict():
    app = JustAPIApp()
    handler, state = captured()
    app.post("/products", handler, body_schema=CreateProduct)

    async with AsyncTestClient(app) as client:
        payload = {"name": "Widget", "price": 12.5, "stock": 3}
        resp = await client.post("/products", json.dumps(payload).encode())
        assert resp["status"] == 200, resp.get("status")
        data = json.loads(resp["body"])
        assert data["name"] == "Widget"
        assert data["price"] == 12.5

    # The handler must have received a parsed dict, not raw bytes.
    assert state["json_type"] == "dict", state["json_type"]
    assert state["getitem_type"] == "dict", state["getitem_type"]
    assert state["validated_body"] == payload or state["validated_body"] == {
        "name": "Widget",
        "price": 12.5,
        "stock": 3,
    }
    assert state["json_value"]["name"] == "Widget"
    print("PASS: body_schema route received parsed dict (not raw bytes)")


if __name__ == "__main__":
    sys.exit(0 if __import__("asyncio").run(test_body_schema_delivers_parsed_dict()) is None else 1)
