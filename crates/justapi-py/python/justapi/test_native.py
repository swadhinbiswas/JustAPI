"""Integration test for Tier B Native API (JustAPIApp).
"""
import concurrent.futures
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

# A synchronous (GIL-bound) handler: under CPython only one worker can run it
# at a time, so a burst of requests exercises the GIL pool's bounded dispatch
# queue (capacity 16 per worker). Before the backpressure fix, overflowing that
# queue surfaced as spurious RFC 9457 404s with no log (try_send rejection
# masked by handle_request). This handler keeps the pool busy long enough that
# many requests accumulate concurrently.
def sync_handler(request):
    import time as _t
    _t.sleep(0.0005)
    return {"message": "sync"}

app = JustAPIApp()
app.get("/hello/{name}", hello_handler)
app.post("/echo", echo_handler)
app.get("/sync", sync_handler)
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


def test_sync_handler_no_spurious_404_under_load():
    """Regression: a burst of concurrent requests to a GIL-bound (sync)
    Python handler must not produce spurious 404s.

    The GIL pool dispatches jobs through a bounded channel (cap 16/worker for a
    single GIL worker). Dispatch used `try_send`, so any overflow surfaced as a
    handler error that `handle_request` masked as an RFC 9457 404. With
    `send().await` backpressure the pool throttles instead of dropping, and the
    handler-chain error branch now returns 500 instead of a fake 404.
    """
    proc = subprocess.Popen(
        [sys.executable, "-c", SERVER_SCRIPT],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    addr = "127.0.0.1:9867"
    time.sleep(0.8)

    def fire(_):
        try:
            resp = urllib.request.urlopen(f"http://{addr}/sync", timeout=10)
            return resp.status
        except urllib.error.HTTPError as e:
            return e.code
        except Exception:
            return -1

    try:
        # 64 threads >> GIL pool capacity of 16, forcing queue contention.
        with concurrent.futures.ThreadPoolExecutor(max_workers=32) as ex:
            statuses = list(ex.map(fire, range(512)))
        bad = [s for s in statuses if s != 200]
        assert not bad, f"Expected all 200, got non-200 statuses: {bad}"
        print("PASS: 512 concurrent /sync requests -> all 200 (no spurious 404)")
    finally:
        proc.terminate()
        try:
            proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.communicate()


if __name__ == "__main__":
    test_native_api()
    test_sync_handler_no_spurious_404_under_load()


def test_justapp_alias_is_justapiapp():
    """JustAPP is an alias of JustAPIApp (and so is JustAPI)."""
    from justapi import JustAPIApp, JustAPP, JustAPI

    assert JustAPP is JustAPIApp
    assert JustAPI is JustAPIApp

    app = JustAPP()

    @app.get("/")
    def root():
        return {"ok": True}

    from justapi import JustAPITestClient

    resp = JustAPITestClient(app).get("/")
    assert resp["status"] == 200
