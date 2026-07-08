from _typeshed import Incomplete
from typing import Any, Mapping, Optional
from ._justapi import TokenStreamResponse

class Response:
    media_type: Incomplete
    charset: str
    status_code: Incomplete
    background: Incomplete
    body: Incomplete
    headers: Incomplete
    def __init__(self, content: Any = None, status_code: int = 200, headers: Mapping[str, str] | None = None, media_type: str | None = None) -> None: ...
    def render(self, content: Any) -> bytes: ...
    def init_headers(self, headers: Mapping[str, str] | None = None) -> list: ...
    def to_dict(self): ...

class HTMLResponse(Response):
    media_type: str

class PlainTextResponse(Response):
    media_type: str

class JSONResponse(Response):
    media_type: str
    def render(self, content: Any) -> bytes: ...

class RedirectResponse(Response):
    def __init__(self, url: str, status_code: int = 307, headers: Optional[Mapping[str, str]] = None) -> None: ...

class StreamingResponse(TokenStreamResponse):
    def __init__(
        self,
        content: Any,
        status_code: int = 200,
        headers: Optional[Mapping[str, str]] = None,
        media_type: Optional[str] = None,
    ) -> None: ...
