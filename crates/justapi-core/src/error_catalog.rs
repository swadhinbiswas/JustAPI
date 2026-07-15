//! Error catalog: well-known JustAPI error codes with helpful messages.
//!
//! Each [`CatalogEntry`] bundles a machine-readable code, a short title, a
//! template message, and default suggestions so that CLI/server code can
//! construct rich [`super::dx::Diagnostic`] values with a single call.

use super::dx::{DiagLevel, Diagnostic};

/// A catalogue entry for a well-known error.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    /// Machine-readable code, e.g. `"E001"`.
    pub code: &'static str,
    /// Short title, e.g. `"Invalid address format"`.
    pub title: &'static str,
    /// Longer template message.
    pub template: &'static str,
    /// Default suggestions shown to the user.
    pub suggestions: &'static [&'static str],
}

impl CatalogEntry {
    /// Build a [`Diagnostic`] from this catalogue entry.
    ///
    /// The diagnostic is always at [`DiagLevel::Error`] and carries the
    /// catalogue's code, template message, and default suggestions.
    pub fn to_diagnostic(&self) -> Diagnostic {
        let mut d = Diagnostic::new(DiagLevel::Error, self.template).code(self.code);
        for s in self.suggestions {
            d = d.suggestion(*s);
        }
        d
    }

    /// Build a diagnostic and attach context (e.g. a file path or value).
    pub fn with_context(&self, ctx: impl Into<String>) -> Diagnostic {
        self.to_diagnostic().context(ctx)
    }
}

// ── Catalog constants ────────────────────────────────────────────────

/// E001 — Invalid address format.
pub const E001_INVALID_ADDRESS: CatalogEntry = CatalogEntry {
    code: "E001",
    title: "Invalid address format",
    template: "the address could not be parsed as HOST:PORT",
    suggestions: &[
        "use the format HOST:PORT, e.g. 127.0.0.1:8080",
        "ensure the port number is between 1 and 65535",
    ],
};

/// E002 — Port already in use.
pub const E002_PORT_IN_USE: CatalogEntry = CatalogEntry {
    code: "E002",
    title: "Port already in use",
    template: "the requested port is already bound by another process",
    suggestions: &[
        "choose a different port with --addr 127.0.0.1:<PORT>",
        "find the blocking process with `lsof -i :<PORT>` or `ss -tlnp`",
    ],
};

/// E003 — TLS certificate not found.
pub const E003_TLS_CERT_NOT_FOUND: CatalogEntry = CatalogEntry {
    code: "E003",
    title: "TLS certificate not found",
    template: "could not read the TLS certificate file",
    suggestions: &[
        "verify the path passed to --tls-cert exists",
        "generate a self-signed cert: openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes",
    ],
};

/// E004 — Database URL invalid.
pub const E004_DB_URL_INVALID: CatalogEntry = CatalogEntry {
    code: "E004",
    title: "Database URL invalid",
    template: "the database connection URL could not be parsed",
    suggestions: &[
        "use the format: postgres://user:password@host:5432/dbname",
        "or for SQLite: sqlite://path/to/db.sqlite",
    ],
};

/// E005 — Migration file parse error.
pub const E005_MIGRATION_PARSE: CatalogEntry = CatalogEntry {
    code: "E005",
    title: "Migration file parse error",
    template: "a migration file could not be parsed",
    suggestions: &[
        "migration files must be named V<number>__<name>.sql",
        "ensure the SQL syntax is valid for your database",
    ],
};

/// E006 — Python not found.
pub const E006_PYTHON_NOT_FOUND: CatalogEntry = CatalogEntry {
    code: "E006",
    title: "Python not found",
    template: "python3 is not available on your PATH",
    suggestions: &[
        "install Python 3.9+ from https://www.python.org/downloads/",
        "or use pyenv: `pyenv install 3.12`",
        "ensure python3 is on your PATH",
    ],
};

/// E007 — Route conflict.
pub const E007_ROUTE_CONFLICT: CatalogEntry = CatalogEntry {
    code: "E007",
    title: "Route conflict",
    template: "two or more routes match the same path pattern",
    suggestions: &[
        "check your route definitions for duplicate paths",
        "use distinct path segments or HTTP methods to disambiguate",
    ],
};

/// E008 — Invalid configuration.
pub const E008_INVALID_CONFIG: CatalogEntry = CatalogEntry {
    code: "E008",
    title: "Invalid configuration",
    template: "the configuration file contains invalid or missing values",
    suggestions: &[
        "check the config file for typos or missing required keys",
        "refer to the JustAPI configuration reference for valid options",
    ],
};

/// All catalogue entries, for enumeration in tests or help text.
pub const ALL_ENTRIES: &[&CatalogEntry] = &[
    &E001_INVALID_ADDRESS,
    &E002_PORT_IN_USE,
    &E003_TLS_CERT_NOT_FOUND,
    &E004_DB_URL_INVALID,
    &E005_MIGRATION_PARSE,
    &E006_PYTHON_NOT_FOUND,
    &E007_ROUTE_CONFLICT,
    &E008_INVALID_CONFIG,
];

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_entries_have_unique_codes() {
        let mut codes: Vec<&str> = ALL_ENTRIES.iter().map(|e| e.code).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), ALL_ENTRIES.len());
    }

    #[test]
    fn catalog_entry_produces_diagnostic() {
        let d = E001_INVALID_ADDRESS.to_diagnostic();
        let text = d.render_plain();
        assert!(text.contains("error[E001]"));
        assert!(text.contains("= help:"));
    }

    #[test]
    fn catalog_with_context() {
        let d = E003_TLS_CERT_NOT_FOUND.with_context("/etc/ssl/cert.pem");
        let text = d.render_plain();
        assert!(text.contains("  --> /etc/ssl/cert.pem"));
        assert!(text.contains("E003"));
    }

    #[test]
    fn all_entries_render_without_panic() {
        for entry in ALL_ENTRIES {
            let d = entry.to_diagnostic();
            let _plain = d.render_plain();
            let _colored = d.render_colored();
        }
    }
}
