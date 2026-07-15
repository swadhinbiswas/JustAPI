//! Basic query builder for constructing SQL statements.
//!
//! Provides `Select`, `Insert`, `Update`, and `Delete` builders.
//! These are simple string builders — they don't provide compile-time
//! query checking (use raw `sqlx::query!()` for that).

/// Build a SELECT query.
#[derive(Debug, Clone)]
pub struct Select {
    table: String,
    columns: Vec<String>,
    conditions: Vec<String>,
    order_by: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

impl Select {
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            columns: vec!["*".into()],
            conditions: Vec::new(),
            order_by: None,
            limit: None,
            offset: None,
        }
    }

    pub fn columns(mut self, columns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.columns = columns.into_iter().map(|c| c.into()).collect();
        self
    }

    pub fn r#where(mut self, condition: impl Into<String>) -> Self {
        self.conditions.push(condition.into());
        self
    }

    pub fn order_by(mut self, column: impl Into<String>) -> Self {
        self.order_by = Some(column.into());
        self
    }

    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Build the SQL string.
    pub fn build(&self) -> String {
        let cols = self.columns.join(", ");
        let mut sql = format!("SELECT {} FROM {}", cols, self.table);
        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.conditions.join(" AND "));
        }
        if let Some(ref ob) = self.order_by {
            sql.push_str(" ORDER BY ");
            sql.push_str(ob);
        }
        if let Some(limit) = self.limit {
            sql.push_str(" LIMIT ");
            sql.push_str(&limit.to_string());
        }
        if let Some(offset) = self.offset {
            sql.push_str(" OFFSET ");
            sql.push_str(&offset.to_string());
        }
        sql
    }

    pub fn build_count(&self) -> String {
        let mut sql = format!("SELECT COUNT(*) as count FROM {}", self.table);
        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.conditions.join(" AND "));
        }
        sql
    }
}

/// Build an INSERT query.
#[derive(Debug, Clone)]
pub struct Insert {
    table: String,
    columns: Vec<String>,
    values: Vec<Vec<String>>,
    returning: Option<Vec<String>>,
}

impl Insert {
    pub fn new(table: impl Into<String>) -> Self {
        Self { table: table.into(), columns: Vec::new(), values: Vec::new(), returning: None }
    }

    pub fn columns(mut self, columns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.columns = columns.into_iter().map(|c| c.into()).collect();
        self
    }

    pub fn values(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.values.push(values.into_iter().map(|v| v.into()).collect());
        self
    }

    pub fn returning(mut self, columns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.returning = Some(columns.into_iter().map(|c| c.into()).collect());
        self
    }

    /// Build the SQL string.
    pub fn build(&self) -> String {
        let cols = self.columns.join(", ");
        let placeholders: Vec<String> = self
            .values
            .iter()
            .map(|row| {
                let params: Vec<String> = row.iter().map(|_| "?".to_string()).collect();
                format!("({})", params.join(", "))
            })
            .collect();
        let mut sql =
            format!("INSERT INTO {} ({}) VALUES {}", self.table, cols, placeholders.join(", "));
        if let Some(ref ret) = self.returning {
            sql.push_str(" RETURNING ");
            sql.push_str(&ret.join(", "));
        }
        sql
    }
}

/// Build an UPDATE query.
#[derive(Debug, Clone)]
pub struct Update {
    table: String,
    sets: Vec<String>,
    conditions: Vec<String>,
    returning: Option<Vec<String>>,
}

impl Update {
    pub fn new(table: impl Into<String>) -> Self {
        Self { table: table.into(), sets: Vec::new(), conditions: Vec::new(), returning: None }
    }

    pub fn set(mut self, column: impl Into<String>, placeholder: impl Into<String>) -> Self {
        self.sets.push(format!("{} = {}", column.into(), placeholder.into()));
        self
    }

    pub fn r#where(mut self, condition: impl Into<String>) -> Self {
        self.conditions.push(condition.into());
        self
    }

    pub fn returning(mut self, columns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.returning = Some(columns.into_iter().map(|c| c.into()).collect());
        self
    }

    pub fn build(&self) -> String {
        let sets = self.sets.join(", ");
        let mut sql = format!("UPDATE {} SET {}", self.table, sets);
        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.conditions.join(" AND "));
        }
        if let Some(ref ret) = self.returning {
            sql.push_str(" RETURNING ");
            sql.push_str(&ret.join(", "));
        }
        sql
    }
}

/// Build a DELETE query.
#[derive(Debug, Clone)]
pub struct Delete {
    table: String,
    conditions: Vec<String>,
    returning: Option<Vec<String>>,
}

impl Delete {
    pub fn new(table: impl Into<String>) -> Self {
        Self { table: table.into(), conditions: Vec::new(), returning: None }
    }

    pub fn r#where(mut self, condition: impl Into<String>) -> Self {
        self.conditions.push(condition.into());
        self
    }

    pub fn returning(mut self, columns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.returning = Some(columns.into_iter().map(|c| c.into()).collect());
        self
    }

    pub fn build(&self) -> String {
        let mut sql = format!("DELETE FROM {}", self.table);
        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.conditions.join(" AND "));
        }
        if let Some(ref ret) = self.returning {
            sql.push_str(" RETURNING ");
            sql.push_str(&ret.join(", "));
        }
        sql
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_all() {
        let q = Select::new("users").build();
        assert_eq!(q, "SELECT * FROM users");
    }

    #[test]
    fn test_select_with_where() {
        let q = Select::new("users")
            .columns(["id", "name", "email"])
            .r#where("id = ?")
            .r#where("active = true")
            .build();
        assert_eq!(q, "SELECT id, name, email FROM users WHERE id = ? AND active = true");
    }

    #[test]
    fn test_select_with_limit_offset() {
        let q = Select::new("users").order_by("name ASC").limit(10).offset(20).build();
        assert_eq!(q, "SELECT * FROM users ORDER BY name ASC LIMIT 10 OFFSET 20");
    }

    #[test]
    fn test_insert() {
        let q = Insert::new("users")
            .columns(["name", "email"])
            .values(["Alice", "alice@example.com"])
            .build();
        assert_eq!(q, "INSERT INTO users (name, email) VALUES (?, ?)");
    }

    #[test]
    fn test_insert_returning() {
        let q = Insert::new("users")
            .columns(["name", "email"])
            .values(["Alice", "alice@example.com"])
            .returning(["id"])
            .build();
        assert_eq!(q, "INSERT INTO users (name, email) VALUES (?, ?) RETURNING id");
    }

    #[test]
    fn test_update() {
        let q = Update::new("users").set("name", "?").r#where("id = ?").build();
        assert_eq!(q, "UPDATE users SET name = ? WHERE id = ?");
    }

    #[test]
    fn test_delete() {
        let q = Delete::new("users").r#where("id = ?").build();
        assert_eq!(q, "DELETE FROM users WHERE id = ?");
    }

    #[test]
    fn test_select_build_count() {
        let q = Select::new("users").r#where("active = true").build_count();
        assert_eq!(q, "SELECT COUNT(*) as count FROM users WHERE active = true");
    }
}
