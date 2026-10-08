"""Integration test for JustAPITestClient — the no-TCP test client.
"""
import json

from justapi import JustAPIApp, JustAPITestClient


def test_test_client_get():
    app = JustAPIApp()

    async def hello_handler(request):
        name = request["path_params"]["name"]
        return {"message": f"Hello {name}!"}

    app.get("/hello/{name}", hello_handler)
    client = JustAPITestClient(app)

    resp = client.get("/hello/world")
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data == {"message": "Hello world!"}

    resp = client.get("/hello/JustAPI")
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data == {"message": "Hello JustAPI!"}


def test_test_client_post():
    app = JustAPIApp()

    async def echo_handler(request):
        return {
            "status": 200,
            "headers": [(b"content-type", b"application/json")],
            "body": request["body"],
        }

    app.post("/echo", echo_handler)
    client = JustAPITestClient(app)

    payload = json.dumps({"hello": "world"}).encode()
    resp = client.post("/echo", payload)
    assert resp["status"] == 200
    assert bytes(resp["body"]) == payload


def test_test_client_put():
    app = JustAPIApp()

    async def echo_handler(request):
        return {
            "status": 200,
            "headers": [(b"content-type", b"application/json")],
            "body": request["body"],
        }

    app.put("/echo", echo_handler)
    client = JustAPITestClient(app)

    payload = json.dumps({"updated": True}).encode()
    resp = client.put("/echo", payload)
    assert resp["status"] == 200
    assert bytes(resp["body"]) == payload


def test_test_client_delete():
    app = JustAPIApp()

    async def delete_handler(request):
        return {"status": 204}

    app.delete("/resource/{id}", delete_handler)
    client = JustAPITestClient(app)

    resp = client.delete("/resource/42")
    assert resp["status"] == 204


def test_test_client_404():
    app = JustAPIApp()

    async def hello_handler(request):
        return {"message": "hello"}

    app.get("/hello", hello_handler)
    client = JustAPITestClient(app)

    resp = client.get("/nonexistent")
    assert resp["status"] == 404


def test_test_client_wrong_method():
    app = JustAPIApp()

    async def echo_handler(request):
        return {"body": request["body"]}

    app.post("/echo", echo_handler)
    client = JustAPITestClient(app)

    resp = client.get("/echo")
    assert resp["status"] == 405


def test_test_client_schema_validation():
    """Rust-side JSON Schema validation works through the test client."""
    from justapi import Schema

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
    client = JustAPITestClient(app)

    # Valid payload
    payload = json.dumps({"name": "Alice", "email": "alice@example.com", "age": 30}).encode()
    resp = client.post("/users", payload)
    assert resp["status"] == 201
    data = json.loads(bytes(resp["body"]))
    assert data["id"] == 1
    assert data["name"] == "Alice"

    # Missing required field
    payload = json.dumps({"name": "Alice"}).encode()
    resp = client.post("/users", payload)
    assert resp["status"] == 422
    error_data = json.loads(bytes(resp["body"]))
    assert isinstance(error_data.get("detail"), str) and len(error_data["detail"]) > 0


def test_test_client_cookie_jar():
    """Cookies set by the server persist across requests (FastAPI parity)."""
    from justapi import JSONResponse

    app = JustAPIApp()

    @app.get("/set")
    def set_():
        r = JSONResponse({"ok": True})
        r.set_cookie("sid", "abc", httponly=True)
        return r

    @app.get("/read")
    def read(request):
        return {"sid": request.cookies.get("sid")}

    @app.get("/del")
    def del_():
        r = JSONResponse({"bye": True})
        r.delete_cookie("sid")
        return r

    c = JustAPITestClient(app)
    assert c.cookies() == {}

    c.get("/set")
    assert c.cookies() == {"sid": "abc"}

    resp = c.get("/read")
    assert json.loads(bytes(resp["body"])) == {"sid": "abc"}

    # Max-Age=0 removes the entry from the jar.
    c.get("/del")
    assert c.cookies() == {}

    # Explicit clear
    c.get("/set")
    c.clear_cookies()
    assert c.cookies() == {}


def test_test_client_explicit_cookie_header_wins():
    """A caller-supplied Cookie header is not overridden by the jar."""
    from justapi import JSONResponse

    app = JustAPIApp()

    @app.get("/set")
    def set_():
        r = JSONResponse({"ok": True})
        r.set_cookie("sid", "jar-value")
        return r

    @app.get("/read")
    def read(request):
        return {"sid": request.cookies.get("sid")}

    c = JustAPITestClient(app)
    c.get("/set")
    resp = c.get_with("/read", [("Cookie", "sid=explicit")])
    assert json.loads(bytes(resp["body"])) == {"sid": "explicit"}
