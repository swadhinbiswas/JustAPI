use async_graphql::{http::GraphiQLSource, EmptyMutation, EmptySubscription, Object, Schema};
// Removed async_graphql_axum
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::StatusCode;
use hyper::{body::Incoming, Method, Request, Response};

use crate::ResponseBody;

/// Maximum allowed query depth to prevent denial-of-service via deeply nested queries.
const DEFAULT_QUERY_DEPTH_LIMIT: usize = 10;

/// Maximum allowed query complexity to prevent resource exhaustion.
/// Each field costs 1, each list costs 10, each inline fragment costs 5.
const DEFAULT_QUERY_COMPLEXITY_LIMIT: usize = 200;

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn system_status(&self) -> &str {
        "JustAPI GraphQL Federation Gateway is running"
    }

    async fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }
}

pub type AppSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

pub fn create_schema() -> AppSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .enable_federation()
        .limit_depth(DEFAULT_QUERY_DEPTH_LIMIT)
        .limit_complexity(DEFAULT_QUERY_COMPLEXITY_LIMIT)
        .finish()
}

/// Create a GraphQL schema with optional introspection disabled.
/// In production, disable introspection to prevent schema enumeration.
pub fn create_schema_with_introspection(enable_introspection: bool) -> AppSchema {
    let builder = Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .enable_federation()
        .limit_depth(DEFAULT_QUERY_DEPTH_LIMIT)
        .limit_complexity(DEFAULT_QUERY_COMPLEXITY_LIMIT);

    let builder = if enable_introspection { builder } else { builder.disable_introspection() };

    builder.finish()
}

pub async fn handle_graphql(
    schema: &AppSchema,
    req: Request<Incoming>,
    enable_graphiql: bool,
) -> anyhow::Result<Response<ResponseBody>> {
    use http_body_util::BodyExt;

    // Check if GET (graphiql) — only serve when explicitly enabled
    if *req.method() == Method::GET {
        if !enable_graphiql {
            return Ok(Response::builder()
                .status(StatusCode::FORBIDDEN)
                .header("content-type", "application/json")
                .body(crate::UnsyncBoxBody::new(
                    Full::new(Bytes::from(r#"{"error":"GraphiQL is disabled in production"}"#))
                        .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                ))?);
        }

        let source = GraphiQLSource::build().endpoint("/graphql").finish();

        let body = crate::UnsyncBoxBody::new(
            Full::new(Bytes::from(source))
                .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
        );

        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/html; charset=utf-8")
            .body(body)?);
    }

    // Read body
    let body_bytes = req.into_body().collect().await?.to_bytes();

    // Enforce body size limit (1MB)
    if body_bytes.len() > 1_048_576 {
        return Ok(Response::builder()
            .status(StatusCode::PAYLOAD_TOO_LARGE)
            .header("content-type", "application/json")
            .body(crate::UnsyncBoxBody::new(
                Full::new(Bytes::from(r#"{"error":"Query body too large (max 1MB)"}"#))
                    .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
            ))?);
    }

    // Parse GraphQL request
    let gql_req: async_graphql::Request = serde_json::from_slice(&body_bytes)?;

    // Execute with depth and complexity limits enforced by the schema
    let res = schema.execute(gql_req).await;

    // Serialize response
    let res_bytes = serde_json::to_vec(&res)?;

    let body = crate::UnsyncBoxBody::new(
        Full::new(Bytes::from(res_bytes))
            .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
    );

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(body)?)
}

pub fn graphiql_html() -> String {
    async_graphql::http::GraphiQLSource::build().endpoint("/graphql").finish()
}

pub async fn execute_graphql_bytes(
    schema: &AppSchema,
    body_bytes: &[u8],
) -> anyhow::Result<String> {
    let gql_req: async_graphql::Request = serde_json::from_slice(body_bytes)?;
    let res = schema.execute(gql_req).await;
    Ok(serde_json::to_string(&res)?)
}
