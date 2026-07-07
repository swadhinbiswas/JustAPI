/// Extract the current OpenTelemetry trace context as hex strings.
///
/// Returns `(trace_id_hex, span_id_hex)` if a valid OTel context is
/// available from the current `tracing` span, or `None` otherwise.
#[cfg(feature = "opentelemetry")]
pub fn get_current_trace_context() -> Option<(String, String)> {
    use opentelemetry::trace::TraceContextExt as _;
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;

    let span = tracing::Span::current();
    let cx = span.context();
    let span_ctx = cx.span();
    let sc = span_ctx.span_context();
    if !sc.is_sampled() {
        return None;
    }
    let trace_id = sc.trace_id().to_string();
    let span_id = sc.span_id().to_string();
    Some((trace_id, span_id))
}

/// Fallback when `opentelemetry` feature is disabled.
#[cfg(not(feature = "opentelemetry"))]
pub fn get_current_trace_context() -> Option<(String, String)> {
    None
}
