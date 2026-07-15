"""Integration test for Phase 11 — Request Validation & Serialization.
"""
import json
import subprocess
import sys
import time
import urllib.request
import urllib.error

SERVER_SCRIPT = r'''
import json
from justapi import JustAPIApp

def validate_create_user(data):
    errors = []
    if "name" not in data:
        errors.append("name is required")
    elif len(data["name"]) < 2:
        errors.append("name must be at least 2 characters")
    if "email" not in data:
        errors.append("email is required")
    elif "@" not in data["email"]:
        errors.append("email must be valid")
    return errors if errors else None

def validate_noop(data):
    return None

app = JustAPIApp()

async def hello_handler(request):
    name = request["path_params"]["name"]
    return {"message": f"Hello {name}!"}
app.get("/hello/{name}", hello_handler)

async def create_user(request):
    body = json.loads(request["body"])
    return {
        "status": 201,
        "body": json.dumps({"id": 1, **body}).encode("utf-8"),
        "headers": [(b"content-type", b"application/json")],
    }
app.post("/users", create_user, body_schema=validate_create_user)

async def noop_handler(request):
    return {"message": "ok"}
app.post("/noop", noop_handler, body_schema=validate_noop)

app.run("127.0.0.1:9868")
'''


def test_validation():
    proc = subprocess.Popen(
        [sys.executable, "-c", SERVER_SCRIPT],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    addr = "127.0.0.1:9868"
    time.sleep(0.8)

    try:
        # 1. Existing functionality still works
        resp = urllib.request.urlopen(f"http://{addr}/hello/world")
        assert resp.status == 200, f"Expected 200, got {resp.status}"
        print("PASS: /hello/world -> 200 (basic route works)")

        # 2. Valid POST with body_schema
        payload = json.dumps({"name": "Alice", "email": "alice@example.com"}).encode()
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
        print("PASS: POST /users with valid body -> 201")

        # 3. Invalid POST (missing required fields)
        payload = json.dumps({"name": "A"}).encode()
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
            assert isinstance(error_data.get("detail"), str) and len(error_data["detail"]) > 0
            print(f"PASS: POST /users with invalid body -> 422 ({error_data['detail']})")

        # 4. Invalid POST (name too short)
        payload = json.dumps({"name": "A", "email": "test@test.com"}).encode()
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
            print("PASS: POST /users with short name -> 422")

        # 5. Body schema that always passes
        payload = json.dumps({"anything": "goes"}).encode()
        req = urllib.request.Request(
            f"http://{addr}/noop",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        resp = urllib.request.urlopen(req)
        assert resp.status == 200
        print("PASS: POST /noop with noop validator -> 200")

        print()
        print("=== All validation tests PASSED ===")

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
    test_validation()
