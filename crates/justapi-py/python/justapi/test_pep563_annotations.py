"""Regression tests for PEP 563 — ``from __future__ import annotations``.

When a module uses postponed evaluation, every annotation is stored as a
string. JustAPI must resolve them (like FastAPI does) before classifying
parameters, or Pydantic body models are reported "field required", int/float
query values stay strings, ``Query`` constraints silently pass, and the
OpenAPI document loses its request bodies.

This module deliberately enables ``from __future__ import annotations`` so
every handler below exercises the stringified path.
"""

from __future__ import annotations

import enum
import json

from pydantic import BaseModel, Field

from justapi import (
    BackgroundTasks,
    Depends,
    JustAPIApp,
    JustAPITestClient,
    Query,
)

app = JustAPIApp()

bg_hits: list = []


class Item(BaseModel):
    name: str = Field(min_length=1)
    price: float = Field(gt=0)


class Color(str, enum.Enum):
    red = "red"
    blue = "blue"


def current_user() -> str:
    return "alice"


def _bg_note(marker: str) -> None:
    bg_hits.append(marker)


@app.post("/items", status_code=201)
def create_item(item: Item, background_tasks: BackgroundTasks):
    background_tasks.add_task(_bg_note, item.name)
    return item


@app.get("/search")
def search(
    q: str = Query(..., min_length=2),
    limit: int = Query(10, ge=1, le=100),
    ratio: float = Query(0.5, gt=0),
    color: Color = Query(None),
    ids: list[int] = Query(None),
):
    return {
        "q": q,
        "limit": limit,
        "ratio": ratio,
        "color": color.value if color is not None else None,
        "ids": ids,
    }


@app.get("/me")
def me(user: str = Depends(current_user)):
    return {"user": user}


@app.get("/items/{item_id}")
def get_item(item_id: int):
    return {"item_id": item_id}


client = JustAPITestClient(app)
JSON_HEADERS = [("Content-Type", "application/json")]


def _json(resp) -> dict:
    raw = bytes(resp["body"])
    return json.loads(raw) if raw else {}


def test_body_model_param_resolves():
    resp = client.post_with(
        "/items", b'{"name":"mug","price":9.5}', JSON_HEADERS
    )
    assert resp["status"] == 201, _json(resp)
    assert _json(resp) == {"name": "mug", "price": 9.5}


def test_body_model_validation_still_applies():
    resp = client.post_with(
        "/items", b'{"name":"","price":-1}', JSON_HEADERS
    )
    assert resp["status"] == 422


def test_body_model_missing_is_422_not_typeerror():
    resp = client.post_with("/items", b"{}", JSON_HEADERS)
    assert resp["status"] == 422
    detail = _json(resp)["detail"]
    assert isinstance(detail, list) and detail[0]["loc"] == ["item"]


def test_query_coercion_and_constraints():
    ok = _json(client.get("/search?q=ab&limit=5&ratio=1.5&color=red"))
    assert ok["limit"] == 5
    assert ok["ratio"] == 1.5
    assert ok["color"] == "red"

    assert client.get("/search")["status"] == 422  # required q
    assert client.get("/search?q=a")["status"] == 422  # min_length
    assert client.get("/search?q=ab&limit=0")["status"] == 422  # ge
    assert client.get("/search?q=ab&limit=101")["status"] == 422  # le
    assert client.get("/search?q=ab&ratio=0")["status"] == 422  # gt
    assert client.get("/search?q=ab&color=green")["status"] == 422  # enum
    assert client.get("/search?q=ab&limit=abc")["status"] == 422  # int parse


def test_query_defaults_apply():
    got = _json(client.get("/search?q=ok"))
    assert got["limit"] == 10
    assert got["ratio"] == 0.5
    assert got["color"] is None


def test_list_query_param():
    got = _json(client.get("/search?q=ab&ids=1&ids=2"))
    assert got["ids"] == [1, 2]


def test_path_param_coercion():
    assert _json(client.get("/items/42"))["item_id"] == 42


def test_dependency_resolves_its_own_annotations():
    assert _json(client.get("/me"))["user"] == "alice"


def test_background_tasks_with_body_model():
    bg_hits.clear()
    resp = client.post_with(
        "/items", b'{"name":"cup","price":2}', JSON_HEADERS
    )
    assert resp["status"] == 201
    assert bg_hits == ["cup"]


def test_openapi_request_body_from_annotation():
    spec = _json(client.get("/openapi.json"))
    body = spec["paths"]["/items"]["post"]["requestBody"]
    assert body["content"]["application/json"]["schema"] == {
        "$ref": "#/components/schemas/Item"
    }
    item = spec["components"]["schemas"]["Item"]
    assert item["properties"]["price"]["type"] == "number"
    assert item["properties"]["name"]["type"] == "string"
    assert "name" in item["required"] and "price" in item["required"]


def test_openapi_query_params_carry_types_and_constraints():
    spec = _json(client.get("/openapi.json"))
    params = {
        p["name"]: p for p in spec["paths"]["/search"]["get"]["parameters"]
    }
    assert params["limit"]["schema"]["type"] == "integer"
    assert params["limit"]["schema"]["minimum"] == 1
    assert params["limit"]["schema"]["maximum"] == 100
    assert params["q"]["required"] is True
    assert params["q"]["schema"]["minLength"] == 2
    assert params["color"]["schema"]["enum"] == ["red", "blue"]
    assert params["ids"]["schema"]["type"] == "array"
