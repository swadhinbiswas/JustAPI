from typing import Any, Dict, Optional

class HTTPException(Exception):
    def __init__(
        self,
        status_code: int,
        detail: Any = None,
        headers: Optional[Dict[str, str]] = None,
    ) -> None: ...
    status_code: int
    detail: Any
    headers: Optional[Dict[str, str]]

class WebSocketException(Exception):
    def __init__(self, code: int = 1000, reason: Optional[str] = None) -> None: ...
    code: int
    reason: Optional[str]
