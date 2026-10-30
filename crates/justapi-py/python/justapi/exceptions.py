"""FastAPI-compatible exception types.

These mirror Starlette/FastAPI semantics:
- ``HTTPException`` carries a ``status_code``, a ``detail`` (any JSON-able
  value, usually a string), and optional ``headers``. The Rust request
  handler intercepts raised ``HTTPException`` instances and turns them into a
  proper HTTP response (``{"detail": ...}`` body, correct status, headers).
- ``WebSocketException`` carries a close ``code`` and ``reason``. The
  ``@app.websocket`` wrapper intercepts it and closes the socket with those
  values.
"""

from typing import Any, Dict, Optional


class HTTPException(Exception):
    def __init__(
        self,
        status_code: int,
        detail: Any = None,
        headers: Optional[Dict[str, str]] = None,
    ) -> None:
        super().__init__(detail)
        self.status_code = status_code
        self.detail = detail
        self.headers = headers


class DatabaseUnavailable(RuntimeError):
    """The database is unreachable or the pool is saturated (retryable).

    Raised by ``app.db.*`` when a connection cannot be acquired within the
    request window (``request_acquire_timeout``, 3s by default) or the server
    refuses/resets the connection. The request layer renders it as
    ``503 Service Unavailable`` with a ``Retry-After`` header instead of a
    generic 500, so load balancers and clients can back off and retry.

    Handlers may catch it to degrade gracefully::

        try:
            rows = app.db.query(...)
        except DatabaseUnavailable:
            return {"cached": True}
    """


class WebSocketException(Exception):
    def __init__(self, code: int = 1000, reason: Optional[str] = None) -> None:
        super().__init__(reason)
        self.code = code
        self.reason = reason
