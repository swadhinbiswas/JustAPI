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

/// Build a `HandlerFn` for a Rust-native `POST`/insert route.
///
/// Pipeline (all in Rust):
/// 1. Read + limit the request body.
/// 2. Validate against `validator` (if any); 422 with `{"detail": ...}` on
///    failure (mirrors the Python path's validation envelope).
/// 3. `INSERT ... RETURNING *` with **bound** parameters (injection-safe) as a
///    single auto-committed statement (SQL guarantees atomicity); column writes
///    are restricted to `columns`.
/// 4. Return the inserted row as `200 application/json`. DB errors → 500.
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
            // 1. Read body (the server-level max_body_size already applies
            //    upstream for the native path).
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

            // 2. Validate.
            if let Some(v) = &validator {
                if let Err(e) = v.validate(&bytes) {
                    return Ok(validation_response(&e.to_string()));
                }
            }
            let body_val: serde_json::Value = match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(_) => {
                    return Ok(validation_response("request body must be valid JSON"));
                }
            };

            // 3. Insert (single auto-committed statement; atomic per SQL).
            let result = pool.insert_returning(&table, &columns, &body_val).await;
            match result {
                Ok(rows) => {
                    let single = rows.as_array().and_then(|a| a.first()).cloned();
                    let payload = single.unwrap_or(rows);
                    Ok(json_response(StatusCode::OK, &payload.to_string()))
                }
                Err(_) => Ok(error_response(StatusCode::INTERNAL_SERVER_ERROR, "database error")),
            }
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
