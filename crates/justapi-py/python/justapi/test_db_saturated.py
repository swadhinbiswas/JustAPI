"""Gap #2 regression: saturated DB pool fails fast with 503 (backpressure).

When every pooled connection is busy, a request that cannot acquire one within
`request_acquire_timeout` must be rejected immediately with `503 Service
Unavailable` — not hang until the (long) `acquire_timeout` elapses and then
return a generic 500. This guards the production failure mode where a momentary
connection crunch silently stalls the whole server for ~30s.

Uses a real HTTP server (the same dispatch path as production, including the
auto-transaction begin) so the saturation actually exercises `begin_request`.
"""
import os
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request

import pytest

from justapi import Database, JustAPIApp


def _server_script(db_path, port):
    """Return source that boots a server with a 1-conn, 1s fast-fail pool."""
    return (
        "import asyncio\n"
        "from justapi import Database, JustAPIApp\n"
        "app = JustAPIApp()\n"
        f'app.set_database(Database("sqlite://{db_path}", max_connections=1, request_acquire_timeout=1.0))\n'
        "\n"
        "@app.post('/slow')\n"
        "async def slow(request):\n"
        "    await asyncio.sleep(3)\n"
        "    return {'ok': True}\n"
        "\n"
        "@app.post('/other')\n"
        "async def other(request):\n"
        "    return {'ok': True}\n"
        "\n"
        f"app.run('127.0.0.1:{port}')\n"
    )


def _post(port, path, timeout=10):
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=b"{}",
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        return urllib.request.urlopen(req, timeout=timeout).status
    except urllib.error.HTTPError as e:
        return e.code


def test_saturated_pool_fast_fails_503():
    tmpdir = tempfile.mkdtemp()
    db_path = os.path.join(tmpdir, "svc_saturated.db")
    open(db_path, "w").close()
    port = 9887
    proc = None
    try:
        proc = subprocess.Popen(
            [sys.executable, "-c", _server_script(db_path, port)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        time.sleep(1.0)

        # /slow grabs the only connection and holds it 3s.
        slow_thread = threading.Thread(target=lambda: _post(port, "/slow", timeout=10))
        slow_thread.start()
        time.sleep(0.3)  # let /slow acquire the connection first

        start = time.monotonic()
        other_status = _post(port, "/other", timeout=10)
        elapsed = time.monotonic() - start

        slow_thread.join()

        assert other_status == 503, f"expected 503 under saturation, got {other_status}"
        # Fast-fail: must NOT wait the full 30s acquire_timeout.
        assert elapsed < 3.0, f"expected fast 503, but took {elapsed:.2f}s"
    finally:
        if proc is not None:
            proc.terminate()
            try:
                proc.communicate(timeout=5)
            except Exception:
                proc.kill()
        if os.path.exists(db_path):
            os.unlink(db_path)
        os.rmdir(tmpdir)
