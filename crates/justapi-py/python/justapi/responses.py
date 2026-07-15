import json
import os
import mimetypes
import urllib.parse
from datetime import datetime, timezone
from typing import Any, Mapping, Optional, Union
from ._justapi import TokenStreamResponse


class Response:
    """HTTP response.

    The body/headers/status are plain Python attributes; the Rust side reads
    them directly (via a ``_justapi_response`` marker) to avoid a ``to_dict``
    round-trip on the hot path.
    """

    _justapi_response = True
    media_type = None
    charset = "utf-8"

    def __init__(
        self,
        content: Any = None,
        status_code: int = 200,
        headers: Optional[Mapping[str, str]] = None,
        media_type: Optional[str] = None,
        background: Any = None,
    ) -> None:
        self.status_code = status_code
        if media_type is not None:
            self.media_type = media_type
        self.background = background
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

    def append_header(self, name: bytes, value: bytes) -> None:
        """Append a raw ``(name, value)`` header tuple (used by ``set_cookie``)."""
        self.headers.append((name, value))

    def set_cookie(
        self,
        name: str,
        value: str = "",
        max_age: Optional[int] = None,
        expires: Optional[Union[int, datetime, str]] = None,
        path: str = "/",
        domain: Optional[str] = None,
        secure: bool = False,
        httponly: bool = False,
        samesite: Optional[str] = None,
    ) -> None:
        """Set a ``Set-Cookie`` response header (Starlette-compatible)."""
        cookie = f"{name}={value}"
        if expires is not None:
            if isinstance(expires, datetime):
                expires = expires.astimezone(timezone.utc).strftime("%a, %d %b %Y %H:%M:%S GMT")
            elif isinstance(expires, int):
                expires = datetime.fromtimestamp(expires, tz=timezone.utc).strftime(
                    "%a, %d %b %Y %H:%M:%S GMT"
                )
            cookie += f"; expires={expires}"
        if max_age is not None:
            cookie += f"; Max-Age={max_age}"
        if path is not None:
            cookie += f"; path={path}"
        if domain is not None:
            cookie += f"; domain={domain}"
        if secure:
            cookie += "; secure"
        if httponly:
            cookie += "; httponly"
        if samesite is not None:
            cookie += f"; samesite={samesite}"
        self.append_header(b"set-cookie", cookie.encode("latin-1"))

    def delete_cookie(self, name: str, path: str = "/", domain: Optional[str] = None) -> None:
        """Expire and delete a cookie by name (sets an already-expired cookie)."""
        self.set_cookie(
            name=name,
            value="",
            expires="Thu, 01 Jan 1970 00:00:00 GMT",
            path=path,
            domain=domain,
            max_age=0,
        )

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
        background: Any = None,
    ) -> None:
        super().__init__(
            content=b"",
            status_code=status_code,
            headers=headers,
            background=background,
        )
        self.headers.append(
            (
                b"location",
                urllib.parse.quote(url, safe=":/%#?=@[]!$&'()*+,;").encode("latin-1"),
            )
        )


class StreamingResponse(TokenStreamResponse):
    def __init__(
        self,
        content: Any,
        status_code: int = 200,
        headers: Optional[Mapping[str, str]] = None,
        media_type: Optional[str] = None,
        background: Any = None,
    ) -> None:
        raw_headers = []
        if headers:
            for k, v in headers.items():
                raw_headers.append((k.lower().encode("latin-1"), v.encode("latin-1")))
        if media_type is not None:
            raw_headers.append((b"content-type", media_type.encode("latin-1")))

        super().__init__(content, status_code, raw_headers)
        self.background = background


class FileResponse(Response):
    """Serve a file from disk (Starlette-compatible).

    The file is read eagerly into memory and returned as a single body; the
    media type is inferred from the file extension unless ``media_type`` is
    given explicitly. A ``Content-Disposition`` header is added when a
    ``filename`` is supplied.
    """

    def __init__(
        self,
        path: Union[str, os.PathLike],
        headers: Optional[Mapping[str, str]] = None,
        media_type: Optional[str] = None,
        filename: Optional[str] = None,
        stat_result: Optional[os.stat_result] = None,
        content_disposition_type: str = "attachment",
        status_code: int = 200,
        background: Any = None,
    ) -> None:
        self.path = os.fspath(path)
        if filename is None:
            filename = os.path.basename(self.path)
        if media_type is None:
            media_type, _ = mimetypes.guess_type(filename)
            if media_type is None:
                media_type = "application/octet-stream"

        content = b""
        try:
            with open(self.path, "rb") as fh:
                content = fh.read()
        except OSError as exc:
            raise RuntimeError(f"File at path {self.path!r} does not exist or is unreadable: {exc}")

        content_headers = dict(headers or {})
        content_headers.setdefault(
            "content-disposition",
            f'{content_disposition_type}; filename="{filename}"',
        )

        super().__init__(
            content=content,
            status_code=status_code,
            headers=content_headers,
            media_type=media_type,
            background=background,
        )
