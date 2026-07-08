import json
import urllib.parse
from typing import Any, Mapping, Optional, Union
from ._justapi import TokenStreamResponse

class Response:
    media_type = None
    charset = "utf-8"

    def __init__(
        self,
        content: Any = None,
        status_code: int = 200,
        headers: Optional[Mapping[str, str]] = None,
        media_type: Optional[str] = None,
    ) -> None:
        self.status_code = status_code
        if media_type is not None:
            self.media_type = media_type
        self.background = None
        self.body = self.render(content)
        self.headers = self.init_headers(headers)

    def render(self, content: Any) -> bytes:
        if content is None:
            return b""
        if isinstance(content, bytes):
            return content
        return str(content).encode(self.charset)

    def init_headers(self, headers: Optional[Mapping[str, str]] = None) -> list:
        raw_headers = []
        if headers:
            for k, v in headers.items():
                raw_headers.append((k.lower().encode("latin-1"), v.encode("latin-1")))
        if self.media_type is not None:
            raw_headers.append((b"content-type", self.media_type.encode("latin-1")))
        return raw_headers

    def to_dict(self):
        return {
            "status": self.status_code,
            "headers": self.headers,
            "body": self.body,
        }

class HTMLResponse(Response):
    media_type = "text/html"

class PlainTextResponse(Response):
    media_type = "text/plain"

class JSONResponse(Response):
    media_type = "application/json"

    def render(self, content: Any) -> bytes:
        return json.dumps(content, default=str).encode(self.charset)

class RedirectResponse(Response):
    def __init__(
        self,
        url: str,
        status_code: int = 307,
        headers: Optional[Mapping[str, str]] = None,
    ) -> None:
        super().__init__(content=b"", status_code=status_code, headers=headers)
        self.headers.append((b"location", urllib.parse.quote(url, safe=":/%#?=@[]!$&'()*+,;").encode("latin-1")))

class StreamingResponse(TokenStreamResponse):
    def __init__(
        self,
        content: Any,
        status_code: int = 200,
        headers: Optional[Mapping[str, str]] = None,
        media_type: Optional[str] = None,
    ) -> None:
        raw_headers = []
        if headers:
            for k, v in headers.items():
                raw_headers.append((k.lower().encode("latin-1"), v.encode("latin-1")))
        if media_type is not None:
            raw_headers.append((b"content-type", media_type.encode("latin-1")))
            
        super().__init__(content, status_code, raw_headers)

