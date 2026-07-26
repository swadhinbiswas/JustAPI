pub mod alerting;
pub mod audit;
pub mod batching;
pub mod coalesce;
pub mod compress;
#[cfg(feature = "db")]
pub mod db;
pub mod dx;
pub mod error;
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
        .expect("Response::builder with valid inputs should never fail")
}

/// Canonical error envelope following RFC 9457 Problem Details. Every non-2xx
/// response in justapi uses this shape so clients have one contract to parse.
/// The `type` URI identifies the error class, `title` is a short human-readable
/// label, `status` is the HTTP status code, and `detail` carries the full message.
pub fn error_response(status: hyper::StatusCode, detail: &str) -> Response<ResponseBody> {
    let status_code: u16 = status.into();
    let title = match status_code {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Error",
    };
    let body = serde_json::json!({
        "type": format!("https://justapi.dev/errors/{}", title.to_lowercase().replace(' ', "-")),
        "title": title,
        "status": status_code,
        "detail": detail,
    })
    .to_string();
    json_response(status, &body)
}

/// 422 validation error using RFC 9457 Problem Details.
pub fn validation_response(detail: &str) -> Response<ResponseBody> {
    error_response(hyper::StatusCode::UNPROCESSABLE_ENTITY, detail)
}

/// 503 Service Unavailable with a `Retry-After` hint, using RFC 9457 Problem Details.
pub fn service_unavailable_response(detail: &str) -> Response<ResponseBody> {
    let body = serde_json::json!({
        "type": "https://justapi.dev/errors/service-unavailable",
        "title": "Service Unavailable",
        "status": 503,
        "detail": detail,
    })
    .to_string();
    Response::builder()
        .status(hyper::StatusCode::SERVICE_UNAVAILABLE)
        .header("content-type", "application/problem+json")
        .header("retry-after", "1")
        .header("content-length", body.len().to_string())
        .body(UnsyncBoxBody::new(
            Full::new(Bytes::from(body))
                .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
        ))
        .expect("Response::builder with valid inputs should never fail")
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
        .expect("Response::builder with valid inputs should never fail")
}
pub mod dummy_extract;
pub mod grpc;
pub mod test_codec;
