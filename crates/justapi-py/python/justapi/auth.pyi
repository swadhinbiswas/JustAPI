from typing import Any, Dict, List, Optional, Callable


class JwtAuth:
    def __init__(self, secret: str, algorithm: str = "HS256", auto_error: bool = True) -> None: ...
    def encode(self, claims: Dict[str, Any]) -> str: ...
    def decode(self, token: str, **options: bool) -> Dict[str, Any]: ...
    async def __call__(self, authorization: Optional[str] = ...) -> Dict[str, Any]: ...


class OAuth2PasswordBearer:
    def __init__(self, tokenUrl: str = "/token", auto_error: bool = True) -> None: ...
    async def __call__(self, authorization: Optional[str] = ...) -> str: ...


class OAuth2PasswordRequestForm:
    grant_type: str
    username: str
    password: str
    scopes: List[str]
    client_id: Optional[str]
    client_secret: Optional[str]
    def __init__(
        self,
        grant_type: str = ...,
        username: str = ...,
        password: str = ...,
        scope: str = "",
        client_id: Optional[str] = ...,
        client_secret: Optional[str] = ...,
    ) -> None: ...


class OAuth2PasswordRequestFormStrict:
    grant_type: str
    username: str
    password: str
    scopes: List[str]
    client_id: Optional[str]
    client_secret: Optional[str]
    def __init__(
        self,
        grant_type: str = ...,
        username: str = ...,
        password: str = ...,
        scope: str = "",
        client_id: Optional[str] = ...,
        client_secret: Optional[str] = ...,
    ) -> None: ...


class HTTPBearer:
    model: Any
    def __init__(self, auto_error: bool = True) -> None: ...
    async def __call__(self, authorization: Optional[str] = ...) -> str: ...


class HTTPBasic:
    model: Any
    def __init__(self, auto_error: bool = True) -> None: ...
    async def __call__(self, authorization: Optional[str] = ...) -> Any: ...


class APIKeyHeader:
    model: Any
    def __init__(self, name: str, auto_error: bool = True) -> None: ...
    async def __call__(self, request: Any) -> str: ...


class APIKeyQuery:
    model: Any
    def __init__(self, name: str, auto_error: bool = True) -> None: ...
    async def __call__(self, request: Any) -> str: ...


class APIKeyCookie:
    model: Any
    def __init__(self, name: str, auto_error: bool = True) -> None: ...
    async def __call__(self, request: Any) -> str: ...
