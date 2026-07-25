"""Tests for JWT authentication and OAuth2 support.

Covers:
- ``JwtAuth`` encode/decode round-trip
- ``OAuth2PasswordBearer`` token extraction
- ``OAuth2PasswordRequestForm`` field binding
- Rust middleware integration via ``app.set_jwt_auth()``
"""

import time
import json

import pytest

from justapi import JustAPIApp, JustAPITestClient, HTTPException
from justapi.auth import JwtAuth, OAuth2PasswordBearer, OAuth2PasswordRequestForm


# ---------------------------------------------------------------------------
# JwtAuth unit tests
# ---------------------------------------------------------------------------

def test_jwt_auth_encode_decode():
    jwt_auth = JwtAuth(secret="test-secret", algorithm="HS256")
    now = int(time.time())
    claims = {"sub": "user123", "name": "Test User", "roles": ["admin"], "iat": now, "exp": now + 3600}
    token = jwt_auth.encode(claims)
    assert isinstance(token, str) and len(token) > 20
    decoded = jwt_auth.decode(token)
    assert decoded["sub"] == "user123"
    assert decoded["name"] == "Test User"
    assert decoded["roles"] == ["admin"]


def test_jwt_auth_decode_invalid_token():
    jwt_auth = JwtAuth(secret="test-secret", algorithm="HS256")
    with pytest.raises(HTTPException) as exc_info:
        jwt_auth.decode("invalid.token.here")
    assert exc_info.value.status_code == 401


def test_jwt_auth_decode_expired_token():
    jwt_auth = JwtAuth(secret="test-secret", algorithm="HS256")
    now = int(time.time())
    claims = {"sub": "user", "exp": now - 120}  # expired 2 min ago (past 60s leeway)
    token = jwt_auth.encode(claims)
    with pytest.raises(HTTPException) as exc_info:
        jwt_auth.decode(token)
    assert exc_info.value.status_code == 401


def test_jwt_auth_decode_no_auto_error():
    jwt_auth = JwtAuth(secret="test-secret", algorithm="HS256", auto_error=False)
    with pytest.raises(ValueError):
        jwt_auth.decode("invalid.token.here")


def test_jwt_auth_different_algorithms():
    for alg in ["HS256", "HS384", "HS512"]:
        jwt_auth = JwtAuth(secret="test-secret", algorithm=alg)
        now = int(time.time())
        claims = {"sub": "user", "exp": now + 3600}
        token = jwt_auth.encode(claims)
        decoded = jwt_auth.decode(token)
        assert decoded["sub"] == "user"


# ---------------------------------------------------------------------------
# OAuth2PasswordBearer tests
# ---------------------------------------------------------------------------

def test_oauth2_password_bearer_init():
    oauth = OAuth2PasswordBearer(tokenUrl="/token")
    assert oauth.tokenUrl == "/token"
    assert oauth.model["type"] == "oauth2"


def test_oauth2_password_bearer_default_token_url():
    oauth = OAuth2PasswordBearer()
    assert oauth.tokenUrl == "/token"


# ---------------------------------------------------------------------------
# OAuth2PasswordRequestForm tests
# ---------------------------------------------------------------------------

def test_oauth2_password_request_form():
    form = OAuth2PasswordRequestForm(
        grant_type="password",
        username="alice",
        password="secret123",
        scope="read write",
    )
    assert form.grant_type == "password"
    assert form.username == "alice"
    assert form.password == "secret123"
    assert form.scopes == ["read", "write"]
    # Form defaults are Form objects (not None) because the Form(...) descriptor
    # is used as the default. At runtime, the param binder resolves them.
    assert form.scopes == ["read", "write"]


def test_oauth2_password_request_form_empty_scopes():
    form = OAuth2PasswordRequestForm(
        grant_type="password",
        username="bob",
        password="hunter2",
        scope="",
    )
    assert form.scopes == []


# ---------------------------------------------------------------------------
# Rust middleware integration test
# ---------------------------------------------------------------------------

def test_set_jwt_auth_on_app():
    app = JustAPIApp()
    app.set_jwt_auth("my-secret", "HS256")
    # Verify the app runs (the middleware is applied at run time)
    # We can't easily test the full middleware chain here, but we can verify
    # that the configuration was accepted without error.
    assert app._app is not None
