//! Connection pool manager with health checks.
//!
//! Manages one or more database connection pools, keyed by name.
//! Supports PostgreSQL, SQLite, and MySQL via `sqlx`.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use sqlx::any::AnyPoolOptions;
use sqlx::{Column, Row};
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
        } else if url.starts_with("sqlite://") {
            DbKind::Sqlite
        } else if url.starts_with("mysql://") {
            DbKind::MySql
        } else {
            DbKind::Postgres
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
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self { url: String::new(), max_connections: 10, kind: None }
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
}

impl AnyPool {
    /// Connect to any supported engine. The backend is selected by the URL
    /// scheme (`postgres://`, `sqlite://`, `mysql://`). `max_connections`
    /// controls the pool size.
    pub async fn connect_with(
        url: &str,
        max_connections: u32,
        kind: DbKind,
    ) -> Result<Self, sqlx::Error> {
        // The `any` driver needs its backend drivers registered before the first
        // connection (sqlx 0.8). Idempotent and cheap.
        sqlx::any::install_default_drivers();
        let connect_url = normalize_db_url(url);
        let inner =
            AnyPoolOptions::new().max_connections(max_connections).connect(&connect_url).await?;
        Ok(Self { inner, kind })
    }

    pub fn kind(&self) -> DbKind {
        self.kind
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

    /// Begin a transaction.
    pub async fn begin(&self) -> Result<sqlx::Transaction<'static, sqlx::Any>, sqlx::Error> {
        self.inner.begin().await
    }

    /// Run a query and return all rows as a JSON array of objects, mirroring how
    /// the Python bridge returns query results. Used by Rust-native CRUD routes
    /// so a `INSERT ... RETURNING` / `SELECT` can be served without touching the
    /// GIL. Column values are mapped best-effort: integers, floats, booleans,
    /// strings, and NULL become their JSON equivalents; other types fall back to
    /// their string form.
    pub async fn query_json(&self, sql: &str) -> Result<serde_json::Value, sqlx::Error> {
        let rows = sqlx::query(sql).fetch_all(&self.inner).await?;
        rows_to_json(rows)
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
        let mut vals: Vec<&serde_json::Value> = Vec::new();
        for c in columns {
            if let Some(v) = obj.get(c) {
                cols.push(c.clone());
                vals.push(v);
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
            q = bind_json(q, v);
        }
        let rows = q.fetch_all(&self.inner).await?;
        rows_to_json(rows)
    }
}

/// Bind a JSON value as a query parameter for `sqlx::Any`. Scalars are downcast
/// to the concrete Rust type sqlx expects (`i64`/`f64`/`bool`/`String`); JSON
/// objects/arrays are stringified. `sqlx::Any` accepts `Option<String>` for NULL.
fn bind_json<'q>(
    q: sqlx::query::Query<'q, sqlx::Any, <sqlx::Any as sqlx::Database>::Arguments<'q>>,
    v: &serde_json::Value,
) -> sqlx::query::Query<'q, sqlx::Any, <sqlx::Any as sqlx::Database>::Arguments<'q>> {
    match v {
        serde_json::Value::Null => q.bind(Option::<String>::None),
        serde_json::Value::Bool(b) => q.bind(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                q.bind(i)
            } else if let Some(f) = n.as_f64() {
                q.bind(f)
            } else {
                q.bind(n.to_string())
            }
        }
        serde_json::Value::String(s) => q.bind(s.clone()),
        other => q.bind(other.to_string()),
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

/// Best-effort mapping of a single `sqlx::any::AnyRow` column value to JSON.
fn column_to_json(row: &sqlx::any::AnyRow, idx: usize) -> Result<serde_json::Value, sqlx::Error> {
    if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
        return Ok(serde_json::Value::from(v));
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return Ok(serde_json::Value::from(v));
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        return Ok(serde_json::Value::from(v));
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return Ok(serde_json::Value::from(v));
    }
    // Fallback: render as string (covers blobs, dates, decimals, etc.).
    match row.try_get::<Option<String>, _>(idx) {
        Ok(s) => Ok(serde_json::Value::from(s)),
        Err(e) => Err(e),
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
        let kind = config.kind.unwrap_or_else(|| DbKind::from_url(&config.url));
        let pool = AnyPool::connect_with(&config.url, config.max_connections, kind).await?;
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
        assert_eq!(DbKind::from_url("mysql://localhost/db"), DbKind::MySql);
    }

    #[test]
    fn test_config_default() {
        let cfg = DatabaseConfig::default();
        assert_eq!(cfg.max_connections, 10);
        assert!(cfg.url.is_empty());
        assert!(cfg.kind.is_none());
    }
}
