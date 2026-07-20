"""shop.config — application instance, database wiring, and shared state.

The JustAPI Runtime binds handlers to a single `JustAPIApp` instance and the
DB pool is reached through `app.db`. We keep both here as package-level singletons
so handler modules can import them without a circular dependency on the app
module that registers routes.
"""
from __future__ import annotations

import os

from justapi import JustAPIApp

HERE = os.path.dirname(os.path.abspath(__file__))
SHOP_DB = os.environ.get("DEMO_SHOP_DB", os.path.join(HERE, "..", "shop.db"))


def create_app() -> JustAPIApp:
    """Build the JustAPIApp, wire the owned SQLite database, and return it.

    Route handlers are registered by importing the handler modules (their
    decorators run at import time against this same `app` instance).
    """
    application = JustAPIApp()
    # Owned, writable database (seeded from Olist by db_setup.py). The framework's
    # wal=True path is exercised separately (ADR-068); plain journal here so the
    # demo runs anywhere without that flag.
    application.set_database(f"sqlite://{SHOP_DB}")
    application.connect_database()
    return application


# Single application instance for the process.
app = create_app()


def db():
    """Return the live connection-pool handle (justapi Database)."""
    return app.db


# Import handler modules *after* `app` and `db` exist so their decorators
# register on the app and their `from .config import app, db` resolves.
from . import catalog  # noqa: E402,F401
from . import customers  # noqa: E402,F401
from . import cart  # noqa: E402,F401
from . import orders  # noqa: E402,F401
from . import reviews  # noqa: E402,F401
