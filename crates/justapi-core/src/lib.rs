pub mod alerting;
pub mod audit;
pub mod batching;
pub mod coalesce;
pub mod compress;
#[cfg(feature = "db")]
pub mod db;
pub mod dx;
pub mod error_catalog;
pub mod extract;
pub mod gateway;
pub mod graphql;
pub mod health;
#[cfg(feature = "mail")]
pub mod mail;
pub mod memory;
pub mod metrics;
pub mod middleware;
pub mod multipart;
#[cfg(feature = "inference")]
pub mod openai;
pub mod openapi;
pub mod panic;
pub mod plugin;
pub mod rate_limit;
pub mod resilience;
pub mod router;
pub mod secrets;
pub mod serialize;
pub mod server;
pub mod static_files;
pub mod testing;
pub mod trace_context;
pub mod tracing_setup;
pub mod validate;
pub mod wasm;
pub mod xml;

pub use server::{serve, Server};

use std::str::FromStr;

use hyper::Method;

/// The HTTP QUERY method (RFC 10008): a safe, idempotent request that
/// carries a representation (like POST) but must not change state (like GET).
///
/// `hyper` does not ship a `Method::QUERY` constant, so JustAPI provides a
/// helper that returns one.
pub fn query_method() -> Method {
    Method::from_str("QUERY").expect("QUERY is a valid HTTP method name")
}

use anyhow::Result;
use futures::StreamExt;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::body::{Bytes, Frame};
use hyper::Response;

/// Unified response body type supporting both static and streaming payloads.
pub type ResponseBody = UnsyncBoxBody<Bytes, anyhow::Error>;

/// Lift a static string into a JSON response.
pub fn json_response(status: hyper::StatusCode, body: &str) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .header("content-length", body.len().to_string())
        .body(UnsyncBoxBody::new(
            Full::new(Bytes::from(body.to_string()))
                .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
        ))
        .unwrap()
}

/// Canonical error envelope. Every non-2xx response in justapi uses this single
/// shape so clients have one contract to parse (see ADR-052). `detail` carries
/// the human-readable message; the numeric status lives in the HTTP status
/// line. Sensitive internals must never be placed in `detail` for 5xx.
pub fn error_response(status: hyper::StatusCode, detail: &str) -> Response<ResponseBody> {
    json_response(status, &serde_json::json!({ "detail": detail }).to_string())
}

/// 422 validation error using the canonical `{"detail": ...}` envelope.
pub fn validation_response(detail: &str) -> Response<ResponseBody> {
    error_response(hyper::StatusCode::UNPROCESSABLE_ENTITY, detail)
}

/// 503 Service Unavailable with a `Retry-After` hint, using the canonical
/// `{"detail": ...}` envelope. Used for backpressure signals — e.g. a saturated
/// DB connection pool that cannot serve a request within its acquire window.
pub fn service_unavailable_response(detail: &str) -> Response<ResponseBody> {
    Response::builder()
        .status(hyper::StatusCode::SERVICE_UNAVAILABLE)
        .header("content-type", "application/json")
        .header("retry-after", "1")
        .body(UnsyncBoxBody::new(
            Full::new(Bytes::from(serde_json::json!({ "detail": detail }).to_string()))
                .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
        ))
        .unwrap()
}

/// Map a SQLx error from a request-path DB operation to the right status.
/// Saturation (`PoolTimedOut`/`PoolClosed`) becomes `503` (backpressure);
/// everything else is a `500`.
#[cfg(feature = "db")]
pub fn db_error_response(e: &sqlx::Error) -> Response<ResponseBody> {
    match e {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => {
            service_unavailable_response("database pool saturated; please retry shortly")
        }
        _ => error_response(hyper::StatusCode::INTERNAL_SERVER_ERROR, "database error"),
    }
}

/// Lift an `anyhow::Result<Bytes>` stream into a streaming response.
pub fn streaming_response(
    status: hyper::StatusCode,
    content_type: &str,
    stream: impl futures::Stream<Item = Result<Bytes>> + Send + 'static,
) -> Response<ResponseBody> {
    let frame_stream = stream.map(|r| r.map(Frame::data));
    Response::builder()
        .status(status)
        .header("content-type", content_type)
        .body(UnsyncBoxBody::new(http_body_util::StreamBody::new(frame_stream)))
        .unwrap()
}
pub mod dummy_extract;
pub mod grpc;
pub mod test_codec;
