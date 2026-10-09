"""Pydantic body/form models + required-param semantics (FastAPI parity).

Covers the flagship FastAPI patterns that were broken:
- `def handler(item: ItemModel)` — plain Pydantic body param (demo_app pattern)
- `item: Item = Body(..., embed=True)` — nested body key
- `form: FormModel = Form(...)` — Pydantic model from multipart AND urlencoded
- `q: str = Query(...)` — missing required param must 422, never leak `Ellipsis`
"""
import json

import pytest
from pydantic import BaseModel

from justapi import Body, Form, JustAPIApp, JustAPITestClient, Query


def _body(resp):
    return json.loads(bytes(resp["body"]))


class ItemRequest(BaseModel):
    name: str
    price: float
    description: str | None = None


class LoginForm(BaseModel):
    username: str
    password: str
    remember_me: bool = False


def test_plain_pydantic_body_param():
    app = JustAPIApp()

    @app.post("/items")
    async def create_item(item: ItemRequest):
        return {"data": item.model_dump()}

    c = JustAPITestClient(app)
    resp = c.post_with(
        "/items", b'{"name":"Test","price":99.99}', [("Content-Type", "application/json")]
    )
    assert resp["status"] == 200, resp
    assert _body(resp)["data"]["name"] == "Test"


def test_plain_pydantic_body_invalid_422():
    app = JustAPIApp()

    @app.post("/items")
    async def create_item(item: ItemRequest):
        return {"data": item.model_dump()}

    c = JustAPITestClient(app)
    resp = c.post_with("/items", b'{"name":"x"}', [("Content-Type", "application/json")])
    assert resp["status"] == 422
    assert "price" in json.dumps(_body(resp))


def test_body_embed_true():
    app = JustAPIApp()

    @app.post("/embed")
    def embed(item: ItemRequest = Body(..., embed=True)):
        return item.model_dump()

    c = JustAPITestClient(app)
    ok = c.post_with(
        "/embed", b'{"item":{"name":"n","price":2.5}}', [("Content-Type", "application/json")]
    )
    assert ok["status"] == 200, ok
    assert _body(ok)["name"] == "n"

    missing = c.post_with("/embed", b'{"name":"n","price":2.5}', [("Content-Type", "application/json")])
    assert missing["status"] == 422


def test_required_query_missing_is_422_not_ellipsis():
    app = JustAPIApp()

    @app.get("/req")
    def req(q: str = Query(...)):
        return {"q": q}

    c = JustAPITestClient(app)
    assert c.get("/req")["status"] == 422
    assert _body(c.get("/req?q=hi")) == {"q": "hi"}


def _multipart(fields, boundary="----T"):
    body = b""
    for k, v in fields:
        body += (
            f"--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n".encode()
        )
    body += f"--{boundary}--\r\n".encode()
    return body, f"multipart/form-data; boundary={boundary}"


def test_form_model_multipart():
    app = JustAPIApp()

    @app.post("/login")
    def login(form: LoginForm = Form(...)):
        return {"username": form.username, "remember": form.remember_me}

    c = JustAPITestClient(app)
    body, ct = _multipart([("username", "u"), ("password", "p"), ("remember_me", "true")])
    resp = c.post_with("/login", body, [("Content-Type", ct)])
    assert resp["status"] == 200, resp
    assert _body(resp) == {"username": "u", "remember": True}


def test_form_model_urlencoded():
    app = JustAPIApp()

    @app.post("/login")
    def login(form: LoginForm = Form(...)):
        return {"username": form.username}

    c = JustAPITestClient(app)
    resp = c.post_with(
        "/login",
        b"username=u&password=p&remember_me=true",
        [("Content-Type", "application/x-www-form-urlencoded")],
    )
    assert resp["status"] == 200, resp
    assert _body(resp)["username"] == "u"

    # Missing required model field → 422 with the field name.
    bad = c.post_with(
        "/login", b"username=u", [("Content-Type", "application/x-www-form-urlencoded")]
    )
    assert bad["status"] == 422
    assert "password" in json.dumps(_body(bad))


def test_scalar_form_urlencoded():
    app = JustAPIApp()

    @app.post("/scalar")
    def scalar(username: str = Form(...), note: str = Form("")):
        return {"username": username, "note": note}

    c = JustAPITestClient(app)
    resp = c.post_with(
        "/scalar", b"username=bob", [("Content-Type", "application/x-www-form-urlencoded")]
    )
    assert resp["status"] == 200, resp
    assert _body(resp) == {"username": "bob", "note": ""}
