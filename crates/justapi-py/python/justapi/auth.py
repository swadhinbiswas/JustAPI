"""JWT authentication and OAuth2 support.

Provides Rust-native JWT encode/decode via ``_JwtAuth``, plus FastAPI-compatible
``Depends`` helpers for Bearer token extraction and the OAuth2 password flow.

Usage::

    from justapi import JustAPIApp, Depends
    from justapi.auth import JwtAuth, OAuth2PasswordBearer

    app = JustAPIApp()
    jwt_auth = JwtAuth(secret="my-secret", algorithm="HS256")

    # Protect a route
    @app.get("/protected")
    async def protected(current_user: dict = Depends(jwt_auth)):
        return {"user": current_user}

    # Or use the OAuth2 scheme explicitly
    oauth2_scheme = OAuth2PasswordBearer(tokenUrl="/token")

    @app.get("/users/me")
    async def read_users_me(token: str = Depends(oauth2_scheme)):
        claims = jwt_auth.decode(token)
        return claims
"""

from ._justapi import _JwtAuth  # type: ignore[import-untyped]
from .exceptions import HTTPException
from .params import Header, Form
from typing import Optional, Any, Dict


class JwtAuth:
    """Configure JWT authentication with Rust-native crypto.

    Can be used as a ``Depends`` callable that decodes the Bearer token from the
    ``Authorization`` header and returns the decoded claims dict.

    Args:
        secret: The shared secret (for HMAC algorithms) or PEM-encoded private key.
        algorithm: Signing algorithm (default ``"HS256"``). One of:
            ``HS256``, ``HS384``, ``HS512``, ``RS256``, ``RS384``,
            ``RS512``, ``ES256``, ``ES384``, ``ED25519``.
        auto_error: Raise ``401`` when no valid token is present (default ``True``).
    """

    def __init__(
        self,
        secret: str,
        algorithm: str = "HS256",
        auto_error: bool = True,
    ):
        self._backend = _JwtAuth(secret, algorithm)
        self._auto_error = auto_error

    def encode(self, claims: dict) -> str:
        """Encode a claims dict into a signed JWT string.

        Args:
            claims: Payload dict (e.g. ``{"sub": "user123", "roles": ["admin"]}``).
        Returns:
            The signed JWT as a string.
        """
        return self._backend.encode(claims)

    def decode(self, token: str, **options: bool) -> dict:
        """Decode and validate a JWT token, returning the claims dict.

        Args:
            token: The JWT string.
            **options: Verification options:
                - ``verify_exp`` (default ``True``)
                - ``verify_iat`` (default ``True``)
                - ``verify_nbf`` (default ``True``)
                - ``verify_aud`` (default ``True``)
                - ``verify_iss`` (default ``True``)
        Returns:
            The decoded claims dict.
        Raises:
            HTTPException(401): If the token is invalid or expired.
        """
        try:
            return dict(self._backend.decode(token, options))
        except Exception as e:
            if self._auto_error:
                raise HTTPException(status_code=401, detail=str(e))
            raise

    async def __call__(self, authorization: Optional[str] = Header(None)) -> dict:
        """Decode the Bearer token from the Authorization header.

        Used as a ``Depends`` callable in route handlers.
        """
        if not authorization:
            if self._auto_error:
                raise HTTPException(
                    status_code=401,
                    detail="Not authenticated",
                    headers={"WWW-Authenticate": "Bearer"},
                )
            return {}
        if not authorization.startswith("Bearer "):
            if self._auto_error:
                raise HTTPException(
                    status_code=401,
                    detail="Invalid authorization header",
                    headers={"WWW-Authenticate": "Bearer"},
                )
            return {}
        token = authorization[7:]
        return self.decode(token)

    def __repr__(self) -> str:
        return f"JwtAuth(algorithm={self._backend!r})"


class OAuth2PasswordBearer:
    """FastAPI-compatible OAuth2 password flow Bearer token extractor.

    Extracts the Bearer token from the ``Authorization`` header. Use as a
    ``Depends`` callable to inject the raw token string into a handler, then
    decode it yourself (or pair it with :class:`JwtAuth.decode`).

    Args:
        tokenUrl: The URL where the client can obtain a token (for OpenAPI docs).
        auto_error: Raise ``401`` when no token is present (default ``True``).
    """

    def __init__(self, tokenUrl: str = "/token", auto_error: bool = True):
        self.tokenUrl = tokenUrl
        self.auto_error = auto_error
        self.model = {"type": "oauth2", "flows": {"password": {"tokenUrl": tokenUrl}}}

    async def __call__(self, authorization: Optional[str] = Header(None)) -> str:
        """Extract the Bearer token from the request."""
        if not authorization:
            if self.auto_error:
                raise HTTPException(
                    status_code=401,
                    detail="Not authenticated",
                    headers={"WWW-Authenticate": "Bearer"},
                )
            return ""
        if not authorization.startswith("Bearer "):
            if self.auto_error:
                raise HTTPException(
                    status_code=401,
                    detail="Invalid authorization header",
                    headers={"WWW-Authenticate": "Bearer"},
                )
            return ""
        return authorization[7:]

    def __repr__(self) -> str:
        return f"OAuth2PasswordBearer(tokenUrl='{self.tokenUrl}')"


class OAuth2PasswordRequestForm:
    """FastAPI-compatible form dependency for the OAuth2 token endpoint.

    Extracts ``grant_type``, ``username``, ``password``, ``scope``, ``client_id``,
    and ``client_secret`` from a form-encoded request body.

    Usage::

        @app.post("/token")
        async def login(form: OAuth2PasswordRequestForm = Depends(OAuth2PasswordRequestForm)):
            # validate form.username / form.password
            ...
    """

    def __init__(
        self,
        grant_type: str = Form(...),
        username: str = Form(...),
        password: str = Form(...),
        scope: str = Form(""),
        client_id: Optional[str] = Form(None),
        client_secret: Optional[str] = Form(None),
    ):
        self.grant_type = grant_type
        self.username = username
        self.password = password
        self.scopes = scope.split() if scope else []
        self.client_id = client_id
        self.client_secret = client_secret

    def __repr__(self) -> str:
        return f"OAuth2PasswordRequestForm(username='{self.username}')"


class OAuth2PasswordRequestFormStrict:
    """Strict variant that requires ``grant_type=password``.

    Same fields as :class:`OAuth2PasswordRequestForm`, but rejects requests where
    ``grant_type`` is not ``"password"`` with a ``422 Unprocessable Entity``.
    """

    def __init__(
        self,
        grant_type: str = Form(...),
        username: str = Form(...),
        password: str = Form(...),
        scope: str = Form(""),
        client_id: Optional[str] = Form(None),
        client_secret: Optional[str] = Form(None),
    ):
        if grant_type != "password":
            raise HTTPException(
                status_code=422,
                detail='Invalid grant_type. Must be "password".',
            )
        self.grant_type = grant_type
        self.username = username
        self.password = password
        self.scopes = scope.split() if scope else []
        self.client_id = client_id
        self.client_secret = client_secret

    def __repr__(self) -> str:
        return f"OAuth2PasswordRequestFormStrict(username='{self.username}')"
