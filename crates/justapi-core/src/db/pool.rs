//! Connection pool manager with health checks.
//!
//! Manages one or more database connection pools, keyed by name.
//! Supports PostgreSQL, SQLite, and MySQL via `sqlx`.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
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

/// Database configuration for a single named pool.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub kind: Option<DbKind>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            max_connections: 10,
            kind: None,
        }
    }
}

/// A ready-to-use database pool (any engine).
#[derive(Clone)]
pub enum AnyPool {
    Pg(PgPool),
    Sqlite(SqlitePool),
    MySql(MySqlPool),
}

impl AnyPool {
    pub fn kind(&self) -> DbKind {
        match self {
            AnyPool::Pg(_) => DbKind::Postgres,
            AnyPool::Sqlite(_) => DbKind::Sqlite,
            AnyPool::MySql(_) => DbKind::MySql,
        }
    }

    pub async fn health_check(&self) -> Result<(), sqlx::Error> {
        match self {
            AnyPool::Pg(p) => sqlx::query("SELECT 1").execute(p).await.map(|_| ()),
            AnyPool::Sqlite(p) => sqlx::query("SELECT 1").execute(p).await.map(|_| ()),
            AnyPool::MySql(p) => sqlx::query("SELECT 1").execute(p).await.map(|_| ()),
        }
    }

    pub async fn execute(&self, sql: &str) -> Result<u64, sqlx::Error> {
        match self {
            AnyPool::Pg(p) => sqlx::query(sql).execute(p).await.map(|r| r.rows_affected()),
            AnyPool::Sqlite(p) => sqlx::query(sql).execute(p).await.map(|r| r.rows_affected()),
            AnyPool::MySql(p) => sqlx::query(sql).execute(p).await.map(|r| r.rows_affected()),
        }
    }

    /// Run a query returning a single i64 value (e.g. COUNT or version).
    pub async fn query_single_i64(&self, sql: &str) -> Result<i64, sqlx::Error> {
        let row: (i64,) = match self {
            AnyPool::Pg(p) => sqlx::query_as(sql).fetch_one(p).await?,
            AnyPool::Sqlite(p) => sqlx::query_as(sql).fetch_one(p).await?,
            AnyPool::MySql(p) => sqlx::query_as(sql).fetch_one(p).await?,
        };
        Ok(row.0)
    }

    /// Run a query returning many (i64,) rows.
    pub async fn query_many_i64(&self, sql: &str) -> Result<Vec<i64>, sqlx::Error> {
        let rows: Vec<(i64,)> = match self {
            AnyPool::Pg(p) => sqlx::query_as(sql).fetch_all(p).await?,
            AnyPool::Sqlite(p) => sqlx::query_as(sql).fetch_all(p).await?,
            AnyPool::MySql(p) => sqlx::query_as(sql).fetch_all(p).await?,
        };
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Begin a transaction.
    pub async fn begin(&self) -> Result<TransactionHandle, sqlx::Error> {
        match self {
            AnyPool::Pg(p) => {
                let tx = p.begin().await?;
                Ok(TransactionHandle::Pg(tx))
            }
            AnyPool::Sqlite(p) => {
                let tx = p.begin().await?;
                Ok(TransactionHandle::Sqlite(tx))
            }
            AnyPool::MySql(p) => {
                let tx = p.begin().await?;
                Ok(TransactionHandle::MySql(tx))
            }
        }
    }
}

/// A handle to an in-progress database transaction.
///
/// Created via [`AnyPool::begin`]. Dropping without
/// committing will roll back the transaction.
pub enum TransactionHandle {
    Pg(sqlx::Transaction<'static, sqlx::Postgres>),
    Sqlite(sqlx::Transaction<'static, sqlx::Sqlite>),
    MySql(sqlx::Transaction<'static, sqlx::MySql>),
}

impl TransactionHandle {
    /// Commit the transaction.
    pub async fn commit(self) -> Result<(), sqlx::Error> {
        match self {
            TransactionHandle::Pg(tx) => tx.commit().await,
            TransactionHandle::Sqlite(tx) => tx.commit().await,
            TransactionHandle::MySql(tx) => tx.commit().await,
        }
    }

    /// Roll back the transaction.
    pub async fn rollback(self) -> Result<(), sqlx::Error> {
        match self {
            TransactionHandle::Pg(tx) => tx.rollback().await,
            TransactionHandle::Sqlite(tx) => tx.rollback().await,
            TransactionHandle::MySql(tx) => tx.rollback().await,
        }
    }
}

/// Manager for multiple named database pools.
#[derive(Default)]
pub struct PoolManager {
    pools: Arc<RwLock<HashMap<String, AnyPool>>>,
}

impl PoolManager {
    pub fn new() -> Self {
        Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn init(
        &mut self,
        name: impl Into<String>,
        config: DatabaseConfig,
    ) -> Result<AnyPool, sqlx::Error> {
        let name = name.into();
        let kind = config.kind.unwrap_or_else(|| DbKind::from_url(&config.url));
        let pool = match kind {
            DbKind::Postgres => AnyPool::Pg(
                PgPoolOptions::new()
                    .max_connections(config.max_connections)
                    .connect(&config.url)
                    .await?,
            ),
            DbKind::Sqlite => AnyPool::Sqlite(
                SqlitePoolOptions::new()
                    .max_connections(config.max_connections)
                    .connect(&config.url)
                    .await?,
            ),
            DbKind::MySql => AnyPool::MySql(
                MySqlPoolOptions::new()
                    .max_connections(config.max_connections)
                    .connect(&config.url)
                    .await?,
            ),
        };
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
        assert_eq!(
            DbKind::from_url("postgres://localhost/db"),
            DbKind::Postgres
        );
        assert_eq!(
            DbKind::from_url("postgresql://localhost/db"),
            DbKind::Postgres
        );
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
