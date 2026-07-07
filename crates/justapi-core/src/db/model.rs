//! Model trait for mapping structs to database tables.
//!
//! Provides `Model` trait with CRUD operations.

use async_trait::async_trait;

use crate::db::AnyPool;

/// Trait for types that map to a database table.
#[async_trait]
pub trait Model: Sized + Send + 'static {
    /// The database table name.
    fn table_name() -> &'static str;

    /// The column name for the primary key (default `"id"`).
    fn pk_column() -> &'static str {
        "id"
    }

    /// Count all rows in the table.
    async fn count(pool: &AnyPool) -> Result<i64, sqlx::Error> {
        let sql = format!("SELECT COUNT(*) as count FROM {}", Self::table_name());
        pool.query_single_i64(&sql).await
    }
}
