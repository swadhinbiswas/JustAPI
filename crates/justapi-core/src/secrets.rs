use std::collections::HashMap;
use std::sync::Arc;

/// How a secret is sourced.
#[derive(Debug, Clone)]
pub enum SecretSource {
    /// Read from an environment variable.
    Env(String),
    /// Read from a file on disk (e.g. Docker secrets in `/run/secrets/`).
    File(std::path::PathBuf),
    /// Inline value (only for development; never commit these).
    Inline(String),
}

/// A named secret that knows how to resolve itself at runtime.
#[derive(Debug, Clone)]
pub struct Secret {
    source: SecretSource,
    name: String,
}

impl Secret {
    pub fn env(var_name: &str) -> Self {
        Self { source: SecretSource::Env(var_name.to_string()), name: var_name.to_string() }
    }

    pub fn file(path: impl Into<std::path::PathBuf>) -> Self {
        let path = path.into();
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        Self { source: SecretSource::File(path), name }
    }

    pub fn inline(value: &str, label: &str) -> Self {
        Self { source: SecretSource::Inline(value.to_string()), name: label.to_string() }
    }

    /// Resolve the secret to its string value.
    ///
    /// Tries env var first, then file, then returns the inline value.
    /// Never logs the resolved value (only the source name).
    pub fn resolve(&self) -> Result<String, anyhow::Error> {
        match &self.source {
            SecretSource::Env(var) => {
                std::env::var(var).map_err(|_| anyhow::anyhow!("env var {} not set", var))
            }
            SecretSource::File(path) => std::fs::read_to_string(path)
                .map(|s| s.trim().to_string())
                .map_err(|e| anyhow::anyhow!("cannot read secret file {}: {}", path.display(), e)),
            SecretSource::Inline(value) => Ok(value.clone()),
        }
    }
}

/// A registry of secrets that can be resolved on demand.
///
/// Secrets are resolved lazily (not at startup) so that the application can
/// start even if a secret source is temporarily unavailable. Use
/// [`SecretsRegistry::resolve_all()`] for eager validation at startup.
#[derive(Debug, Clone, Default)]
pub struct SecretsRegistry {
    secrets: Arc<std::sync::RwLock<HashMap<String, Secret>>>,
}

impl SecretsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, secret: Secret) {
        let mut map = self.secrets.write().unwrap_or_else(|e| e.into_inner());
        map.insert(secret.name.clone(), secret);
    }

    pub fn resolve(&self, name: &str) -> Result<String, anyhow::Error> {
        let map = self.secrets.read().unwrap_or_else(|e| e.into_inner());
        match map.get(name) {
            Some(secret) => secret.resolve(),
            None => anyhow::bail!("secret '{}' not registered", name),
        }
    }

    /// Resolve all secrets eagerly. Useful for startup validation.
    pub fn resolve_all(&self) -> Vec<(String, Result<String, anyhow::Error>)> {
        let map = self.secrets.read().unwrap_or_else(|e| e.into_inner());
        map.iter().map(|(name, secret)| (name.clone(), secret.resolve())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_inline() {
        let s = Secret::inline("my-value", "test");
        assert_eq!(s.resolve().unwrap(), "my-value");
    }

    #[test]
    fn test_secret_env() {
        let s = Secret::env("PATH");
        assert!(s.resolve().is_ok());
    }

    #[test]
    fn test_secret_env_missing() {
        let s = Secret::env("__DOES_NOT_EXIST_12345__");
        assert!(s.resolve().is_err());
    }

    #[test]
    fn test_secret_file_missing() {
        let s = Secret::file("/tmp/__does_not_exist_12345__");
        assert!(s.resolve().is_err());
    }

    #[test]
    fn test_registry_resolve() {
        let reg = SecretsRegistry::new();
        reg.register(Secret::inline("val", "my-key"));
        assert_eq!(reg.resolve("my-key").unwrap(), "val");
    }

    #[test]
    fn test_registry_missing() {
        let reg = SecretsRegistry::new();
        assert!(reg.resolve("nonexistent").is_err());
    }

    #[test]
    fn test_registry_resolve_all() {
        let reg = SecretsRegistry::new();
        reg.register(Secret::inline("a", "key-a"));
        reg.register(Secret::env("PATH"));
        reg.register(Secret::file("/tmp/__nonexistent__"));

        let results = reg.resolve_all();
        assert_eq!(results.len(), 3);
    }
}
