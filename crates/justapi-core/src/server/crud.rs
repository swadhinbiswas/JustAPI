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
use crate::{db_error_response, error_response, json_response, validation_response};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::Request;
use hyper::StatusCode;

/// The CRUD operation a Rust-native route performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrudOp {
    Insert,
    Select,
    Update,
    Delete,
}

/// Per-route Rust-native CRUD spec (ADR-056 Step C).
#[derive(Clone)]
pub struct CrudSpec {
    pub op: CrudOp,
    pub table: String,
    /// Columns the route is allowed to read/write (allowlist against injection
    /// and against clients writing arbitrary columns).
    pub columns: Vec<String>,
    /// Column used to identify a single row for update/delete by id
    /// (e.g. `id`). Looked up from the matched path params.
    pub id_column: String,
}

fn is_allowed(columns: &[String], name: &str) -> bool {
    columns.iter().any(|c| c.eq_ignore_ascii_case(name))
}

/// Parse a form-style query string (`a=1&b=2`) into an ordered list of
/// `(key, value)` pairs, keeping only keys present in `allowed`.
fn parse_query(filter: &[u8], allowed: &[String]) -> Vec<(String, String)> {
    let s = String::from_utf8_lossy(filter);
    let mut out = Vec::new();
    for pair in s.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        let k = k.to_string();
        if is_allowed(allowed, &k) {
            out.push((k, v.to_string()));
        }
    }
    out
}

/// Look up the id column value from the matched path params, parsed as JSON.
fn id_param(path_params: &[(String, String)], id_column: &str) -> Option<serde_json::Value> {
    let raw = path_params
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(id_column))
        .map(|(_, v)| v.clone())?;
    // Try to parse as a number so `WHERE id = ?` binds an int, not a string.
    if let Ok(i) = raw.parse::<i64>() {
        Some(serde_json::json!(i))
    } else {
        Some(serde_json::Value::String(raw))
    }
}

/// Build a driver-aware placeholder generator. Postgres/MySQL use positional
/// `$N` / `?`; SQLite uses `?`. The closure increments its index on each call so
/// callers emit placeholders in the same order the values are bound.
fn placeholder_gen(pool: &AnyPool) -> impl FnMut() -> String + '_ {
    let dollar = matches!(pool.kind(), crate::db::DbKind::Postgres);
    let mut i = 0usize;
    move || {
        i += 1;
        if dollar {
            format!("${}", i)
        } else {
            "?".to_string()
        }
    }
}

/// Core Rust-native CRUD logic over an already-read body.
///
/// Dispatches on `spec.op`:
/// - **Insert**: validate body (if `validator`), insert the columns present in
///   both the JSON and `spec.columns`, return the inserted row as 200 JSON.
/// - **Select**: filter by query-string params (allowlisted) and/or the path id;
///   return matching rows as 200 JSON.
/// - **Update**: identify the row by path id, set the allowlisted columns from
///   the body, return the updated row as 200 JSON.
/// - **Delete**: identify the row by path id, delete it, return the deleted row
///   as 200 JSON.
///
/// Validation failure / non-object JSON → 422 with `{"detail": ...}`; a missing
/// id for update/delete → 400; DB errors → 500.
pub async fn crud_dispatch_bytes(
    pool: &AnyPool,
    spec: &CrudSpec,
    path_params: &[(String, String)],
    query_string: &[u8],
    body: &[u8],
    validator: Option<&Arc<CompiledValidator>>,
) -> Result<hyper::Response<crate::ResponseBody>, anyhow::Error> {
    match spec.op {
        CrudOp::Insert => {
            // Validate.
            if let Some(v) = validator {
                if let Err(e) = v.validate(body) {
                    return Ok(validation_response(&e.to_string()));
                }
            }
            let body_val: serde_json::Value = match serde_json::from_slice(body) {
                Ok(v) => v,
                Err(_) => return Ok(validation_response("request body must be valid JSON")),
            };
            let obj = match body_val.as_object() {
                Some(o) => o,
                None => return Ok(validation_response("request body must be a JSON object")),
            };
            let mut cols: Vec<String> = Vec::new();
            let mut vals: Vec<serde_json::Value> = Vec::new();
            for c in &spec.columns {
                if let Some(v) = obj.get(c) {
                    cols.push(c.clone());
                    vals.push(v.clone());
                }
            }
            if cols.is_empty() {
                return Ok(validation_response("no insertable columns matched the request body"));
            }
            let mut ph = placeholder_gen(pool);
            let placeholders: Vec<String> = (0..cols.len()).map(|_| ph()).collect();
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({}) RETURNING *",
                spec.table,
                cols.join(", "),
                placeholders.join(", ")
            );
            match pool.query_with_params(&sql, &vals).await {
                Ok(rows) => {
                    let single = rows.as_array().and_then(|a| a.first()).cloned();
                    let payload = single.unwrap_or(rows);
                    Ok(json_response(StatusCode::OK, &payload.to_string()))
                }
                Err(e) => Ok(db_error_response(&e)),
            }
        }
        CrudOp::Select => {
            let mut params: Vec<serde_json::Value> = Vec::new();
            let mut wheres: Vec<String> = Vec::new();
            let mut ph = placeholder_gen(pool);
            // Filter by path id when present.
            if let Some(id) = id_param(path_params, &spec.id_column) {
                wheres.push(format!("{} = {}", spec.id_column, ph()));
                params.push(id);
            }
            // Filter by allowlisted query-string params.
            for (k, v) in parse_query(query_string, &spec.columns) {
                wheres.push(format!("{} = {}", k, ph()));
                params.push(serde_json::Value::String(v));
            }
            let mut sql = format!("SELECT * FROM {}", spec.table);
            if !wheres.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&wheres.join(" AND "));
            }
            match pool.query_with_params(&sql, &params).await {
                Ok(rows) => Ok(json_response(StatusCode::OK, &rows.to_string())),
                Err(e) => Ok(db_error_response(&e)),
            }
        }
        CrudOp::Update => {
            let id = match id_param(path_params, &spec.id_column) {
                Some(v) => v,
                None => {
                    return Ok(error_response(
                        StatusCode::BAD_REQUEST,
                        "missing path id for update",
                    ))
                }
            };
            let body_val: serde_json::Value = match serde_json::from_slice(body) {
                Ok(v) => v,
                Err(_) => return Ok(validation_response("request body must be valid JSON")),
            };
            let obj = match body_val.as_object() {
                Some(o) => o,
                None => return Ok(validation_response("request body must be a JSON object")),
            };
            let mut sets: Vec<String> = Vec::new();
            let mut params: Vec<serde_json::Value> = Vec::new();
            let mut ph = placeholder_gen(pool);
            for c in &spec.columns {
                if let Some(v) = obj.get(c) {
                    sets.push(format!("{} = {}", c, ph()));
                    params.push(v.clone());
                }
            }
            if sets.is_empty() {
                return Ok(validation_response("no updatable columns matched the request body"));
            }
            params.push(id);
            let sql = format!(
                "UPDATE {} SET {} WHERE {} = {} RETURNING *",
                spec.table,
                sets.join(", "),
                spec.id_column,
                ph()
            );
            match pool.query_with_params(&sql, &params).await {
                Ok(rows) => {
                    let single = rows.as_array().and_then(|a| a.first()).cloned();
                    match single {
                        Some(payload) => Ok(json_response(StatusCode::OK, &payload.to_string())),
                        None => Ok(error_response(StatusCode::NOT_FOUND, "row not found")),
                    }
                }
                Err(e) => Ok(db_error_response(&e)),
            }
        }
        CrudOp::Delete => {
            let id = match id_param(path_params, &spec.id_column) {
                Some(v) => v,
                None => {
                    return Ok(error_response(
                        StatusCode::BAD_REQUEST,
                        "missing path id for delete",
                    ))
                }
            };
            let mut ph = placeholder_gen(pool);
            let sql = format!(
                "DELETE FROM {} WHERE {} = {} RETURNING *",
                spec.table,
                spec.id_column,
                ph()
            );
            match pool.query_with_params(&sql, &[id]).await {
                Ok(rows) => {
                    let single = rows.as_array().and_then(|a| a.first()).cloned();
                    match single {
                        Some(payload) => Ok(json_response(StatusCode::OK, &payload.to_string())),
                        None => Ok(error_response(StatusCode::NOT_FOUND, "row not found")),
                    }
                }
                Err(e) => Ok(db_error_response(&e)),
            }
        }
    }
}

/// Build a `HandlerFn` for a Rust-native CRUD route, dispatching on `spec.op`.
/// Reads + limits the body, then delegates to [`crud_dispatch_bytes`].
pub fn crud_handler(
    pool: Arc<AnyPool>,
    spec: CrudSpec,
    validator: Option<Arc<CompiledValidator>>,
) -> HandlerFn {
    Arc::new(move |req: Request<Incoming>| {
        let pool = pool.clone();
        let spec = spec.clone();
        let validator = validator.clone();
        Box::pin(async move {
            let path_params: Vec<(String, String)> =
                req.extensions().get::<Vec<(String, String)>>().cloned().unwrap_or_default();
            let query_string = req.uri().query().unwrap_or("").as_bytes().to_vec();
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
            crud_dispatch_bytes(
                &pool,
                &spec,
                &path_params,
                &query_string,
                &bytes,
                validator.as_ref(),
            )
            .await
        })
    })
}

// --- Backwards-compatible thin wrappers (Step B insert kept working) ---------

/// Build a `HandlerFn` for a Rust-native `POST`/insert route.
pub fn crud_insert_handler(
    pool: Arc<AnyPool>,
    table: String,
    columns: Vec<String>,
    validator: Option<Arc<CompiledValidator>>,
) -> HandlerFn {
    crud_handler(
        pool,
        CrudSpec { op: CrudOp::Insert, table, columns, id_column: "id".to_string() },
        validator,
    )
}

/// Core Rust-native `POST`/insert logic over an already-read body.
pub async fn crud_insert_bytes(
    pool: &AnyPool,
    table: &str,
    columns: &[String],
    validator: Option<&Arc<CompiledValidator>>,
    body: &[u8],
) -> Result<hyper::Response<crate::ResponseBody>, anyhow::Error> {
    crud_dispatch_bytes(
        pool,
        &CrudSpec {
            op: CrudOp::Insert,
            table: table.to_string(),
            columns: columns.to_vec(),
            id_column: "id".to_string(),
        },
        &[],
        b"",
        body,
        validator,
    )
    .await
}

/// Build a `HandlerFn` for a Rust-native `GET`/select route.
#[allow(dead_code)]
pub fn crud_select_handler(
    _pool: Arc<AnyPool>,
    _table: String,
    _query_validator: Option<Arc<CompiledValidator>>,
) -> HandlerFn {
    // Superseded by the unified `crud_handler`; kept for API stability.
    Arc::new(|_req: Request<Incoming>| {
        Box::pin(async move { Ok(error_response(StatusCode::NOT_IMPLEMENTED, "use crud_handler")) })
    })
}
