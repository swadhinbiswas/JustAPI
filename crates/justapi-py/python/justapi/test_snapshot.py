"""Tests for snapshot testing support.
"""
import json
import os
import tempfile

import pytest
import pytest_asyncio
from justapi import JustAPIApp
from justapi.testing import Snapshot, AsyncTestClient, assert_ok


# ---------------------------------------------------------------------------
# Snapshot basics
# ---------------------------------------------------------------------------


def test_snapshot_assert_match_creates_file():
    with tempfile.TemporaryDirectory() as tmpdir:
        snap = Snapshot(snap_dir=tmpdir)
        snap.assert_match({"hello": "world"}, "test_match")

        snap_file = os.path.join(tmpdir, "test_match.snap")
        assert os.path.exists(snap_file)
        data = json.loads(open(snap_file).read())
        assert data == {"hello": "world"}


def test_snapshot_assert_match_passes_on_match():
    with tempfile.TemporaryDirectory() as tmpdir:
        snap = Snapshot(snap_dir=tmpdir)
        snap.assert_match({"a": 1}, "test_pass")
        # Second call should pass (same value)
        snap.assert_match({"a": 1}, "test_pass")


def test_snapshot_assert_match_fails_on_mismatch():
    with tempfile.TemporaryDirectory() as tmpdir:
        snap = Snapshot(snap_dir=tmpdir)
        snap.assert_match({"a": 1}, "test_fail")

        with pytest.raises(AssertionError, match="Snapshot mismatch"):
            snap.assert_match({"a": 2}, "test_fail")


def test_snapshot_auto_name():
    """When name is omitted, the calling test function name is used."""
    with tempfile.TemporaryDirectory() as tmpdir:
        snap = Snapshot(snap_dir=tmpdir)
        snap.assert_match("value")  # uses test_snapshot_auto_name as name

        snap_file = os.path.join(tmpdir, "test_snapshot_auto_name.snap")
        assert os.path.exists(snap_file)


def test_snapshot_update_via_env(monkeypatch):
    """SNAPSHOT_UPDATE=1 accepts any value without comparison."""
    monkeypatch.setenv("SNAPSHOT_UPDATE", "1")
    with tempfile.TemporaryDirectory() as tmpdir:
        snap = Snapshot(snap_dir=tmpdir)
        snap.assert_match({"v1": "old"}, "test_update_env")
        # Change the value and assert again — should pass because update is on
        snap.assert_match({"v2": "new"}, "test_update_env")
        # Verify the stored value is now the new one
        stored = json.loads(open(os.path.join(tmpdir, "test_update_env.snap")).read())
        assert stored == {"v2": "new"}


# ---------------------------------------------------------------------------
# Snapshot with response dicts
# ---------------------------------------------------------------------------


def test_snapshot_assert_response():
    with tempfile.TemporaryDirectory() as tmpdir:
        snap = Snapshot(snap_dir=tmpdir)
        resp = {
            "status": 200,
            "headers": {"content-type": "application/json"},
            "body": b'{"ok":true}',
        }
        snap.assert_response(resp, "test_resp")
        snap.assert_response(resp, "test_resp")  # same value passes

        snap.assert_response(resp, "test_resp2")


def test_snapshot_assert_response_fails_on_mismatch():
    with tempfile.TemporaryDirectory() as tmpdir:
        snap = Snapshot(snap_dir=tmpdir)
        snap.assert_response(
            {"status": 200, "headers": {}, "body": b'{"a":1}'}, "mismatch"
        )

        with pytest.raises(AssertionError):
            snap.assert_response(
                {"status": 200, "headers": {}, "body": b'{"a":2}'}, "mismatch"
            )


def test_snapshot_assert_body():
    with tempfile.TemporaryDirectory() as tmpdir:
        snap = Snapshot(snap_dir=tmpdir)
        snap.assert_body({"body": b"binary data \x00\xff"}, "body_test")
        snap.assert_body({"body": b"binary data \x00\xff"}, "body_test")


# ---------------------------------------------------------------------------
# Integration with AsyncTestClient
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_snapshot_with_test_client():
    app = JustAPIApp()
    app.get("/hello/{name}", lambda name: {"message": f"Hello {name}!"})
    app.post("/echo", lambda body: {"body": body})

    import tempfile

    with tempfile.TemporaryDirectory() as tmpdir:
        snap = Snapshot(snap_dir=tmpdir)

        async with AsyncTestClient(app) as c:
            resp = await c.get("/hello/world")
            snap.assert_response(resp, "hello_world")

            resp = await c.post("/echo", b'{"x":1}')
            snap.assert_response(resp, "echo_body")

        snap_file = os.path.join(tmpdir, "hello_world.snap")
        assert os.path.exists(snap_file)
        data = json.loads(open(snap_file).read())
        assert data["status"] == 200
        import base64
        body = json.loads(base64.b64decode(data["body_base64"]))
        assert body == {"message": "Hello world!"}


@pytest.mark.asyncio
async def test_snapshot_default_dir():
    """When no snap_dir is given, snapshots go to __snapshots__/ next to test file."""
    snap = Snapshot()
    async with AsyncTestClient(JustAPIApp()) as c:
        pass  # just testing setup, not actual request
    snap.assert_match("test", "test_snapshot_default_dir")
    expected = os.path.join(
        os.path.dirname(__file__), "__snapshots__", "test_snapshot_default_dir.snap"
    )
    assert os.path.exists(expected)
    os.unlink(expected)
    os.rmdir(os.path.join(os.path.dirname(__file__), "__snapshots__"))
