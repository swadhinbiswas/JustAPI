"""Integration test for Phase 14 observability features.

Tests verify the framework can start and serve requests. Enhanced metrics
and health checks are tested at the Rust level via unit tests.
"""
import json
import subprocess
import sys
import time
import urllib.request
import urllib.error

SERVER_SCRIPT = r"""
from justapi import JustAPIApp

async def hello_handler(request):
    name = request["path_params"]["name"]
    return {"message": f"Hello {name}!"}

async def echo_handler(request):
    return {
        "status": 200,
        "headers": [(b"content-type", b"application/json")],
        "body": request["body"],
    }

app = JustAPIApp()
app.get("/hello/{name}", hello_handler)
app.post("/echo", echo_handler)
app.run("127.0.0.1:9877")
"""


def test_server_starts_and_serves_requests():
    proc = subprocess.Popen(
        [sys.executable, "-c", SERVER_SCRIPT],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    addr = "127.0.0.1:9877"
    time.sleep(0.8)

    try:
        resp = urllib.request.urlopen(f"http://{addr}/hello/world")
        assert resp.status == 200
        data = json.loads(resp.read())
        assert data == {"message": "Hello world!"}
        print("PASS: /hello/world")

        payload = b'{"hello":"world"}'
        req = urllib.request.Request(
            f"http://{addr}/echo",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        resp = urllib.request.urlopen(req)
        assert resp.status == 200
        assert resp.read() == payload
        print("PASS: POST /echo")

        try:
            urllib.request.urlopen(f"http://{addr}/nonexistent")
            assert False, "Expected 404"
        except urllib.error.HTTPError as e:
            assert e.code == 404
            print("PASS: /nonexistent -> 404")

    finally:
        proc.terminate()
        proc.wait()


def test_justapi_test_client_works():
    """Verify JustAPITestClient works for testing without a running server."""
    from justapi import JustAPIApp, JustAPITestClient

    app = JustAPIApp()

    async def hello_handler(request):
        name = request["path_params"]["name"]
        return {"message": f"Hello {name}!"}

    app.get("/hello/{name}", hello_handler)
    client = JustAPITestClient(app)

    resp = client.get("/hello/Python")
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data == {"message": "Hello Python!"}
    print("PASS: JustAPITestClient works")


if __name__ == "__main__":
    test_server_starts_and_serves_requests()
    test_justapi_test_client_works()
    print("All tests PASSED")
