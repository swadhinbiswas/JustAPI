"""Integration test for Phase 11 — JSON Schema validation (Schema class + Pydantic bridge).
"""
import json
import subprocess
import sys
import time
import urllib.request
import urllib.error

SERVER_SCRIPT = r'''
from justapi import JustAPIApp, Schema
import json

class UserSchema(Schema):
    name: str
    email: str
    age: int | None = None

async def create_user(request):
    body = json.loads(request["body"])
    return {
        "status": 201,
        "body": json.dumps({"id": 1, **body}).encode("utf-8"),
        "headers": [(b"content-type", b"application/json")],
    }

async def hello_handler(request):
    name = request["path_params"]["name"]
    return {"message": f"Hello {name}!"}

app = JustAPIApp()

app.get("/hello/{name}", hello_handler)
app.post("/users", create_user, schema=UserSchema)

# Pydantic bridge test
from pydantic import BaseModel
class ProductModel(BaseModel):
    name: str
    price: float

async def create_product(request):
    body = json.loads(request["body"])
    return {"status": 201, "body": json.dumps({"sku": 42, **body}).encode(), "headers": [(b"content-type", b"application/json")]}

app.post("/products", create_product, schema=ProductModel)

# Raw JSON Schema string test
raw_schema = json.dumps({
    "type": "object",
    "properties": {
        "title": {"type": "string"},
        "year": {"type": "integer"}
    },
    "required": ["title"],
    "additionalProperties": False
})

async def create_movie(request):
    body = json.loads(request["body"])
    return {"status": 201, "body": json.dumps({"id": 7, **body}).encode(), "headers": [(b"content-type", b"application/json")]}

app.post("/movies", create_movie, schema=raw_schema)

app.run("127.0.0.1:9869")
'''


def test_schema_validation():
    proc = subprocess.Popen(
        [sys.executable, "-c", SERVER_SCRIPT],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    addr = "127.0.0.1:9869"
    time.sleep(0.8)

    try:
        # 1. Basic route still works
        resp = urllib.request.urlopen(f"http://{addr}/hello/world")
        assert resp.status == 200, f"Expected 200, got {resp.status}"
        data = json.loads(resp.read())
        assert data == {"message": "Hello world!"}
        print("PASS: GET /hello/world -> 200")

        # 2. Schema class: valid POST
        payload = json.dumps({"name": "Alice", "email": "alice@example.com", "age": 30}).encode()
        req = urllib.request.Request(
            f"http://{addr}/users",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        resp = urllib.request.urlopen(req)
        assert resp.status == 201, f"Expected 201, got {resp.status}"
        data = json.loads(resp.read())
        assert data["name"] == "Alice"
        assert data["email"] == "alice@example.com"
        assert data["age"] == 30
        print("PASS: Schema class valid POST -> 201")

        # 3. Schema class: missing required field
        payload = json.dumps({"name": "Alice"}).encode()
        req = urllib.request.Request(
            f"http://{addr}/users",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            urllib.request.urlopen(req)
            assert False, "Expected 422"
        except urllib.error.HTTPError as e:
            assert e.code == 422, f"Expected 422, got {e.code}"
            error_data = json.loads(e.read())
            assert error_data["title"] == "Validation Error"
            assert any("email" in err.get("message", "") or "email" in err.get("field", "") for err in error_data["errors"])
            print("PASS: Schema class missing field -> 422")

        # 4. Schema class: wrong type
        payload = json.dumps({"name": "Alice", "email": "a@b.com", "age": "not a number"}).encode()
        req = urllib.request.Request(
            f"http://{addr}/users",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            urllib.request.urlopen(req)
            assert False, "Expected 422"
        except urllib.error.HTTPError as e:
            assert e.code == 422
            print("PASS: Schema class wrong type -> 422")

        # 5. Pydantic bridge: valid POST
        payload = json.dumps({"name": "Widget", "price": 9.99}).encode()
        req = urllib.request.Request(
            f"http://{addr}/products",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        resp = urllib.request.urlopen(req)
        assert resp.status == 201
        data = json.loads(resp.read())
        assert data["name"] == "Widget"
        assert data["price"] == 9.99
        print("PASS: Pydantic bridge valid POST -> 201")

        # 6. Pydantic bridge: missing required field
        payload = json.dumps({"name": "Widget"}).encode()
        req = urllib.request.Request(
            f"http://{addr}/products",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            urllib.request.urlopen(req)
            assert False, "Expected 422"
        except urllib.error.HTTPError as e:
            assert e.code == 422
            print("PASS: Pydantic bridge missing field -> 422")

        # 7. Raw JSON Schema string: valid POST
        payload = json.dumps({"title": "Inception", "year": 2010}).encode()
        req = urllib.request.Request(
            f"http://{addr}/movies",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        resp = urllib.request.urlopen(req)
        assert resp.status == 201
        print("PASS: Raw JSON Schema valid POST -> 201")

        # 8. Raw JSON Schema string: missing required field
        payload = json.dumps({"year": 2010}).encode()
        req = urllib.request.Request(
            f"http://{addr}/movies",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            urllib.request.urlopen(req)
            assert False, "Expected 422"
        except urllib.error.HTTPError as e:
            assert e.code == 422
            print("PASS: Raw JSON Schema missing field -> 422")

        # 9. Invalid JSON body
        req = urllib.request.Request(
            f"http://{addr}/users",
            data=b"not json",
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            urllib.request.urlopen(req)
            assert False, "Expected 422"
        except urllib.error.HTTPError as e:
            assert e.code == 422
            error_data = json.loads(e.read())
            assert "Invalid JSON" in error_data["detail"]
            print("PASS: Invalid JSON body -> 422")

        print()
        print("=== All schema validation tests PASSED ===")

    finally:
        proc.terminate()
        try:
            stdout, stderr = proc.communicate(timeout=5)
            if stderr:
                decoded = stderr.decode()
                if decoded.strip():
                    print("Server stderr:", decoded)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.communicate()


if __name__ == "__main__":
    test_schema_validation()
