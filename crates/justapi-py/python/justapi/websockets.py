"""FastAPI/Starlette-compatible WebSocket support.

Mirrors ``starlette.websockets``: ``WebSocket`` exposes the connection scope
(``headers``, ``query_params``, ``path_params``, ``cookies``, ``client``,
``url``, ``app``, ``state``), the async send/receive surface
(``accept``, ``receive_text``, ``receive_bytes``, ``receive_json``,
``send_text``, ``send_bytes``, ``send_json``, ``close``, ``url_for``) plus the
iterator helpers (``iter_text``, ``iter_bytes``, ``iter_json``), and the
``WebSocketState`` enum and ``WebSocketDisconnect`` exception.
"""

from enum import IntEnum

from ._justapi import WebSocket as _RustWebSocket


class WebSocketState(IntEnum):
    CONNECTING = 0
    CONNECTED = 1
    DISCONNECTED = 2
    RESPONSE = 3


class WebSocketDisconnect(Exception):
    def __init__(self, code: int = 1000, reason: str = None):
        self.code = code
        self.reason = reason or ""
        super().__init__(f"WebSocketDisconnect(code={code}, reason={reason!r})")


class WebSocket:
    """Python wrapper around the Rust ``_justapi.WebSocket``.

    Adds the async-generator ``iter_*`` helpers (which the Rust layer cannot
    express directly) and translates disconnects into ``WebSocketDisconnect``.
    All other members delegate to the underlying Rust object.
    """

    def __init__(self, ws: "_RustWebSocket"):
        self._ws = ws

    # --- connection scope (delegated) ---
    @property
    def app(self):
        return self._ws.app

    @property
    def url(self):
        return self._ws.url

    @property
    def base_url(self):
        return self._ws.base_url

    @property
    def headers(self):
        return self._ws.headers

    @property
    def query_params(self):
        return self._ws.query_params

    @property
    def path_params(self):
        return self._ws.path_params

    @property
    def cookies(self):
        return self._ws.cookies

    @property
    def client(self):
        return self._ws.client

    @property
    def state(self):
        return self._ws.state

    @property
    def client_state(self):
        return self._ws.client_state

    @property
    def application_state(self):
        return self._ws.application_state

    @property
    def subprotocol(self):
        return self._ws.subprotocol

    # --- handshake / lifecycle (delegated) ---
    async def accept(self, subprotocol=None, headers=None):
        return await self._ws.accept(subprotocol=subprotocol, headers=headers)

    async def close(self, code=None, reason=None):
        return await self._ws.close(code=code, reason=reason)

    def url_for(self, name, **path_params):
        return self._ws.url_for(name, **path_params)

    # --- receive (delegated, translating disconnects) ---
    async def receive(self):
        return await self._ws.receive()

    async def receive_text(self):
        try:
            return await self._ws.receive_text()
        except EOFError:
            raise WebSocketDisconnect()

    async def receive_bytes(self):
        try:
            return await self._ws.receive_bytes()
        except EOFError:
            raise WebSocketDisconnect()

    async def receive_json(self, mode="text"):
        try:
            return await self._ws.receive_json(mode=mode)
        except EOFError:
            raise WebSocketDisconnect()

    # --- send (delegated) ---
    async def send(self, message):
        return await self._ws.send(message)

    async def send_text(self, message):
        return await self._ws.send_text(message)

    async def send_bytes(self, message):
        return await self._ws.send_bytes(message)

    async def send_json(self, data, mode="text"):
        return await self._ws.send_json(data, mode=mode)

    # --- iterators (async iterables) ---
    def iter_text(self):
        return _WsIterator(self, "text")

    def iter_bytes(self):
        return _WsIterator(self, "bytes")

    def iter_json(self):
        return _WsIterator(self, "json")


class _WsIterator:
    """Async iterator over inbound WebSocket messages.

    Implemented as a regular async-iterable (not an ``async def`` generator)
    because pyo3-async-runtimes coroutines do not suspend correctly inside
    native Python async generators; a plain ``__anext__`` coroutine mirrors the
    manual ``await ws.receive_text()`` loop that works.
    """

    def __init__(self, ws: "WebSocket", kind: str):
        self._ws = ws
        self._kind = kind

    def __aiter__(self):
        return self

    async def __anext__(self):
        try:
            if self._kind == "text":
                return await self._ws.receive_text()
            if self._kind == "bytes":
                return await self._ws.receive_bytes()
            return await self._ws.receive_json()
        except WebSocketDisconnect:
            raise StopAsyncIteration
