"""Per-user rate limiting (`app.enable_user_rate_limit`) — keyed on JWT `sub`."""
import time

from justapi import JustAPIApp, JustAPITestClient, JwtAuth


def _app(secret: str, rps: int = 1) -> JustAPIApp:
    app = JustAPIApp()
    app.set_jwt_auth(secret, "HS256")
    app.enable_user_rate_limit(requests_per_second=rps)

    @app.get("/me")
    def me(request):
        return {"sub": request.auth["sub"]}

    return app


def _token(secret: str, sub: str) -> str:
    return JwtAuth(secret=secret, algorithm="HS256").encode(
        {"sub": sub, "exp": int(time.time()) + 60}
    )


def test_user_rate_limit_per_claim():
    secret = "s3cret"
    c = JustAPITestClient(_app(secret))
    alice = _token(secret, "alice")
    bob = _token(secret, "bob")

    first = c.get_with("/me", [("Authorization", f"Bearer {alice}")])
    assert first["status"] == 200, first

    limited = c.get_with("/me", [("Authorization", f"Bearer {alice}")])
    assert limited["status"] == 429, limited
    assert limited["headers"].get("retry-after") == "1"

    # Different user → own bucket.
    other = c.get_with("/me", [("Authorization", f"Bearer {bob}")])
    assert other["status"] == 200, other


def test_user_rate_limit_without_token_is_401_first():
    # JWT runs before the limiter: an unauthenticated request is rejected by
    # auth, not silently bucketed.
    secret = "s3cret"
    c = JustAPITestClient(_app(secret))
    resp = c.get_with("/me", [])
    assert resp["status"] == 401, resp
