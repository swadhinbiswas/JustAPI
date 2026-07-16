//! Connection pool manager with health checks.
//!
//! Manages one or more database connection pools, keyed by name.
//! Supports PostgreSQL, SQLite, and MySQL via `sqlx`.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use futures::TryStreamExt as _;
use sqlx::any::AnyPoolOptions;
use sqlx::{Column, Executor, Row};
use tokio::sync::RwLock;

/// Database engine kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbKind {
    Postgres,
    Sqlite,
    MySql,
}

impl fmt::Display for DbKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbKind::Postgres => write!(f, "postgres"),
            DbKind::Sqlite => write!(f, "sqlite"),
            DbKind::MySql => write!(f, "mysql"),
        }
    }
}

impl DbKind {
    pub fn from_url(url: &str) -> Self {
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            DbKind::Postgres
        } else if url.starts_with("sqlite://") || url.starts_with("sqlite:") {
            DbKind::Sqlite
        } else if url.starts_with("mysql://") || url.starts_with("mariadb://") {
            DbKind::MySql
        } else {
            DbKind::Postgres
        }
    }
}

/// Transaction isolation level, mapped to each engine's dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
    /// Snapshot isolation (Postgres `REPEATABLE READ`/`SERIALIZABLE` semantics
    /// vary; this maps to the engine's default repeatable/snapshot level).
    Snapshot,
}

impl IsolationLevel {
    /// The `SET TRANSACTION ISOLATION LEVEL ...` fragment for this engine.
    pub fn to_sql(self, _kind: DbKind) -> &'static str {
        match self {
            IsolationLevel::ReadUncommitted => "READ UNCOMMITTED",
            IsolationLevel::ReadCommitted => "READ COMMITTED",
            IsolationLevel::RepeatableRead => "REPEATABLE READ",
            IsolationLevel::Serializable => "SERIALIZABLE",
            IsolationLevel::Snapshot => "REPEATABLE READ",
        }
    }

    pub fn is_supported(self, kind: DbKind) -> bool {
        // SQLite only supports SERIALIZABLE (its default). Everything else
        // honours the standard levels.
        match kind {
            DbKind::Sqlite => self == IsolationLevel::Serializable,
            _ => true,
        }
    }
}

/// Translate a user-facing DB URL into the form `sqlx::Any` expects.
///
/// `sqlx::Any` is stricter than the per-driver pools about URL shape: SQLite
/// wants `sqlite:<path>` (single colon), e.g. `sqlite::memory:` or
/// `sqlite:./file.db`, whereas applications conventionally write
/// `sqlite://:memory:` / `sqlite://./file.db` (double slash, Postgres-style).
/// Postgres/MySQL URLs keep their `//` form. We only rewrite the SQLite scheme
/// so existing `sqlite://...` configs (used by the Python `app.set_database`)
/// keep working.
fn normalize_db_url(url: &str) -> String {
    if url.starts_with("sqlite://") {
        // `sqlx::Any` wants `sqlite:<path>` (single colon). Replace the `//`:
        // `sqlite://:memory:` => `sqlite::memory:`, `sqlite://./x` => `sqlite:./x`,
        // `sqlite:///abs/x` => `sqlite:/abs/x`. Already-single-colon URLs
        // (`sqlite:./x`) don't match and pass through unchanged.
        url.replacen("sqlite://", "sqlite:", 1)
    } else {
        url.to_string()
    }
}

/// Database configuration for a single named pool.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub kind: Option<DbKind>,
    /// Optional DDL run once right after the pool is created (e.g. `CREATE
    /// TABLE ...`). Multiple `;`-separated statements are each run in order.
    pub init_sql: Option<String>,
    /// Optional PRAGMA / session statements run on every connection right after
    /// it is checked out of the pool (e.g. `journal_mode=WAL`,
    /// `synchronous=NORMAL`). SQLite-only; ignored by other drivers. Applied via
    /// `after_connect` so every pooled connection picks them up.
    pub pragmas: Option<Vec<String>>,
    /// Maximum time to wait for a connection from the pool (acquire timeout).
    /// `None` uses sqlx's default (no timeout).
    pub acquire_timeout: Option<Duration>,
    /// Maximum idle time before a connection is closed and recycled.
    pub idle_timeout: Option<Duration>,
    /// Maximum lifetime of a connection before it is forcibly closed.
    pub max_lifetime: Option<Duration>,
    /// Background health-check interval. When `Some`, the [`PoolManager`] spawns
    /// a task that pings the pool and logs/marks it unhealthy. `None` disables
    /// the loop.
    pub health_check_interval: Option<Duration>,
    /// Default transaction isolation level for `begin()`/`transaction()`.
    pub default_isolation: Option<IsolationLevel>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            max_connections: 10,
            kind: None,
            init_sql: None,
            pragmas: None,
            acquire_timeout: Some(Duration::from_secs(30)),
            idle_timeout: None,
            max_lifetime: Some(Duration::from_secs(1800)),
            health_check_interval: None,
            default_isolation: None,
        }
    }
}

/// A typed parameter value for a prepared statement.
///
/// `serde_json::Value` is the convenience path, but it cannot represent raw
/// blobs or full-precision decimals. [`Param`] adds those cases so handlers can
/// bind `bytes`/`BLOB` columns and pass native JSON directly.
#[derive(Debug, Clone)]
pub enum Param {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    /// Raw bytes, bound as a BLOB. Round-trips back as `{"$bytes": "<base64>"}`.
    Bytes(Vec<u8>),
    /// A JSON document, bound as a native JSON/JSONB value where the driver
    /// supports it, else serialized to text.
    Json(serde_json::Value),
}

impl From<&serde_json::Value> for Param {
    fn from(v: &serde_json::Value) -> Self {
        // Typed-param wire markers emitted by the Python bridge. `DbParam.bytes`
        // serializes to `{"$bytes": "<base64>"}` so BLOB values survive the
        // JSON bridge and are bound as a real BLOB (not text).
        if let Some(obj) = v.as_object() {
            if let Some(b64) = obj.get("$bytes").and_then(|x| x.as_str()) {
                return match B64.decode(b64) {
                    Ok(b) => Param::Bytes(b),
                    Err(_) => Param::Text(b64.to_string()),
                };
            }
        }
        match v {
            serde_json::Value::Null => Param::Null,
            serde_json::Value::Bool(b) => Param::Bool(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Param::Int(i)
                } else if let Some(f) = n.as_f64() {
                    Param::Float(f)
                } else {
                    Param::Text(n.to_string())
                }
            }
            serde_json::Value::String(s) => Param::Text(s.clone()),
            other => Param::Json(other.clone()),
        }
    }
}

impl Param {
    /// Convert a slice of JSON values into typed params (the common case).
    pub fn from_json_slice(vals: &[serde_json::Value]) -> Vec<Param> {
        vals.iter().map(Param::from).collect()
    }
}

/// A ready-to-use database pool (any engine).
///
/// Wraps a type-erased `sqlx::AnyPool` plus the engine [`DbKind`] (needed to pick
/// the correct SQL placeholder style). Storing the pool as `Any` lets the
/// Rust-native CRUD handlers use a single generic code path over `sqlx::Any`
/// instead of per-driver generics, while still dispatching to the real Postgres,
/// SQLite, or MySQL backend at runtime (driven by the connection URL scheme).
#[derive(Clone)]
pub struct AnyPool {
    inner: sqlx::AnyPool,
    kind: DbKind,
    default_isolation: Option<IsolationLevel>,
}

impl AnyPool {
    /// Connect to any supported engine. The backend is selected by the URL
    /// scheme (`postgres://`, `sqlite://`, `mysql://`). `max_connections`
    /// controls the pool size. `pragmas` (SQLite-only) are run on every
    /// connection as it is checked out of the pool (e.g. `journal_mode=WAL`).
    pub async fn connect_with(
        url: &str,
        max_connections: u32,
        kind: DbKind,
        pragmas: Option<Vec<String>>,
    ) -> Result<Self, sqlx::Error> {
        Self::connect(config(url, max_connections, kind, pragmas)).await
    }

    /// Connect from a full [`DatabaseConfig`], applying pool tuning (acquire
    /// timeout, idle/lifetime recycling) and SQLite PRAGMAs.
    pub async fn connect(config: DatabaseConfig) -> Result<Self, sqlx::Error> {
        let kind = config.kind.unwrap_or_else(|| DbKind::from_url(&config.url));
        sqlx::any::install_default_drivers();
        let connect_url = normalize_db_url(&config.url);
        let mut opts = AnyPoolOptions::new().max_connections(config.max_connections);
        if let Some(t) = config.acquire_timeout {
            opts = opts.acquire_timeout(t);
        }
        if let Some(t) = config.idle_timeout {
            opts = opts.idle_timeout(t);
        }
        if let Some(t) = config.max_lifetime {
            opts = opts.max_lifetime(t);
        }
        if let (DbKind::Sqlite, Some(pragmas)) = (kind, config.pragmas) {
            opts = opts.after_connect(move |conn, _meta| {
                let pragmas = pragmas.clone();
                Box::pin(async move {
                    for p in &pragmas {
                        conn.execute(sqlx::query(p)).await?;
                    }
                    Ok(())
                })
            });
        }
        let inner = opts.connect(&connect_url).await?;
        Ok(Self { inner, kind, default_isolation: config.default_isolation })
    }

    pub fn kind(&self) -> DbKind {
        self.kind
    }

    /// Normalize `?` placeholders to the driver's native style. Postgres uses
    /// positional `$1`, `$2`, ...; SQLite and MySQL accept `?` natively so the
    /// SQL is returned unchanged. This lets callers always pass `?` regardless
    /// of the backing engine.
    fn normalize_sql(&self, sql: &str) -> String {
        if self.kind != DbKind::Postgres {
            return sql.to_string();
        }
        let mut out = String::with_capacity(sql.len());
        let mut idx: usize = 0;
        for ch in sql.chars() {
            if ch == '?' {
                idx += 1;
                out.push_str(&format!("${}", idx));
            } else {
                out.push(ch);
            }
        }
        out
    }

    pub fn default_isolation(&self) -> Option<IsolationLevel> {
        self.default_isolation
    }

    /// Access the underlying type-erased pool.
    pub fn as_any(&self) -> &sqlx::AnyPool {
        &self.inner
    }

    pub async fn health_check(&self) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT 1").execute(&self.inner).await.map(|_| ())
    }

    pub async fn execute(&self, sql: &str) -> Result<u64, sqlx::Error> {
        sqlx::query(sql).execute(&self.inner).await.map(|r| r.rows_affected())
    }

    /// Run a query returning a single i64 value (e.g. COUNT or version).
    pub async fn query_single_i64(&self, sql: &str) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(sql).fetch_one(&self.inner).await?;
        Ok(row.0)
    }

    /// Run a query returning many (i64,) rows.
    pub async fn query_many_i64(&self, sql: &str) -> Result<Vec<i64>, sqlx::Error> {
        let rows: Vec<(i64,)> = sqlx::query_as(sql).fetch_all(&self.inner).await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Begin a transaction with the pool's default isolation level (if set).
    pub async fn begin(&self) -> Result<sqlx::Transaction<'static, sqlx::Any>, sqlx::Error> {
        self.begin_with(self.default_isolation).await
    }

    /// Begin a transaction with an explicit isolation level. If the engine does
    /// not support the requested level (e.g. non-SERIALIZABLE on SQLite), it
    /// falls back to the engine default rather than erroring.
    pub async fn begin_with(
        &self,
        isolation: Option<IsolationLevel>,
    ) -> Result<sqlx::Transaction<'static, sqlx::Any>, sqlx::Error> {
        let mut tx = self.inner.begin().await?;
        if let Some(iso) = isolation {
            if iso.is_supported(self.kind) && self.kind != DbKind::Sqlite {
                let stmt = format!("SET TRANSACTION ISOLATION LEVEL {}", iso.to_sql(self.kind));
                tx.execute(sqlx::query(&stmt)).await?;
            }
        }
        Ok(tx)
    }

    /// Run a query and return all rows as a JSON array of objects, mirroring how
    /// the Python bridge returns query results. Used by Rust-native CRUD routes
    /// so a `INSERT ... RETURNING` / `SELECT` can be served without touching the
    /// GIL.
    pub async fn query_json(&self, sql: &str) -> Result<serde_json::Value, sqlx::Error> {
        let rows = sqlx::query(sql).fetch_all(&self.inner).await?;
        rows_to_json(rows)
    }

    /// Run a SQL statement with bound parameters and return all rows as a JSON
    /// array of objects. Parameters are downcast from JSON via [`bind_param`], so
    /// this is injection-safe (no string interpolation). Used by the Rust-native
    /// CRUD handlers (select/update/delete) which build the SQL string but bind
    /// every value as a parameter. Also exposed to Python handlers via the
    /// `DbPool` bridge as `query(sql, params)`.
    pub async fn query_with_params(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<serde_json::Value, sqlx::Error> {
        self.query_with(sql, &Param::from_json_slice(params)).await
    }

    /// Like [`AnyPool::query_with_params`] but accepts typed [`Param`]s (supports
    /// BLOB binding and native JSON values).
    pub async fn query_with(
        &self,
        sql: &str,
        params: &[Param],
    ) -> Result<serde_json::Value, sqlx::Error> {
        let sql = self.normalize_sql(sql);
        let mut q = sqlx::query(&sql);
        for p in params {
            q = bind_param(q, p);
        }
        let rows = q.fetch_all(&self.inner).await?;
        rows_to_json(rows)
    }

    /// Run a write (INSERT/UPDATE/DELETE/DDL) with bound parameters. Injection-safe
    /// (no string interpolation). Returns the number of rows affected.
    pub async fn execute_with_params(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<u64, sqlx::Error> {
        self.execute_with(sql, &Param::from_json_slice(params)).await
    }

    /// Like [`AnyPool::execute_with_params`] but accepts typed [`Param`]s.
    pub async fn execute_with(&self, sql: &str, params: &[Param]) -> Result<u64, sqlx::Error> {
        let sql = self.normalize_sql(sql);
        let mut q = sqlx::query(&sql);
        for p in params {
            q = bind_param(q, p);
        }
        let res = q.execute(&self.inner).await?;
        Ok(res.rows_affected())
    }

    /// Stream a query in batches of `chunk` rows, returning a `Vec` of row-chunks.
    ///
    /// Unlike [`AnyPool::query_with`] (which buffers the whole result set), this
    /// reads the result set incrementally and yields each `chunk`-sized window as
    /// a JSON array, so a very large result set stays bounded in memory on the
    /// Rust side. Each chunk is itself a JSON array of row objects.
    pub async fn query_stream(
        &self,
        sql: &str,
        params: &[Param],
        chunk: usize,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let chunk = chunk.max(1);
        let sql = self.normalize_sql(sql);
        let mut q = sqlx::query(&sql);
        for p in params {
            q = bind_param(q, p);
        }
        let mut rows = q.fetch(&self.inner);
        let mut chunks: Vec<serde_json::Value> = Vec::new();
        let mut buf: Vec<serde_json::Value> = Vec::with_capacity(chunk);
        while let Some(row) = rows.try_next().await? {
            buf.push(row_to_json(&row)?);
            if buf.len() == chunk {
                chunks.push(serde_json::Value::Array(std::mem::take(&mut buf)));
            }
        }
        if !buf.is_empty() {
            chunks.push(serde_json::Value::Array(buf));
        }
        Ok(chunks)
    }

    /// Run a list of `(sql, params)` statements atomically inside one transaction
    /// and commit. On any error the transaction is rolled back. Returns the rows
    /// of the final statement (as JSON) if it produced rows, else an object with
    /// the total rows affected.
    ///
    /// Row-vs-write detection is **robust**: each statement is fetched; if it
    /// returns rows they are recorded, otherwise its `rows_affected` is folded
    /// into the running total. This handles `WITH ... SELECT`, `WITH ... INSERT`,
    /// and `RETURNING` correctly without brittle keyword probing.
    pub async fn transaction(
        &self,
        stmts: &[(String, Vec<serde_json::Value>)],
    ) -> Result<serde_json::Value, sqlx::Error> {
        let typed: Vec<(String, Vec<Param>)> =
            stmts.iter().map(|(s, p)| (s.clone(), Param::from_json_slice(p))).collect();
        self.transaction_with(&typed).await
    }

    /// Like [`AnyPool::transaction`] but accepts typed [`Param`]s.
    pub async fn transaction_with(
        &self,
        stmts: &[(String, Vec<Param>)],
    ) -> Result<serde_json::Value, sqlx::Error> {
        self.transaction_with_isolation(stmts, self.default_isolation).await
    }

    /// Run a transaction with an explicit isolation level (overriding the pool
    /// default for this call). See [`AnyPool::transaction_with`].
    pub async fn transaction_with_isolation(
        &self,
        stmts: &[(String, Vec<Param>)],
        isolation: Option<IsolationLevel>,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let mut tx = self.begin_with(isolation).await?;
        let mut last_rows: serde_json::Value = serde_json::Value::Null;
        let mut total_affected: u64 = 0;
        for (sql, params) in stmts {
            let sql = self.normalize_sql(sql);
            let mut q = sqlx::query(&sql);
            for p in params {
                q = bind_param(q, p);
            }
            // `fetch_many` yields each row (`Left`) and, once, the final
            // `QueryResult` (`Right`) which carries `rows_affected`. This
            // executes the statement exactly once and simultaneously tells us
            // whether it was a read (rows present) or a write (rows_affected),
            // without brittle keyword probing and without re-executing.
            #[allow(deprecated)]
            let mut stream = q.fetch_many(&mut *tx);
            let mut rows: Vec<serde_json::Value> = Vec::new();
            let mut affected: u64 = 0;
            while let Some(item) = stream.try_next().await? {
                match item {
                    sqlx::Either::Left(result) => {
                        affected = result.rows_affected();
                    }
                    sqlx::Either::Right(row) => {
                        rows.push(row_to_json(&row)?);
                    }
                }
            }
            if rows.is_empty() {
                total_affected += affected;
                last_rows = serde_json::Value::Null;
            } else {
                last_rows = serde_json::Value::Array(rows);
            }
        }
        tx.commit().await?;
        if last_rows.is_null() {
            Ok(serde_json::json!({ "rows_affected": total_affected }))
        } else {
            Ok(last_rows)
        }
    }

    /// Insert a row from a JSON object (`{column: value}`) and return the
    /// inserted row(s) as JSON via `RETURNING *`. Values are bound as parameters
    /// (no string interpolation), so this is injection-safe. Placeholder style is
    /// chosen per driver (`$N` for Postgres, `?` for SQLite/MySQL). The statement
    /// runs as a single auto-committed statement (SQL guarantees atomicity), which
    /// is sufficient for a single-row insert; multi-statement transaction support
    /// can be layered on later via [`AnyPool::begin`].
    ///
    /// `columns` restricts which keys are written (so a client cannot inject
    /// arbitrary columns); only keys present in both the JSON and `columns` are
    /// inserted.
    pub async fn insert_returning(
        &self,
        table: &str,
        columns: &[String],
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let obj = body.as_object().ok_or_else(|| {
            sqlx::Error::ColumnNotFound("request body must be a JSON object".into())
        })?;
        let mut cols: Vec<String> = Vec::new();
        let mut vals: Vec<Param> = Vec::new();
        for c in columns {
            if let Some(v) = obj.get(c) {
                cols.push(c.clone());
                vals.push(Param::from(v));
            }
        }
        if cols.is_empty() {
            return Err(sqlx::Error::ColumnNotFound(
                "no insertable columns matched the request body".into(),
            ));
        }
        let ph: Box<dyn Fn(usize) -> String + Send + Sync> = match self.kind() {
            DbKind::Postgres => Box::new(|i| format!("${}", i + 1)),
            DbKind::Sqlite | DbKind::MySql => Box::new(|_| "?".to_string()),
        };
        let placeholders: Vec<String> = (0..cols.len()).map(ph).collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) RETURNING *",
            table,
            cols.join(", "),
            placeholders.join(", ")
        );
        let mut q = sqlx::query(&sql);
        for v in &vals {
            q = bind_param(q, v);
        }
        let rows = q.fetch_all(&self.inner).await?;
        rows_to_json(rows)
    }
}

/// Bind a [`Param`] as a query parameter for `sqlx::Any`.
fn bind_param<'q>(
    q: sqlx::query::Query<'q, sqlx::Any, <sqlx::Any as sqlx::Database>::Arguments<'q>>,
    v: &Param,
) -> sqlx::query::Query<'q, sqlx::Any, <sqlx::Any as sqlx::Database>::Arguments<'q>> {
    match v {
        Param::Null => q.bind(Option::<String>::None),
        Param::Bool(b) => q.bind(*b),
        Param::Int(i) => q.bind(*i),
        Param::Float(f) => q.bind(*f),
        Param::Text(s) => q.bind(s.clone()),
        Param::Bytes(b) => q.bind(b.clone()),
        Param::Json(j) => q.bind(j.to_string()),
    }
}

/// Convert a slice of `sqlx::any::AnyRow` rows into a JSON array of column-keyed objects.
fn rows_to_json(rows: Vec<sqlx::any::AnyRow>) -> Result<serde_json::Value, sqlx::Error> {
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut obj = serde_json::Map::new();
        for col in row.columns() {
            let name = col.name().to_string();
            let val: serde_json::Value = column_to_json(row, col.ordinal())?;
            obj.insert(name, val);
        }
        out.push(serde_json::Value::Object(obj));
    }
    Ok(serde_json::Value::Array(out))
}

/// Convert a single `sqlx::any::AnyRow` into a JSON object (column-keyed).
fn row_to_json(row: &sqlx::any::AnyRow) -> Result<serde_json::Value, sqlx::Error> {
    let mut obj = serde_json::Map::new();
    for col in row.columns() {
        let name = col.name().to_string();
        let val: serde_json::Value = column_to_json(row, col.ordinal())?;
        obj.insert(name, val);
    }
    Ok(serde_json::Value::Object(obj))
}

/// Best-effort, fidelity-preserving mapping of a single `sqlx::any::AnyRow`
/// column value to JSON.
///
/// Mapping rules (in priority order):
/// - NULL → `null`
/// - `bool` → JSON bool
/// - 32/64-bit integers → JSON integer (kept as `i64` to preserve precision)
/// - `f64` → JSON number, but **non-finite** values (NaN/±inf) become `null`
///   because JSON has no representation for them (prevents serialization panics)
/// - `Vec<u8>` (BLOB) → `{"$bytes": "<base64>"}` so the client can detect and
///   decode binary without ambiguity
/// - everything else (text, UUID, DECIMAL, TIMESTAMP, JSONB, enums, …) → JSON
///   string (the canonical text form). High-precision DECIMAL is preserved as a
///   string rather than lossily coerced to `f64`.
fn column_to_json(row: &sqlx::any::AnyRow, idx: usize) -> Result<serde_json::Value, sqlx::Error> {
    // NULL check first (try_get::<Option<_>> yields None for NULL).
    if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
        return Ok(serde_json::Value::from(v));
    }
    if let Ok(v) = row.try_get::<Option<i32>, _>(idx) {
        return Ok(serde_json::Value::from(v.map(i64::from)));
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return Ok(serde_json::Value::from(v));
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        // Guard against NaN/inf: JSON cannot represent them.
        return Ok(match v {
            Some(f) if f.is_finite() => serde_json::Value::from(f),
            _ => serde_json::Value::Null,
        });
    }
    // BLOB / bytea → base64-wrapped object so binary survives the JSON bridge.
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
        return Ok(match v {
            Some(b) => serde_json::json!({ "$bytes": B64.encode(b) }),
            None => serde_json::Value::Null,
        });
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return Ok(serde_json::Value::from(v));
    }
    // Fallback: render as string (covers dates, decimals, JSONB text, etc.).
    match row.try_get::<Option<String>, _>(idx) {
        Ok(s) => Ok(serde_json::Value::from(s)),
        Err(e) => Err(e),
    }
}

/// Build a [`DatabaseConfig`] from the common fields (kept for callers that do
/// not need pool tuning).
fn config(
    url: &str,
    max_connections: u32,
    kind: DbKind,
    pragmas: Option<Vec<String>>,
) -> DatabaseConfig {
    DatabaseConfig {
        url: url.to_string(),
        max_connections,
        kind: Some(kind),
        init_sql: None,
        pragmas,
        acquire_timeout: Some(std::time::Duration::from_secs(30)),
        idle_timeout: None,
        max_lifetime: Some(std::time::Duration::from_secs(1800)),
        health_check_interval: None,
        default_isolation: None,
    }
}

/// Manager for multiple named database pools.
#[derive(Default)]
pub struct PoolManager {
    pools: Arc<RwLock<HashMap<String, AnyPool>>>,
}

impl PoolManager {
    pub fn new() -> Self {
        Self { pools: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn init(
        &mut self,
        name: impl Into<String>,
        config: DatabaseConfig,
    ) -> Result<AnyPool, sqlx::Error> {
        let name = name.into();
        let pool = AnyPool::connect(config).await?;
        self.pools.write().await.insert(name, pool.clone());
        Ok(pool)
    }

    pub async fn get(&self, name: &str) -> Option<AnyPool> {
        self.pools.read().await.get(name).cloned()
    }

    pub async fn default(&self) -> Option<AnyPool> {
        self.get("").await
    }

    pub async fn health_check_all(&self) -> (usize, usize) {
        let pools = self.pools.read().await;
        let total = pools.len();
        let mut healthy = 0;
        for pool in pools.values() {
            if pool.health_check().await.is_ok() {
                healthy += 1;
            }
        }
        (healthy, total)
    }

    /// Spawn a background health-check loop for all pools. Pings every pool at
    /// `interval` and logs unhealthy ones. Errors are logged, never propagated —
    /// a flaky health check must not take the server down.
    pub async fn spawn_health_checks(self: &Arc<Self>, interval: Duration) {
        let pools = self.pools.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let snapshot = pools.read().await;
                for (name, pool) in snapshot.iter() {
                    if let Err(e) = pool.health_check().await {
                        tracing::warn!(pool = %name, error = %e, "database health check failed");
                    }
                }
            }
        });
    }

    pub async fn len(&self) -> usize {
        self.pools.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.pools.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_kind_from_url() {
        assert_eq!(DbKind::from_url("postgres://localhost/db"), DbKind::Postgres);
        assert_eq!(DbKind::from_url("postgresql://localhost/db"), DbKind::Postgres);
        assert_eq!(DbKind::from_url("sqlite://./test.db"), DbKind::Sqlite);
        assert_eq!(DbKind::from_url("sqlite::memory:"), DbKind::Sqlite);
        assert_eq!(DbKind::from_url("mysql://localhost/db"), DbKind::MySql);
        assert_eq!(DbKind::from_url("mariadb://localhost/db"), DbKind::MySql);
    }

    #[test]
    fn test_config_default() {
        let cfg = DatabaseConfig::default();
        assert_eq!(cfg.max_connections, 10);
        assert!(cfg.url.is_empty());
        assert!(cfg.kind.is_none());
        assert!(cfg.acquire_timeout.is_some());
        assert!(cfg.health_check_interval.is_none());
    }

    #[test]
    fn test_isolation_support() {
        assert!(IsolationLevel::Serializable.is_supported(DbKind::Sqlite));
        assert!(!IsolationLevel::ReadCommitted.is_supported(DbKind::Sqlite));
        assert!(IsolationLevel::ReadCommitted.is_supported(DbKind::Postgres));
    }

    #[tokio::test]
    async fn test_sqlite_roundtrip_types() {
        let pool =
            AnyPool::connect(config("sqlite::memory:", 1, DbKind::Sqlite, None)).await.unwrap();
        pool.execute(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, ok INTEGER, data BLOB, big REAL)",
        )
        .await
        .unwrap();
        pool.execute_with(
            "INSERT INTO t (name, ok, data, big) VALUES (?, ?, ?, ?)",
            &[
                Param::Text("hello".into()),
                Param::Int(1),
                Param::Bytes(vec![1, 2, 3]),
                Param::Float(f64::INFINITY),
            ],
        )
        .await
        .unwrap();
        let rows = pool.query_with("SELECT * FROM t", &[]).await.unwrap();
        let row = rows.as_array().unwrap()[0].as_object().unwrap();
        assert_eq!(row["name"], serde_json::json!("hello"));
        assert_eq!(row["ok"], serde_json::json!(1));
        // BLOB → base64 object.
        assert_eq!(row["data"], serde_json::json!({ "$bytes": B64.encode([1u8, 2, 3]) }));
        // Non-finite float → null (JSON-safe).
        assert_eq!(row["big"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn test_transaction_detects_writes_and_reads() {
        let pool =
            AnyPool::connect(config("sqlite::memory:", 1, DbKind::Sqlite, None)).await.unwrap();
        pool.execute("CREATE TABLE u (id INTEGER PRIMARY KEY, n INTEGER)").await.unwrap();
        // A write followed by a read inside one transaction.
        let res = pool
            .transaction(&[
                ("INSERT INTO u (n) VALUES (?)".into(), vec![serde_json::json!(5)]),
                ("SELECT SUM(n) AS total FROM u".into(), vec![]),
            ])
            .await
            .unwrap();
        assert_eq!(res, serde_json::json!([{ "total": 5 }]));
    }

    #[tokio::test]
    async fn test_stream_chunks() {
        let pool =
            AnyPool::connect(config("sqlite::memory:", 1, DbKind::Sqlite, None)).await.unwrap();
        pool.execute("CREATE TABLE s (v INTEGER)").await.unwrap();
        for i in 0..5 {
            pool.execute_with("INSERT INTO s (v) VALUES (?)", &[Param::Int(i)]).await.unwrap();
        }
        let chunks = pool.query_stream("SELECT v FROM s ORDER BY v", &[], 2).await.unwrap();
        // 5 rows in chunks of 2 → [2,2,1].
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].as_array().unwrap().len(), 2);
        assert_eq!(chunks[2].as_array().unwrap().len(), 1);
    }
}
