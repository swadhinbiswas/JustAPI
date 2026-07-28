//! Route handler dispatch — maps `Handler` variants to HTTP responses.

use anyhow::Result;
use http_body_util::BodyExt;
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response, StatusCode};

use super::sse_ws::sse_response;
use crate::health::HealthRegistry;
use crate::memory::BufferPool;
use crate::metrics::Metrics;
use crate::openapi;
use crate::server::{Handler, BUILTIN_SPEC};
use crate::{error_response, json_response, ResponseBody};

/// Dispatch a matched handler to produce an HTTP response.
pub(crate) async fn execute_handler(
    handler: Handler,
    params: Vec<(String, String)>,
    req: Request<Incoming>,
    pool: &BufferPool,
    metrics: &Metrics,
    health_registry: Option<&HealthRegistry>,
    #[cfg(feature = "graphql")] graphql_schema: Option<&crate::graphql::AppSchema>,
    openapi_spec: Option<&str>,
    max_body_size: usize,
) -> Result<Response<ResponseBody>> {
    let handler_name = match handler {
        Handler::Static { .. } => "static",
        Handler::Echo => "echo",
        Handler::ParamsEcho => "params_echo",
        Handler::Sse => "sse",
        Handler::Health => "health",
        Handler::Ready => "ready",
        Handler::Live => "live",
        Handler::Prometheus => "prometheus",
        Handler::OpenApiJson => "openapi_json",
        Handler::SwaggerUi => "swagger_ui",
        Handler::Redoc => "redoc",
        Handler::GraphQL => "graphql",
        Handler::Custom(_) => "custom",
    };
    let _handler_span = tracing::debug_span!("handler.execute", name = handler_name);

    match handler {
        Handler::Static { status, body } => {
            if status.is_success() {
                Ok(json_response(status, body))
            } else {
                Ok(error_response(status, body))
            }
        }
        Handler::Echo => {
            let body_bytes = match http_body_util::Limited::new(req.into_body(), max_body_size)
                .collect()
                .await
            {
                Ok(collected) => {
                    let bytes = collected.to_bytes();
                    metrics.add_bytes_in(bytes.len() as u64);
                    bytes
                }
                Err(e) if e.to_string().contains("length limit") => {
                    return Ok(error_response(StatusCode::PAYLOAD_TOO_LARGE, "payload too large"))
                }
                Err(_) => return Ok(error_response(StatusCode::BAD_REQUEST, "bad request")),
            };
            let mut buf = pool.acquire(body_bytes.len());
            buf.extend_from_slice(&body_bytes);
            let body_str = String::from_utf8_lossy(&buf).to_string();
            pool.release(buf);
            Ok(json_response(StatusCode::OK, &body_str))
        }
        Handler::ParamsEcho => {
            let params_str: Vec<String> =
                params.iter().map(|(k, v)| format!(r#""{}":"{}""#, k, v)).collect();
            let body = format!("{{{}}}", params_str.join(","));
            Ok(json_response(StatusCode::OK, &body))
        }
        Handler::Sse => Ok(sse_response()),
        Handler::Health => Ok(crate::metrics::health_response()),
        Handler::Ready => {
            if let Some(reg) = health_registry {
                if reg.is_empty() {
                    Ok(crate::metrics::ready_response())
                } else {
                    Ok(reg.health_response().await)
                }
            } else {
                Ok(crate::metrics::ready_response())
            }
        }
        Handler::Live => Ok(crate::metrics::live_response()),
        Handler::Prometheus => Ok(crate::metrics::metrics_response(metrics)),
        Handler::OpenApiJson => {
            let body: String =
                if let Some(spec) = openapi_spec { spec.to_string() } else { BUILTIN_SPEC.clone() };
            Ok(json_response(StatusCode::OK, &body))
        }
        Handler::SwaggerUi => {
            let html = openapi::swagger_ui_html();
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/html; charset=utf-8")
                .header("content-length", html.len().to_string())
                .body(crate::UnsyncBoxBody::new(
                    http_body_util::Full::new(Bytes::from(html))
                        .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                ))
                .unwrap())
        }
        Handler::Redoc => {
            let html = openapi::redoc_html();
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/html; charset=utf-8")
                .header("content-length", html.len().to_string())
                .body(crate::UnsyncBoxBody::new(
                    http_body_util::Full::new(Bytes::from(html))
                        .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                ))
                .unwrap())
        }
        Handler::GraphQL => {
            #[cfg(feature = "graphql")]
            {
                if let Some(schema) = graphql_schema {
                    let enable_graphiql = std::env::var("JUSTAPI_ENABLE_GRAPHIQL")
                        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false);
                    crate::graphql::handle_graphql(schema, req, enable_graphiql).await.or_else(
                        |e| {
                            Ok(Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(crate::UnsyncBoxBody::new(
                                    http_body_util::Full::new(Bytes::from(format!(
                                        "GraphQL Error: {}",
                                        e
                                    )))
                                    .map_err(
                                        |e: std::convert::Infallible| -> anyhow::Error {
                                            match e {}
                                        },
                                    ),
                                ))
                                .unwrap())
                        },
                    )
                } else {
                    Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(crate::UnsyncBoxBody::new(
                            http_body_util::Full::new(Bytes::from(
                                "GraphQL schema not initialized",
                            ))
                            .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                        ))
                        .unwrap())
                }
            }
            #[cfg(not(feature = "graphql"))]
            {
                Ok(error_response(StatusCode::NOT_FOUND, "GraphQL is not enabled"))
            }
        }
        Handler::Custom(f) => {
            let path_params: Vec<(String, String)> = params;
            let mut req = req;
            req.extensions_mut().insert(path_params);
            f(req).await
        }
    }
}
