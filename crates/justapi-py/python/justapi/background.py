"""Background task support for JustAPI.

The `BackgroundTasks` class is implemented in Rust (see `justapi_py::background`)
for a fast, bounded, observable scheduler. This module re-exports it so the rest
of the codebase can keep doing `from .background import BackgroundTasks`.
"""

from ._justapi import BackgroundTasks

__all__ = ["BackgroundTasks"]
