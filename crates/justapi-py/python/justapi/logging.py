"""Logging configuration for JustAPI.

The runtime installs a default INFO, text-formatted logger to stdout the moment
``app.run()`` is called, so request access logs and server events are visible
out of the box. Use the functions below to override that default before calling
``run()`` (a subscriber already set is never replaced):

- ``init_logging(level="info", format="text")`` — text or JSON to stdout.
- ``init_json_logging()`` — structured JSON to stdout.
- ``init_file_logging("app.log")`` — JSON to a rolling file.
- ``init_otlp_tracing(endpoint, service_name)`` — export spans via OTLP gRPC.
- ``shutdown_tracing()`` — flush and stop the subscriber.
"""
from ._justapi import (  # type: ignore[import-untyped]
    init_logging,
    init_json_logging,
    init_file_logging,
    init_otlp_tracing,
    shutdown_tracing,
)

__all__ = [
    "init_logging",
    "init_json_logging",
    "init_file_logging",
    "init_otlp_tracing",
    "shutdown_tracing",
]
