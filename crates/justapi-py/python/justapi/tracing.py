import contextvars

_trace_id_var: contextvars.ContextVar[str | None] = contextvars.ContextVar(
    "trace_id", default=None
)
_span_id_var: contextvars.ContextVar[str | None] = contextvars.ContextVar(
    "span_id", default=None
)


def get_current_trace_id() -> str | None:
    """Return the current OpenTelemetry trace ID as a hex string, or None."""
    return _trace_id_var.get()


def get_current_span_id() -> str | None:
    """Return the current OpenTelemetry span ID as a hex string, or None."""
    return _span_id_var.get()
