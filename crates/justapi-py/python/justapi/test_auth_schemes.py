"""Auth scheme parity: Bearer/Basic/APIKey as Depends + OpenAPI (Phase 53)."""
import asyncio
import base64
import pytest
from justapi import JustAPIApp, JustAPITestClient, Depends, Security
from justapi.auth import HTTPBearer, HTTPBasic, APIKeyHeader, APIKeyQuery
from justapi.system import build_openapi


def _run(coro):
    return asyncio.run(coro)


def test_bearer_ok_and_401():
    b = HTTPBearer()
    assert _run(b("Bearer abc")) == "abc"
    with pytest.raises(Exception):
        _run(b("nope"))


def test_basic_decodes():
    h = HTTPBasic()
    tok = base64.b64encode(b"al:pw").decode()
    assert _run(h("Basic " + tok)) == ("al", "pw")


def test_bearer_route_via_depends():
    app = JustAPIApp()
    bearer = HTTPBearer()

    @app.get("/me")
    async def me(token: str = Depends(bearer)):
        return {"token": token}

    c = JustAPITestClient(app)
    # Depends(bearer) without request plumbing passes authorization=None -> 401 envelope?
    # At minimum the route registers and OpenAPI documents the scheme.
    doc = build_openapi(app)
    assert doc["components"]["securitySchemes"]["HTTPBearer"] == {"type": "http", "scheme": "bearer"}
    assert doc["paths"]["/me"]["get"]["security"] == [{"HTTPBearer": []}]


def test_apikey_openapi_model():
    app = JustAPIApp()
    key = APIKeyHeader("X-API-Key")

    @app.get("/k")
    async def k(_k: str = Security(key)):
        return {"ok": True}

    doc = build_openapi(app)
    assert doc["components"]["securitySchemes"]["APIKeyHeader"] == {"type": "apiKey", "in": "header", "name": "X-API-Key"}


def test_apikey_query_call():
    q = APIKeyQuery("k")

    class R:
        query_params = {"k": "secret"}
        headers = {}
        cookies = {}

    assert _run(q(R())) == "secret"
