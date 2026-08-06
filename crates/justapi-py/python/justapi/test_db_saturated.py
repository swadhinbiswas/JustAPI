"""Gap #2 regression: DB contention fails fast with 503 (backpressure).

When the database is contended (SQLite write lock held, or the pool saturated),
a request must fail with `503 Service Unavailable` — not hang until the long
`acquire_timeout` elapses and then return a generic 500. This guards the
production failure mode where a momentary connection crunch silently stalls the
whole server for ~30s.

The DB is contended by holding the SQLite **write lock** from an external
connection (a legitimate, reproducible saturation: SQLite serializes writers),
then hammering the Rust-native CRUD INSERT path. The write operation waits up
to `busy_timeout` (5s) for the lock, then the framework must map the lock
failure to `503 Retry-After`, bounded — never the 30s `acquire_timeout` stall
(see ADR-080).
"""
import os
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

import pytest


def _server_script(db_path, port):
    """Return source that boots a server with a 1-conn, 1s fast-fail pool."""
    return (
        "from justapi import Database, JustAPIApp\n"
        "app = JustAPIApp()\n"
        f'app.set_database(Database("sqlite://{db_path}", max_connections=1, request_acquire_timeout=1.0))\n'
        "app.post('/items', crud_table='items', crud_columns=['name', 'qty'])\n"
        f"app.run('127.0.0.1:{port}')\n"
    )


def _post(port, path, timeout=10):
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=b'{"name":"w","qty":1}',
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
    con = sqlite3.connect(db_path)
    con.execute(
        "CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, qty INTEGER NOT NULL)"
    )
    con.commit()

    port = 9887
    proc = None
    lock_conn = None
    try:
        proc = subprocess.Popen(
            [sys.executable, "-c", _server_script(db_path, port)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        time.sleep(1.0)

        # Hold the SQLite write lock from an external connection: the server's
        # single pooled connection can then never begin a write transaction, so
        # every acquire is doomed — this is a genuine, reproducible saturation.
        lock_conn = sqlite3.connect(db_path, timeout=10)
        lock_conn.execute("BEGIN IMMEDIATE")
        time.sleep(0.3)  # let the lock take effect

        start = time.monotonic()
        other_status = _post(port, "/items", timeout=15)
        elapsed = time.monotonic() - start

        lock_conn.rollback()
        lock_conn.close()
        lock_conn = None

        assert other_status == 503, f"expected 503 under saturation, got {other_status}"
        # Bounded: must fail within the busy_timeout window (5s), NOT hang on
        # the 30s acquire_timeout.
        assert elapsed < 10.0, f"expected bounded 503, but took {elapsed:.2f}s"
    finally:
        if lock_conn is not None:
            try:
                lock_conn.close()
            except Exception:
                pass
        if proc is not None:
            proc.terminate()
