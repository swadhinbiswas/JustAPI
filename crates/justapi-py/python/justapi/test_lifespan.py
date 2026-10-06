"""Lifespan parity: JustAPIApp(lifespan=...) entered/exited around run (Phase 53)."""
from contextlib import asynccontextmanager
from justapi import JustAPIApp


def test_lifespan_enter_exit_order():
    events = []

    @asynccontextmanager
    async def lifespan(app):
        events.append("enter")
        yield
        events.append("exit")

    app = JustAPIApp(lifespan=lifespan)

    @app.on_event("startup")
    def _up():
        events.append("startup")

    @app.on_event("shutdown")
    def _down():
        events.append("shutdown")

    # Don't bind a socket: stub the server start.
    app._serve = lambda addr, max_body_size=None: None
    app.run("127.0.0.1:0")

    assert events == ["startup", "enter", "exit", "shutdown"], events


def test_no_lifespan_still_runs_events():
    events = []
    app = JustAPIApp()

    @app.on_event("startup")
    def _up():
        events.append("startup")

    app._serve = lambda addr, max_body_size=None: None
    app.run("127.0.0.1:0")
    assert events == ["startup"]
