"""`status_code=` on the route decorator is applied to the response.

FastAPI parity: the decorator kwarg sets the *actual* status for plain
dict/list/scalar returns — it is not just OpenAPI metadata. Explicit
`Response` objects, envelope dicts and streaming responses keep their own
status (they win over the decorator default).

Covers both pipelines: the in-process test client and a real server
(subprocess), since they use different native handlers.
"""

from __future__ import annotations

import json
import socket
import subprocess
import sys
import time
import urllib.request

from justapi import JSONResponse, JustAPIApp, JustAPITestClient

app = JustAPIApp()


@app.post("/items", status_code=201)
def create_item():
    return {"created": True}


@app.get("/items", status_code=202)
async def list_items():
    return [1, 2, 3]


@app.delete("/items", status_code=204)
def delete_item():
    return {"status": 204}


@app.get("/custom", status_code=202)
def custom():
    return JSONResponse({"ok": True}, status_code=207)


@app.get("/data", status_code=201)
def data():
    return {"status": "ok", "n": 1}


@app.get("/plain")
def plain():
    return {"ok": True}


client = JustAPITestClient(app)


def _body(resp):
    raw = bytes(resp["body"])
    return json.loads(raw) if raw else None


def test_decorator_status_applies_to_dict_return():
    resp = client.post("/items", b"")
    assert resp["status"] == 201
    assert _body(resp) == {"created": True}


def test_decorator_status_applies_to_async_list_return():
    resp = client.get("/items")
    assert resp["status"] == 202
    assert _body(resp) == [1, 2, 3]


def test_envelope_status_wins_over_decorator():
    resp = client.delete("/items")
    assert resp["status"] == 204
    assert bytes(resp["body"]) == b""


def test_response_object_status_wins_over_decorator():
    resp = client.get("/custom")
    assert resp["status"] == 207
    assert _body(resp) == {"ok": True}


def test_data_dict_with_status_key_keeps_body():
    """A data dict that merely contains a `status` key is not an envelope."""
    resp = client.get("/data")
    assert resp["status"] == 201
    assert _body(resp) == {"status": "ok", "n": 1}


def test_default_status_is_200():
    assert client.get("/plain")["status"] == 200


# ---------------------------------------------------------------------------
# Real server pipeline (subprocess): same behavior through `app.run()`.
# ---------------------------------------------------------------------------

LIVE_SERVER = """
import sys
from justapi import JustAPIApp

app = JustAPIApp()

@app.post("/items", status_code=201)
def create():
    return {"created": True}

@app.get("/plain")
def plain():
    return {"ok": True}

app.run(f"127.0.0.1:{sys.argv[1]}")
"""


def _free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def test_status_code_on_the_real_server():
    port = _free_port()
    proc = subprocess.Popen(
        [sys.executable, "-c", LIVE_SERVER, str(port)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        deadline = time.time() + 20
        while True:
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/plain") as r:
                    assert r.status == 200
                break
            except Exception:
                if time.time() > deadline:
                    raise AssertionError("server did not become ready")
                time.sleep(0.2)

        req = urllib.request.Request(
            f"http://127.0.0.1:{port}/items", data=b"", method="POST"
        )
        with urllib.request.urlopen(req) as r:
            assert r.status == 201
            assert json.loads(r.read()) == {"created": True}
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
