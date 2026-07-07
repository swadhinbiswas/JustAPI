use async_graphql::{http::GraphiQLSource, EmptyMutation, EmptySubscription, Object, Schema};
// Removed async_graphql_axum
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::StatusCode;
use hyper::{body::Incoming, Method, Request, Response};

use crate::ResponseBody;

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
        .finish()
}

pub async fn handle_graphql(
    schema: &AppSchema,
    req: Request<Incoming>,
) -> anyhow::Result<Response<ResponseBody>> {
    use http_body_util::BodyExt;

    // Check if GET (graphiql)
    if *req.method() == Method::GET {
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

    // For POST, we would parse the request body and execute the schema.
    // However, since we are using raw Hyper (or need an adapter like async-graphql-axum),
    // we must manually deserialize. Let's do a basic execution.

    // Read body
    let body_bytes = req.into_body().collect().await?.to_bytes();

    // Parse GraphQL request
    let gql_req: async_graphql::Request = serde_json::from_slice(&body_bytes)?;

    // Execute
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
