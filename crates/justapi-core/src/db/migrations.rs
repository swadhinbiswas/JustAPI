//! File-based SQL migration system.
//!
//! Migrations are plain `.sql` files in a `migrations/` directory, named
//! `<version>_<description>.sql` (e.g. `001_create_users.sql`).

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::db::AnyPool;

/// A single migration with UP and optional DOWN SQL.
#[derive(Debug, Clone)]
pub struct Migration {
    pub version: u64,
    pub name: String,
    pub up_sql: String,
    pub down_sql: Option<String>,
}

impl Migration {
    pub fn parse_filename(filename: &str) -> Option<(u64, String)> {
        let stem = filename.strip_suffix(".sql")?;
        let parts: Vec<&str> = stem.splitn(2, '_').collect();
        if parts.len() < 2 {
            return None;
        }
        let version: u64 = parts[0].parse().ok()?;
        let name = parts[1].to_string();
        Some((version, name))
    }

    pub fn parse_content(content: &str) -> (String, Option<String>) {
        if let Some(pos) = content.find("-- DOWN") {
            // Extract UP section: content before "-- DOWN", then strip "-- UP" marker
            let up = content[..pos].trim().to_string();
            let up = up.trim_start_matches("-- UP").trim().to_string();
            // Extract DOWN section: content after "-- DOWN" marker line
            let down = content[pos + 7..].trim().to_string();
            let down = down.trim_start_matches("-- DOWN").trim().to_string();
            (up, Some(down))
        } else {
            // No DOWN section - entire file is UP
            let up = content.trim().to_string();
            let up = up.trim_start_matches("-- UP").trim().to_string();
            (up, None)
        }
    }

    pub fn from_file(path: &Path) -> Result<Self, String> {
        let full_filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("Invalid filename: {:?}", path))?;
        let (version, name) = Self::parse_filename(full_filename).ok_or_else(|| {
            format!(
                "Migration filename must be <version>_<name>.sql, got: {:?}",
                full_filename
            )
        })?;
        let content =
            fs::read_to_string(path).map_err(|e| format!("Cannot read {:?}: {}", path, e))?;
        let (up_sql, down_sql) = Self::parse_content(&content);
        Ok(Migration {
            version,
            name,
            up_sql,
            down_sql,
        })
    }
}

/// Discovers and runs migrations from a directory.
#[derive(Default)]
pub struct Migrator {
    migrations: Vec<Migration>,
}

impl Migrator {
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    pub fn discover(&mut self, dir: &Path) -> Result<(), String> {
        if !dir.exists() {
            return Err(format!("Migrations directory not found: {:?}", dir));
        }
        let mut migrations = Vec::new();
        for entry in fs::read_dir(dir).map_err(|e| format!("Cannot read {:?}: {}", dir, e))? {
            let entry = entry.map_err(|e| format!("Read error: {}", e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("sql") {
                continue;
            }
            migrations.push(Migration::from_file(&path)?);
        }
        migrations.sort_by_key(|m| m.version);
        self.migrations = migrations;
        Ok(())
    }

    pub fn migrations(&self) -> &[Migration] {
        &self.migrations
    }

    /// Run pending migrations. Creates `_justapi_migrations` tracking table.
    /// Returns list of newly-applied migrations.
    pub async fn run(&self, pool: &AnyPool) -> Result<Vec<Migration>, String> {
        self.ensure_tracking_table(pool).await?;
        let applied = self.get_applied_versions(pool).await?;
        let mut applied_list = Vec::new();
        for migration in &self.migrations {
            if applied.contains(&migration.version) {
                continue;
            }
            tracing::info!(
                "Running migration UP v{} ({})",
                migration.version,
                migration.name
            );
            pool.execute(&migration.up_sql)
                .await
                .map_err(|e| format!("Migration v{} error: {}", migration.version, e))?;
            self.record_migration(pool, migration.version, &migration.name)
                .await?;
            applied_list.push(migration.clone());
        }
        Ok(applied_list)
    }

    /// Roll back the most recently applied migration.
    pub async fn rollback_one(&self, pool: &AnyPool) -> Result<(), String> {
        let applied = self.get_applied_versions(pool).await?;
        let max_version = applied
            .iter()
            .max()
            .cloned()
            .ok_or_else(|| "No migrations to roll back".to_string())?;
        let m = self
            .migrations
            .iter()
            .find(|m| m.version == max_version)
            .ok_or_else(|| format!("Migration v{} not found in files", max_version))?;
        if let Some(ref down_sql) = m.down_sql {
            tracing::info!("Rolling back migration v{} ({})", m.version, m.name);
            pool.execute(down_sql)
                .await
                .map_err(|e| format!("Rollback v{} error: {}", m.version, e))?;
            self.remove_migration(pool, m.version).await?;
        }
        Ok(())
    }

    pub async fn ensure_tracking_table(&self, pool: &AnyPool) -> Result<(), String> {
        let sql = match pool.kind() {
            crate::db::DbKind::Postgres => {
                "CREATE TABLE IF NOT EXISTS _justapi_migrations (\
                 version BIGINT PRIMARY KEY, name TEXT NOT NULL, applied_at TIMESTAMPTZ DEFAULT NOW()\
                 )"
            }
            crate::db::DbKind::Sqlite => {
                "CREATE TABLE IF NOT EXISTS _justapi_migrations (\
                 version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at TEXT DEFAULT (datetime('now'))\
                 )"
            }
            crate::db::DbKind::MySql => {
                "CREATE TABLE IF NOT EXISTS _justapi_migrations (\
                 version BIGINT PRIMARY KEY, name VARCHAR(255) NOT NULL, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP\
                 )"
            }
        };
        pool.execute(sql)
            .await
            .map_err(|e| format!("Create tracking table error: {}", e))?;
        Ok(())
    }

    /// Get set of already-applied migration versions.
    async fn get_applied_versions(&self, pool: &AnyPool) -> Result<HashSet<u64>, String> {
        let versions = pool
            .query_many_i64("SELECT version FROM _justapi_migrations")
            .await
            .map_err(|e| format!("Query applied migrations error: {}", e))?;
        Ok(versions.into_iter().map(|v| v as u64).collect())
    }

    async fn record_migration(
        &self,
        pool: &AnyPool,
        version: u64,
        name: &str,
    ) -> Result<(), String> {
        let safe_name = name.replace('\'', "''");
        let sql = match pool.kind() {
            crate::db::DbKind::Postgres => {
                format!("INSERT INTO _justapi_migrations (version, name) VALUES ({}, '{}') ON CONFLICT DO NOTHING",
                    version, safe_name)
            }
            crate::db::DbKind::Sqlite => {
                format!(
                    "INSERT OR IGNORE INTO _justapi_migrations (version, name) VALUES ({}, '{}')",
                    version, safe_name
                )
            }
            crate::db::DbKind::MySql => {
                format!(
                    "INSERT IGNORE INTO _justapi_migrations (version, name) VALUES ({}, '{}')",
                    version, safe_name
                )
            }
        };
        pool.execute(&sql)
            .await
            .map_err(|e| format!("Record migration error: {}", e))?;
        Ok(())
    }

    async fn remove_migration(&self, pool: &AnyPool, version: u64) -> Result<(), String> {
        let sql = format!(
            "DELETE FROM _justapi_migrations WHERE version = {}",
            version
        );
        pool.execute(&sql)
            .await
            .map_err(|e| format!("Delete migration error: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_filename() {
        let (v, n) = Migration::parse_filename("001_create_users.sql").unwrap();
        assert_eq!(v, 1);
        assert_eq!(n, "create_users");
    }

    #[test]
    fn test_parse_filename_invalid() {
        assert!(Migration::parse_filename("bad.sql").is_none());
        assert!(Migration::parse_filename("no_extension").is_none());
    }

    #[test]
    fn test_parse_content_with_down() {
        let content = "-- UP\nCREATE TABLE users (id INT);\n\n-- DOWN\nDROP TABLE users;";
        let (up, down) = Migration::parse_content(content);
        assert_eq!(up, "CREATE TABLE users (id INT);");
        assert_eq!(down.unwrap(), "DROP TABLE users;");
    }

    #[test]
    fn test_parse_content_up_only() {
        let content = "CREATE TABLE users (id INT);";
        let (up, down) = Migration::parse_content(content);
        assert_eq!(up, "CREATE TABLE users (id INT);");
        assert!(down.is_none());
    }
}
