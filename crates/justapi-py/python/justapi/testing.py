"""JustAPI testing utilities — async fixtures, DB helpers, and assertions.

.. warning::
   Test functions **must not** have the same name as imported helpers.
   E.g. ``from justapi.testing import test_db`` then ``def test_db():``
   will cause a name collision — rename your test to ``test_my_db:`` instead.



Usage:
    import pytest
    import pytest_asyncio
    from justapi import JustAPIApp
    from justapi.testing import AsyncTestClient, assert_ok, assert_json

    @pytest_asyncio.fixture
    async def app():
        app = JustAPIApp()
        app.get("/ping", lambda r: {"pong": True})
        return app

    @pytest_asyncio.fixture
    async def client(app):
        async with AsyncTestClient(app) as c:
            yield c

    @pytest.mark.asyncio
    async def test_ping(client):
        resp = await client.get("/ping")
        assert_ok(resp)
        assert_json(resp, {"pong": True})

    # With database:
    @pytest_asyncio.fixture
    async def db_client(app):
        async with AsyncTestClient(app, database="sqlite::memory:") as c:
            yield c
"""

import asyncio
import inspect
import json
import os
import subprocess
import sys
import tempfile
from functools import partial
from typing import Any, Awaitable, Callable, Optional


class AsyncTestClient:
    """Async wrapper around *JustAPITestClient* for use in ``pytest-asyncio`` fixtures.

    Use as an async context manager:

        async with AsyncTestClient(app) as client:
            resp = await client.get("/hello")

    To test database-backed routes, pass the *database* URL:

        async with AsyncTestClient(app, database="sqlite::memory:") as client:
            resp = await client.post("/users", b'{"name":"Alice"}')
    """

    def __init__(self, app: "Any", database: Optional[str] = None) -> None:
        self._app = app
        self._database = database
        self._client: Optional["Any"] = None
        self._setup_hooks: list[Callable[["AsyncTestClient"], Awaitable[None]]] = []
        self._teardown_hooks: list[Callable[["AsyncTestClient"], Awaitable[None]]] = []

    async def _build_client(self) -> None:
        from justapi import JustAPITestClient
        
        inner_app = getattr(self._app, "_inner", self._app)
        self._client = JustAPITestClient(inner_app, database=self._database)

    async def _destroy_client(self) -> None:
        self._client = None

    async def __aenter__(self) -> "AsyncTestClient":
        await self._build_client()
        for hook in self._setup_hooks:
            res = hook(self)
            if res is not None:
                await res
        return self

    async def __aexit__(self, *exc_info: Any) -> None:
        for hook in reversed(self._teardown_hooks):
            res = hook(self)
            if res is not None:
                await res
        await self._destroy_client()

    def on_setup(self, hook: Callable[["AsyncTestClient"], Awaitable[None]]) -> "AsyncTestClient":
        self._setup_hooks.append(hook)
        return self

    def on_teardown(self, hook: Callable[["AsyncTestClient"], Awaitable[None]]) -> "AsyncTestClient":
        self._teardown_hooks.append(hook)
        return self

    async def _run(self, method: str, path: str, body: bytes = b"") -> dict:
        client = self._client
        if client is None:
            raise RuntimeError("AsyncTestClient not initialized")

        loop = asyncio.get_running_loop()

        if method in ("GET", "DELETE"):
            fn = partial(getattr(client, method.lower()), path)
        else:
            fn = partial(getattr(client, method.lower()), path, body)

        return await loop.run_in_executor(None, fn)

    async def get(self, path: str) -> dict:
        return await self._run("GET", path)

    async def post(self, path: str, body: bytes = b"") -> dict:
        return await self._run("POST", path, body)

    async def put(self, path: str, body: bytes = b"") -> dict:
        return await self._run("PUT", path, body)

    async def patch(self, path: str, body: bytes = b"") -> dict:
        return await self._run("PATCH", path, body)

    async def delete(self, path: str) -> dict:
        return await self._run("DELETE", path)

    async def query(self, path: str, body: bytes = b"") -> dict:
        client = self._client
        if client is None:
            raise RuntimeError("AsyncTestClient not initialized")
        loop = asyncio.get_running_loop()
        # RFC 10008 requires a Content-Type on QUERY requests.
        fn = partial(client.query_with, path, body, [("Content-Type", "application/json")])
        return await loop.run_in_executor(None, fn)


# ---------------------------------------------------------------------------
# Database test helpers
# ---------------------------------------------------------------------------


class ManagedDb:
    """Managed test database with automatic cleanup.

    Use as an async context manager:

        async with ManagedDb(url="sqlite://:memory:") as db:
            app.set_database(db.url)
            # run routes against in-memory SQLite
    """

    def __init__(self, url: str = "sqlite://:memory:", migrations_dir: Optional[str] = None):
        self._url = url
        self._migrations_dir = migrations_dir
        self._cleanup_paths: list[str] = []

    @property
    def url(self) -> str:
        return self._url

    async def run_migrations(self, directory: Optional[str] = None) -> None:
        """Run SQL migrations against the test database using sqlx CLI."""
        migrations = directory or self._migrations_dir
        if not migrations:
            return
        if not os.path.isdir(migrations):
            raise FileNotFoundError(f"Migrations directory not found: {migrations}")

        loop = asyncio.get_running_loop()

        def _run():
            result = subprocess.run(
                [sys.executable, "-m", "sqlx", "migrate", "run",
                 "--database-url", self._url, "--source", migrations],
                capture_output=True, text=True, timeout=30,
            )
            if result.returncode != 0:
                raise RuntimeError(
                    f"Migration failed: {result.stderr.strip() or result.stdout.strip()}"
                )

        await loop.run_in_executor(None, _run)

    async def run_sql(self, sql: str) -> None:
        """Execute raw SQL against the test database.

        For file-based databases (``sqlite:///path/to/db.db``), runs SQL
        against the same file.  For in-memory databases (``sqlite://:memory:``),
        this opens a *separate* in-memory database — use only for schema
        metadata that doesn't need to persist across connections.
        """
        import sqlite3

        path = _sqlite_path(self._url)

        loop = asyncio.get_running_loop()

        def _run():
            conn = sqlite3.connect(path)
            try:
                conn.executescript(sql)
                conn.commit()
            finally:
                conn.close()

        await loop.run_in_executor(None, _run)

    async def __aenter__(self) -> "ManagedDb":
        return self

    async def __aexit__(self, *exc_info: Any) -> None:
        for path in self._cleanup_paths:
            if os.path.exists(path):
                os.unlink(path)


def test_db(url: str = "sqlite://:memory:", migrations_dir: Optional[str] = None) -> ManagedDb:
    """Create a *ManagedDb* instance for use in pytest fixtures.

    Usage:
        @pytest_asyncio.fixture
        async def db():
            async with test_db() as db:
                await db.run_sql("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)")
                yield db
    """
    return ManagedDb(url=url, migrations_dir=migrations_dir)


# ---------------------------------------------------------------------------
# DB fixture helper — configure app with test database and return AsyncTestClient
# ---------------------------------------------------------------------------


async def db_client(
    app: "Any",
    url: str = "sqlite://:memory:",
    *,
    seed_sql: Optional[str] = None,
) -> AsyncTestClient:
    """Create an *AsyncTestClient* configured with a test database.

    Usage:
        @pytest_asyncio.fixture
        async def client(app):
            async with await db_client(app, seed_sql="CREATE TABLE ...") as c:
                yield c
    """
    if seed_sql:
        import sqlite3

        db_path = _sqlite_path(url)
        conn = sqlite3.connect(db_path)
        try:
            conn.executescript(seed_sql)
            conn.commit()
        finally:
            conn.close()

    c = AsyncTestClient(app, database=url)
    await c._build_client()
    return c


# ---------------------------------------------------------------------------
# Snapshot testing
# ---------------------------------------------------------------------------


class Snapshot:
    """Response body snapshot assertions.

    Snapshots are stored as JSON files under ``__snapshots__/`` alongside
    the test file.  The first run creates the snapshot; subsequent runs
    compare the response against it.

    Usage:
        from justapi.testing import Snapshot

        snapshot = Snapshot()

        async def test_get_users(client):
            resp = await client.get("/users")
            snapshot.assert_response(resp, "users_list")

    To update all snapshots to match new output::

        SNAPSHOT_UPDATE=1 python -m pytest ...
    """

    def __init__(self, snap_dir: Optional[str] = None):
        self._snap_dir = snap_dir
        self._update = os.environ.get("SNAPSHOT_UPDATE", "").lower() in (
            "1",
            "true",
            "yes",
        )

    # -- Public API -------------------------------------------------------

    def assert_response(self, resp: dict, name: Optional[str] = None) -> None:
        """Assert that a response dict (status + headers + body) matches its
        stored snapshot."""
        path = self._resolve_path(name)
        current = _serialize_response(resp)
        self._assert_match(path, current)

    def assert_match(self, value: Any, name: Optional[str] = None) -> None:
        """Assert that an arbitrary JSON-serialisable value matches its
        stored snapshot."""
        path = self._resolve_path(name)
        current = json.dumps(value, indent=2, default=str, ensure_ascii=False)
        self._assert_match(path, current)

    def assert_body(self, resp: dict, name: Optional[str] = None) -> None:
        """Assert that the raw response body (as bytes) matches its stored
        snapshot (stored as base64)."""
        path = self._resolve_path(name)
        import base64

        current = base64.b64encode(bytes(resp["body"])).decode("ascii")
        self._assert_match(path, current)

    # -- Internals --------------------------------------------------------

    def _resolve_path(self, name: Optional[str] = None) -> str:
        # Walk up the stack to find the first frame outside testing.py
        frame = inspect.currentframe()
        while frame:
            mod_file = frame.f_globals.get("__file__", "")
            if not mod_file.endswith("testing.py"):
                break
            frame = frame.f_back

        if name is not None:
            snap_name = name
        else:
            snap_name = frame.f_code.co_name if frame else "unnamed"

        if self._snap_dir:
            base = self._snap_dir
        else:
            caller_file = frame.f_globals.get("__file__", ".") if frame else "."
            base = os.path.join(os.path.dirname(caller_file), "__snapshots__")

        os.makedirs(base, exist_ok=True)
        return os.path.join(base, f"{snap_name}.snap")

    def _assert_match(self, path: str, current: str) -> None:
        if self._update:
            self._write(path, current)
            return

        if not os.path.exists(path):
            self._write(path, current)
            return

        stored = self._read(path)
        if stored != current:
            msg_lines = [
                f"Snapshot mismatch: {path}",
                "",
                "--- stored",
                "+++ current",
            ]
            for diff in _diff_lines(stored, current):
                msg_lines.append(diff)
            msg_lines.append("")
            msg_lines.append("Run with SNAPSHOT_UPDATE=1 to update.")
            raise AssertionError("\n".join(msg_lines))

    @staticmethod
    def _write(path: str, content: str) -> None:
        with open(path, "w") as f:
            f.write(content)
            f.write("\n")

    @staticmethod
    def _read(path: str) -> str:
        with open(path) as f:
            return f.read().rstrip("\n")


def _serialize_response(resp: dict) -> str:
    import base64

    body_b64 = base64.b64encode(bytes(resp.get("body", b""))).decode("ascii")
    headers = {}
    for k, v in resp.get("headers", {}).items():
        headers[k] = v
    return json.dumps(
        {
            "status": resp.get("status", 200),
            "headers": headers,
            "body_base64": body_b64,
        },
        indent=2,
        ensure_ascii=False,
    )


def _diff_lines(a: str, b: str) -> list[str]:
    """Simple line-by-line diff between two strings."""
    import difflib

    return list(
        difflib.unified_diff(
            a.splitlines(keepends=True),
            b.splitlines(keepends=True),
            fromfile="stored",
            tofile="current",
            lineterm="",
        )
    )


# ---------------------------------------------------------------------------
# Assertion helpers
# ---------------------------------------------------------------------------


def assert_ok(resp: dict) -> None:
    assert resp["status"] == 200, (
        f"Expected 200, got {resp['status']}: {bytes(resp.get('body', b'')).decode(errors='replace')}"
    )


def assert_status(resp: dict, status: int) -> None:
    assert resp["status"] == status, (
        f"Expected {status}, got {resp['status']}: {bytes(resp.get('body', b'')).decode(errors='replace')}"
    )


def assert_json(resp: dict, expected: Any) -> None:
    body = json.loads(bytes(resp["body"]))
    assert body == expected, f"Expected {expected!r}, got {body!r}"


def assert_header(resp: dict, name: str, expected: Optional[str] = None) -> None:
    headers = {k.lower(): v for k, v in resp.get("headers", {}).items()}
    if expected is not None:
        value = headers.get(name.lower())
        assert value == expected, f"Expected header {name}={expected!r}, got {value!r}"
    else:
        assert name.lower() in headers, f"Expected header {name!r} not present"


def transaction_test_db(url: str = "sqlite://:memory:") -> str:
    """Return a database URL suitable for testing (in-memory SQLite by default).

    Usage:
        app.set_database(transaction_test_db())
    """
    return url


def _sqlite_path(url: str) -> str:
    """Extract the file path from a ``sqlite://`` URL for use with ``sqlite3.connect``."""
    if url == "sqlite://:memory:" or url == "sqlite://?mode=memory":
        return ":memory:"
    if url.startswith("sqlite://"):
        path = url[len("sqlite://"):]
        # Strip query parameters
        if "?" in path:
            path = path.split("?")[0]
        return path
    return url


__all__ = [
    "AsyncTestClient",
    "ManagedDb",
    "Snapshot",
    "test_db",
    "db_client",
    "assert_ok",
    "assert_status",
    "assert_json",
    "assert_header",
    "transaction_test_db",
]
