//! Rust-native CRUD route handlers.
//!
//! These handlers run **entirely in Rust** — no Python, no GIL acquisition, no
//! `spawn_blocking` hop. They are the next step beyond the validate-and-echo
//! native fast path (ADR-056): a route whose behavior is composed solely of
//! Rust primitives (validate body → SQL → serialize JSON) is served by a
//! `Handler::Custom` closure, matching Actix/Axum-class speed while keeping the
//! Python DX for route *definition*.
//!
//! Correctness contract: a Rust-native CRUD route must behave identically to the
//! equivalent Python route — same status codes, the `{"detail": ...}` error
//! envelope, transaction semantics (auto-begin/commit on writes, rollback on
//! error), and `application/json` content type.

use std::sync::Arc;

use crate::db::AnyPool;
use crate::middleware::HandlerFn;
use crate::validate::CompiledValidator;
use crate::{error_response, json_response, validation_response};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::Request;
use hyper::StatusCode;

/// Core Rust-native `POST`/insert logic over an already-read body.
///
/// `body` is the raw request bytes (the caller is responsible for enforcing the
/// max body size). Pipeline (all in Rust): validate against `validator` (if
/// any) → parse JSON → `INSERT ... RETURNING *` with bound parameters
/// (injection-safe, column writes restricted to `columns`) → return the row as
/// `200 application/json`. Validation failure → 422 with `{"detail": ...}`; a
/// non-object / non-JSON body → 422; DB errors → 500.
pub async fn crud_insert_bytes(
    pool: &AnyPool,
    table: &str,
    columns: &[String],
    validator: Option<&Arc<CompiledValidator>>,
    body: &[u8],
) -> Result<hyper::Response<crate::ResponseBody>, anyhow::Error> {
    // 1. Validate.
    if let Some(v) = validator {
        if let Err(e) = v.validate(body) {
            return Ok(validation_response(&e.to_string()));
        }
    }
    let body_val: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return Ok(validation_response("request body must be valid JSON")),
    };

    // 2. Insert (single auto-committed statement; atomic per SQL).
    match pool.insert_returning(table, columns, &body_val).await {
        Ok(rows) => {
            let single = rows.as_array().and_then(|a| a.first()).cloned();
            let payload = single.unwrap_or(rows);
            Ok(json_response(StatusCode::OK, &payload.to_string()))
        }
        Err(_) => Ok(error_response(StatusCode::INTERNAL_SERVER_ERROR, "database error")),
    }
}

/// Build a `HandlerFn` for a Rust-native `POST`/insert route.
///
/// Reads + limits the body, then delegates to [`crud_insert_bytes`].
pub fn crud_insert_handler(
    pool: Arc<AnyPool>,
    table: String,
    columns: Vec<String>,
    validator: Option<Arc<CompiledValidator>>,
) -> HandlerFn {
    Arc::new(move |req: Request<Incoming>| {
        let pool = pool.clone();
        let table = table.clone();
        let columns = columns.clone();
        let validator = validator.clone();
        Box::pin(async move {
            let bytes = match http_body_util::Limited::new(
                req.into_body(),
                crate::server::DEFAULT_MAX_BODY_SIZE,
            )
            .collect()
            .await
            {
                Ok(c) => c.to_bytes(),
                Err(_) => {
                    return Ok(error_response(
                        StatusCode::BAD_REQUEST,
                        "could not read request body",
                    ))
                }
            };
            crud_insert_bytes(&pool, &table, &columns, validator.as_ref(), &bytes).await
        })
    })
}

/// Build a `HandlerFn` for a Rust-native `GET`/select route.
///
/// Validates the query string (if `query_validator` given), then runs a
/// `SELECT` via the `Select` builder and returns rows as `200` JSON.
#[allow(dead_code)]
pub fn crud_select_handler(
    _pool: Arc<AnyPool>,
    _table: String,
    _query_validator: Option<Arc<CompiledValidator>>,
) -> HandlerFn {
    // Placeholder for Step C; select wiring follows the same pattern.
    Arc::new(|_req: Request<Incoming>| {
        Box::pin(async move {
            Ok(error_response(StatusCode::NOT_IMPLEMENTED, "crud_select not yet wired"))
        })
    })
}
