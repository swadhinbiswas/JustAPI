//! Developer-experience (DX) diagnostics: rich, colored terminal output.
//!
//! Provides [`Diagnostic`] for structured error, warning, info, and hint
//! messages with optional error codes, suggestions, and file/route context.
//! Respects the `NO_COLOR` environment variable and detects whether stdout
//! is a TTY, falling back to plain text when colours are inappropriate.

use std::fmt;
use std::io::IsTerminal;

// ── ANSI escape helpers ──────────────────────────────────────────────

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const WHITE: &str = "\x1b[37m";

/// Returns `true` when colour output should be used.
///
/// Colour is enabled when:
/// - `NO_COLOR` is **not** set (see <https://no-color.org/>), **and**
/// - stdout is a terminal (not piped/redirected).
pub fn colors_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

// ── DiagLevel ────────────────────────────────────────────────────────

/// Severity level for a [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagLevel {
    /// A fatal problem that prevents the operation from completing.
    Error,
    /// A non-fatal issue that deserves attention.
    Warning,
    /// Informational message.
    Info,
    /// A gentle suggestion for improvement.
    Hint,
}

impl DiagLevel {
    /// Human-readable label, e.g. `"error"`.
    fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }

    /// ANSI colour sequence for this level.
    fn color(self) -> &'static str {
        match self {
            Self::Error => RED,
            Self::Warning => YELLOW,
            Self::Info => CYAN,
            Self::Hint => GREEN,
        }
    }
}

// ── Diagnostic ───────────────────────────────────────────────────────

/// A styled diagnostic message with colours and suggestions.
///
/// # Examples
///
/// ```
/// use justapi_core::dx::{Diagnostic, DiagLevel};
///
/// let d = Diagnostic::new(DiagLevel::Error, "invalid address format")
///     .code("E001")
///     .context("--addr flag")
///     .suggestion("use HOST:PORT, e.g. 127.0.0.1:8080");
///
/// // Render to a plain-text string (for logging / testing):
/// let text = d.render_plain();
/// assert!(text.contains("error[E001]"));
/// ```
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Severity level.
    pub level: DiagLevel,
    /// Optional machine-readable error code, e.g. `"E001"`.
    pub code: Option<String>,
    /// One-line human description of the problem.
    pub message: String,
    /// Zero or more actionable suggestions.
    pub suggestions: Vec<String>,
    /// Optional context such as a file path or route.
    pub context: Option<String>,
}

impl Diagnostic {
    /// Create a new diagnostic at the given level.
    pub fn new(level: DiagLevel, message: impl Into<String>) -> Self {
        Self { level, code: None, message: message.into(), suggestions: Vec::new(), context: None }
    }

    /// Attach an error code (e.g. `"E001"`).
    pub fn code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Add a single suggestion line.
    pub fn suggestion(mut self, s: impl Into<String>) -> Self {
        self.suggestions.push(s.into());
        self
    }

    /// Attach context (file path, route pattern, config key, …).
    pub fn context(mut self, ctx: impl Into<String>) -> Self {
        self.context = Some(ctx.into());
        self
    }

    // ── rendering ────────────────────────────────────────────────

    /// Render with ANSI colours (when appropriate).
    pub fn display(&self) -> String {
        if colors_enabled() {
            self.render_colored()
        } else {
            self.render_plain()
        }
    }

    /// Render **without** any ANSI escape codes.
    pub fn render_plain(&self) -> String {
        let mut out = String::new();

        // Header line: "error[E001]: message" or "warning: message"
        out.push_str(self.level.label());
        if let Some(ref c) = self.code {
            out.push('[');
            out.push_str(c);
            out.push(']');
        }
        out.push_str(": ");
        out.push_str(&self.message);
        out.push('\n');

        // Context
        if let Some(ref ctx) = self.context {
            out.push_str("  --> ");
            out.push_str(ctx);
            out.push('\n');
        }

        // Suggestions
        for s in &self.suggestions {
            out.push_str("   = help: ");
            out.push_str(s);
            out.push('\n');
        }

        out
    }

    /// Render **with** ANSI escape codes.
    pub fn render_colored(&self) -> String {
        let color = self.level.color();
        let mut out = String::new();

        // Header
        out.push_str(BOLD);
        out.push_str(color);
        out.push_str(self.level.label());
        if let Some(ref c) = self.code {
            out.push('[');
            out.push_str(c);
            out.push(']');
        }
        out.push_str(RESET);
        out.push_str(BOLD);
        out.push_str(WHITE);
        out.push_str(": ");
        out.push_str(&self.message);
        out.push_str(RESET);
        out.push('\n');

        // Context
        if let Some(ref ctx) = self.context {
            out.push_str(CYAN);
            out.push_str("  --> ");
            out.push_str(ctx);
            out.push_str(RESET);
            out.push('\n');
        }

        // Suggestions
        for s in &self.suggestions {
            out.push_str(GREEN);
            out.push_str("   = help: ");
            out.push_str(s);
            out.push_str(RESET);
            out.push('\n');
        }

        out
    }

    /// Print this diagnostic to stderr.
    pub fn emit(&self) {
        eprint!("{}", self.display());
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render_plain())
    }
}

// ── Helper: strip ANSI codes (for tests) ─────────────────────────────

/// Strip all ANSI CSI escape sequences from `input`.
///
/// Useful in tests that need to assert on rendered diagnostic text
/// regardless of colour settings.
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // consume until we see a letter (the terminator of a CSI sequence)
            for inner in chars.by_ref() {
                if inner.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_error_with_code_and_suggestion() {
        let d = Diagnostic::new(DiagLevel::Error, "invalid address format")
            .code("E001")
            .context("--addr 999.999.999.999:99999")
            .suggestion("use HOST:PORT, e.g. 127.0.0.1:8080");

        let text = d.render_plain();
        assert!(text.starts_with("error[E001]: invalid address format\n"));
        assert!(text.contains("  --> --addr 999.999.999.999:99999"));
        assert!(text.contains("   = help: use HOST:PORT"));
    }

    #[test]
    fn plain_warning_no_code() {
        let d = Diagnostic::new(DiagLevel::Warning, "deprecated config key");
        let text = d.render_plain();
        assert!(text.starts_with("warning: deprecated config key\n"));
        // No code brackets
        assert!(!text.contains('['));
    }

    #[test]
    fn colored_output_contains_ansi() {
        let d = Diagnostic::new(DiagLevel::Info, "server listening").context("0.0.0.0:8080");
        let colored = d.render_colored();
        assert!(colored.contains("\x1b["));
        // Stripping ANSI should yield the plain version
        let stripped = strip_ansi(&colored);
        assert!(stripped.contains("info: server listening"));
        assert!(stripped.contains("  --> 0.0.0.0:8080"));
    }

    #[test]
    fn no_color_env_respected() {
        // Temporarily set NO_COLOR
        std::env::set_var("NO_COLOR", "1");
        let enabled = colors_enabled();
        std::env::remove_var("NO_COLOR");
        // NO_COLOR was set, so colours should have been disabled regardless of TTY.
        assert!(!enabled);
    }

    #[test]
    fn strip_ansi_roundtrip() {
        let raw = "\x1b[1m\x1b[31merror\x1b[0m: boom";
        let stripped = strip_ansi(raw);
        assert_eq!(stripped, "error: boom");
    }

    #[test]
    fn multiple_suggestions_rendered() {
        let d = Diagnostic::new(DiagLevel::Hint, "improve performance")
            .suggestion("enable compression")
            .suggestion("use HTTP/2");
        let text = d.render_plain();
        assert_eq!(text.matches("= help:").count(), 2);
    }

    #[test]
    fn display_trait_uses_plain() {
        let d = Diagnostic::new(DiagLevel::Error, "oops").code("E099");
        let via_display = format!("{d}");
        assert!(via_display.contains("error[E099]: oops"));
        // Display should never include ANSI
        assert!(!via_display.contains("\x1b["));
    }
}
