"""Smoke-check the shipped stubs mirror the real public surface.

Not a pytest file (it is analysed statically, never executed) — the
`if TYPE_CHECKING` guard keeps runtime imports out. Run:

    mypy crates/justapi-py/tests/stub_surface_check.py
"""
from typing import TYPE_CHECKING, List

if TYPE_CHECKING:
    from justapi import (
        APIKeyHeader,
        APIRouter,
        FileResponse,
        HTTPBearer,
        JustAPIApp,
        Mailer,
        Query,
        Security,
        Session,
        UploadFile,
        build_openapi,
    )

    app = JustAPIApp(
        title="check",
        version="1.0.0",
        servers=[{"url": "https://x"}],
        contact={"name": "a"},
        license_info={"name": "MIT"},
        lifespan=None,
    )

    router = APIRouter(prefix="/v1", tags=["t"], callbacks=[])

    @router.get("/a", name="a")
    def a() -> dict:
        return {}

    @router.post("/b", body_schema=None)
    def b() -> dict:
        return {}

    app.include_router(router, prefix="/api", tags=["x"])
    app.enable_secure_headers(with_hsts=True)
    app.add_middleware(object)
    app.add_api_route("/c", lambda: {})
    app.add_api_websocket_route("/ws", lambda ws: None)
    app.mount("/static", "assets")
    app.route("/r", methods=["GET"])(lambda: {})
    app.on_event("startup")(lambda: None)
    app.graphql("/gql")
    app.register_health_check("db", lambda: True)
    app.schedule("* * * * *", lambda: None)
    app.every(5, lambda: None)
    url: str = app.url_for("a")
    doc: dict = build_openapi(app)

    bearer = Security(HTTPBearer(), scopes=["read"])
    key = APIKeyHeader("X-API-Key")

    async def upload(files: List[UploadFile], q: int = Query(1, ge=0)) -> dict:
        return {}

    session: Session
    session.set({"k": "v"})

    mailer = Mailer(host="smtp.example.com", default_from="a@b.c")

    async def mail() -> None:
        await mailer.send_async(to="a@b.c", subject="s", body="b")

    def file() -> FileResponse:
        return FileResponse("/tmp/x.txt")


if TYPE_CHECKING:
    # Typical FastAPI-migration idioms must type-check too.
    from justapi import Depends, HTTPException, JustAPIApp, Query

    app2 = JustAPIApp()

    @app2.get("/items/{item_id}")
    def read_item(item_id: int, q: str | None = None) -> dict:
        return {"item_id": item_id, "q": q}

    @app2.get("/search")
    def search(limit: int = Query(10, ge=1, le=100), tag: str = "all") -> dict:
        return {"limit": limit, "tag": tag}

    def get_token() -> str:
        return "t"

    @app2.get("/secure")
    def secure(token: str = Depends(get_token)) -> dict:
        if not token:
            raise HTTPException(status_code=401, detail="nope")
        return {"tok": token}
