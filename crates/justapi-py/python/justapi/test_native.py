"""Integration test for Tier B Native API (JustAPIApp).
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
app.run("127.0.0.1:9867")
"""


def test_native_api():
    proc = subprocess.Popen(
        [sys.executable, "-c", SERVER_SCRIPT],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    addr = "127.0.0.1:9867"
    time.sleep(0.8)

    try:
        resp = urllib.request.urlopen(f"http://{addr}/hello/world")
        assert resp.status == 200, f"Expected 200, got {resp.status}"
        data = json.loads(resp.read())
        assert data == {"message": "Hello world!"}, f"Unexpected: {data}"
        print("PASS: /hello/world ->", data)

        resp = urllib.request.urlopen(f"http://{addr}/hello/JustAPI")
        assert resp.status == 200
        data = json.loads(resp.read())
        assert data == {"message": "Hello JustAPI!"}, f"Unexpected: {data}"
        print("PASS: /hello/JustAPI ->", data)

        try:
            urllib.request.urlopen(f"http://{addr}/nonexistent")
            assert False, "Expected 404"
        except urllib.error.HTTPError as e:
            assert e.code == 404, f"Expected 404, got {e.code}"
            print("PASS: /nonexistent -> 404")

        payload = b'{"hello":"world"}'
        req = urllib.request.Request(
            f"http://{addr}/echo",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        resp = urllib.request.urlopen(req)
        assert resp.status == 200, f"Expected 200, got {resp.status}"
        echo_body = resp.read()
        assert echo_body == payload, f"Echo mismatch: {echo_body} != {payload}"
        print("PASS: POST /echo -> body echoed")

        try:
            urllib.request.urlopen(f"http://{addr}/echo")
            assert False, "Expected 405 (wrong method)"
        except urllib.error.HTTPError as e:
            assert e.code == 405, f"Expected 405, got {e.code}"
            print("PASS: GET /echo -> 405 (wrong method)")

        print()
        print("=== All tests PASSED ===")

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
    test_native_api()
