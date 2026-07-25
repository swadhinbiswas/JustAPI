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
