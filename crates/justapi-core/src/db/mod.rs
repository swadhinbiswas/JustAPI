//! Database integration: connection pooling, migrations, query building.
//!
//! Feature-gated behind `db` — enabled by default in the Python package.
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use justapi_core::db::{PoolManager, DatabaseConfig, DbKind};
//!
//! # async fn example() {
//! let config = DatabaseConfig {
//!     kind: DbKind::Postgres,
//!     url: "postgres://user:pass@localhost/mydb".into(),
//!     max_connections: 10,
//! };
//! let mut mgr = PoolManager::new();
//! let pool = mgr.init("default", config).await.unwrap();
//! # }
//! ```

pub mod migrations;
pub mod model;
pub mod pool;
pub mod query;

pub use migrations::*;
pub use model::*;
pub use pool::*;
pub use query::*;
