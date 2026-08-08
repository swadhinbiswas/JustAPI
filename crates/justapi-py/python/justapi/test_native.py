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


def test_native_async_marker():
    """@native_async marks a handler; it works like any async handler but is
    flagged for the fastest dispatch (parallel on free-threaded builds, ADR-089)."""
    import asyncio
    from justapi import JustAPIApp, native_async, JustAPITestClient

    app = JustAPIApp()

    @app.get("/x")
    @native_async
    async def handler(request):
        await asyncio.sleep(0.001)
        return {"ok": True, "path": request.get("path")}

    r = JustAPITestClient(app).get("/x")
    assert r["status"] == 200
    assert b"ok" in bytes(r["body"])
    # The decorator marks the raw function; the wrapper propagation is
    # verified by the Rust dispatch using the marker (integration-tested).
    assert getattr(handler, "__native_async__", False) is True


def test_native_async_db_query_and_execute():
    """query_async / execute_async: awaitable native DB ops (ADR-093).

    Runs on a file-based SQLite (in-memory gives each pooled connection its
    own database, which is a test artifact, not a framework bug). Verifies
    both the async query and async write paths return correct results."""
    import os
    import tempfile
    import asyncio
    from justapi import JustAPIApp, JustAPITestClient

    db_path = os.path.join(tempfile.mkdtemp(), "native_async.db")
    app = JustAPIApp()
    app.set_database(f"sqlite://{db_path}")
    app.connect_database()
    app.db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
    app.db.execute("INSERT INTO users (name) VALUES ('alice'), ('bob')")

    @app.get("/q")
    async def q(req):
        return await app.db.query_async("SELECT * FROM users ORDER BY id")

    @app.get("/w")
    async def w(req):
        n = await app.db.execute_async("INSERT INTO users (name) VALUES ('carol')")
        return {"affected": n}

    c = JustAPITestClient(app)
    r1 = c.get("/q")
    r2 = c.get("/w")
    assert r1["status"] == 200, r1
    assert r2["status"] == 200, r2
    rows = json.loads(r1["body"])
    assert [row["name"] for row in rows] == ["alice", "bob"]
    assert json.loads(r2["body"]) == {"affected": 1}

    # The write actually persisted (read back through the async path).
    r3 = c.get("/q")
    rows3 = json.loads(r3["body"])
    assert [row["name"] for row in rows3] == ["alice", "bob", "carol"]


def test_native_async_db_concurrent_through_http():
    """Many concurrent query_async requests all succeed (parallel DB ops)."""
    import os
    import tempfile
    import concurrent.futures
    from justapi import JustAPIApp, JustAPITestClient

    db_path = os.path.join(tempfile.mkdtemp(), "native_async_conc.db")
    app = JustAPIApp()
    app.set_database(f"sqlite://{db_path}")
    app.connect_database()
    app.db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)")
    app.db.execute("INSERT INTO t (v) VALUES ('x')")

    @app.get("/native")
    async def native(req):
        return await app.db.query_async("SELECT count(*) AS c FROM t")

    c = JustAPITestClient(app)
    with concurrent.futures.ThreadPoolExecutor(max_workers=16) as ex:
        futs = [ex.submit(c.get, "/native") for _ in range(40)]
        results = [f.result() for f in futs]
    assert all(r["status"] == 200 for r in results), results


def test_run_defaults_to_localhost_8000():
    """app.run() with no args binds 127.0.0.1:8000 (README quickstart)."""
    import threading
    import time
    import urllib.request
    from justapi import JustAPIApp

    app = JustAPIApp()

    @app.get("/")
    def root():
        return {"Hello": "World"}

    t = threading.Thread(target=app.run, daemon=True)
    t.start()
    try:
        deadline = time.time() + 15
        last = None
        while time.time() < deadline:
            try:
                last = urllib.request.urlopen("http://127.0.0.1:8000/", timeout=2)
                break
            except Exception as e:
                last = e
                time.sleep(0.3)
        assert last is not None and getattr(last, "status", None) == 200, f"server not reachable: {last}"
    finally:
        import socket
        s = socket.socket()
        try:
            s.connect(("127.0.0.1", 8000))
            s.close()
        except Exception:
            pass


def test_pydantic_model_as_body_schema():
    """Pydantic v2 models work as body_schema (validated via their JSON
    Schema in the Rust engine) — FastAPI-migration path (fixed 2026-08-08)."""
    from pydantic import BaseModel, Field
    from justapi import JustAPIApp, JustAPITestClient

    class Pet(BaseModel):
        name: str = Field(..., min_length=1)
        species: str

    app = JustAPIApp()

    @app.post("/pets", body_schema=Pet)
    def create(request):
        body = request.json()
        return {"name": body["name"], "species": body["species"]}

    c = JustAPITestClient(app)
    ok = c.post("/pets", b'{"name": "rex", "species": "dog"}')
    empty = c.post("/pets", b'{"name": "", "species": "dog"}')
    wrong = c.post("/pets", b'{"name": 123, "species": "dog"}')
    assert ok["status"] == 200, ok
    assert empty["status"] == 422, empty
    assert wrong["status"] == 422, wrong
