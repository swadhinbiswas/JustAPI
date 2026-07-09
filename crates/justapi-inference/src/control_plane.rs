//! LLM Control Plane — model registry, versioning, and LoRA-aware resolution.
//!
//! Pure-data, always-compiled layer (no weights, no GPU). This is the
//! orchestration "brain" (Ray Serve / Dynamo / AIBrix equivalent) that decides,
//! for a given request, which concrete model version + adapter to serve and
//! with what runtime profile. The actual weight loading is performed by the
//! [`crate::Engine`] using the resolved [`WeightLocation`].
//!
//! Multi-replica supervision, KV-aware routing, and autoscaling build on top of
//! this registry (tracked for later in Phase 46).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::engine::EngineDevice;
use crate::real::quant::QuantMethod;

/// Where a model version's weights physically live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeightLocation {
    /// Local filesystem directory or file.
    Local(PathBuf),
    /// Object storage (S3 / GCS / MinIO compatible).
    S3 { bucket: String, key: String },
    /// HuggingFace Hub repo (optional revision / branch).
    HuggingFace {
        repo: String,
        revision: Option<String>,
    },
    /// A remote model registry served over HTTP(S).
    RemoteRegistry { url: String },
}

/// How a model version should be served.
#[derive(Debug, Clone)]
pub struct RuntimeProfile {
    /// Device the model is pinned to.
    pub device: EngineDevice,
    /// Max concurrent sequences that may share the model.
    pub max_concurrency: usize,
    /// Max total tokens in a single scheduler batch (prefill + decode).
    pub max_batch_tokens: usize,
    /// Estimated GPU memory footprint in MiB (for placement / admission).
    pub gpu_memory_mib: usize,
    /// Quantization method this profile targets.
    pub quant_method: QuantMethod,
    /// Adapter names expected to be resident (for LoRA-dense serving).
    pub expected_adapters: Vec<String>,
}

impl Default for RuntimeProfile {
    fn default() -> Self {
        Self {
            device: EngineDevice::Cpu,
            max_concurrency: 1,
            max_batch_tokens: 2048,
            gpu_memory_mib: 0,
            quant_method: QuantMethod::None,
            expected_adapters: Vec::new(),
        }
    }
}

/// A single versioned model artifact.
#[derive(Debug, Clone)]
pub struct ModelVersion {
    /// Version identifier (e.g. "v1", "2024-01-01", or a content hash).
    pub version: String,
    /// Where the weights live.
    pub weight_location: WeightLocation,
    /// Runtime profile for this version.
    pub runtime_profile: RuntimeProfile,
    /// Creation time as Unix seconds (for `latest` resolution).
    pub created_at_unix: u64,
    /// Alternate names that also resolve to this version (e.g. "stable").
    pub aliases: Vec<String>,
}

impl ModelVersion {
    /// Create a version with the default runtime profile.
    pub fn new(version: impl Into<String>, weight_location: WeightLocation) -> Self {
        Self {
            version: version.into(),
            weight_location,
            runtime_profile: RuntimeProfile::default(),
            created_at_unix: 0,
            aliases: Vec::new(),
        }
    }

    /// Builder: attach a runtime profile.
    pub fn with_profile(mut self, profile: RuntimeProfile) -> Self {
        self.runtime_profile = profile;
        self
    }

    /// Builder: set creation timestamp.
    pub fn with_created_at(mut self, unix: u64) -> Self {
        self.created_at_unix = unix;
        self
    }

    /// Builder: attach aliases.
    pub fn with_aliases(mut self, aliases: Vec<String>) -> Self {
        self.aliases = aliases;
        self
    }

    /// Whether `name` matches this version or one of its aliases.
    pub fn matches(&self, name: &str) -> bool {
        self.version == name || self.aliases.iter().any(|a| a == name)
    }
}

/// A registered model: a name with multiple versions + adapter routing.
#[derive(Debug, Clone)]
pub struct ModelRecord {
    /// Canonical model name (e.g. "llama-7b").
    pub name: String,
    /// All known versions of this model.
    pub versions: Vec<ModelVersion>,
    /// The version used when none is requested.
    pub default_version: String,
    /// LoRA routing key → adapter name (resolved at request time).
    pub adapter_routing: HashMap<String, String>,
}

impl ModelRecord {
    /// Create an empty record (no versions yet).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            versions: Vec::new(),
            default_version: String::new(),
            adapter_routing: HashMap::new(),
        }
    }

    /// Add (or replace) a version.
    pub fn add_version(&mut self, version: ModelVersion) {
        if let Some(slot) = self
            .versions
            .iter_mut()
            .find(|v| v.version == version.version)
        {
            *slot = version;
        } else {
            self.versions.push(version);
        }
        if self.default_version.is_empty() {
            self.default_version = self.versions.last().unwrap().version.clone();
        }
    }

    /// Set the default version (must already exist).
    pub fn set_default(&mut self, version: &str) -> bool {
        if self.versions.iter().any(|v| v.matches(version)) {
            self.default_version = version.to_string();
            true
        } else {
            false
        }
    }

    /// Map a routing key to an adapter name for this model.
    pub fn route_adapter(&mut self, routing_key: impl Into<String>, adapter: impl Into<String>) {
        self.adapter_routing
            .insert(routing_key.into(), adapter.into());
    }

    /// Resolve a version-or-alias string to a concrete version.
    pub fn resolve_version(&self, version_or_alias: &str) -> Option<&ModelVersion> {
        self.versions.iter().find(|v| v.matches(version_or_alias))
    }

    /// The newest version by `created_at_unix` (ties broken by insertion order).
    pub fn latest_version(&self) -> Option<&ModelVersion> {
        self.versions.iter().max_by_key(|v| v.created_at_unix)
    }

    /// Resolve a routing key to an adapter name.
    pub fn resolve_adapter(&self, routing_key: &str) -> Option<&str> {
        self.adapter_routing.get(routing_key).map(|s| s.as_str())
    }
}

/// A fully resolved serving target produced by [`ControlPlane::resolve`].
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub model_name: String,
    pub version: String,
    pub weight_location: WeightLocation,
    pub runtime_profile: RuntimeProfile,
    pub adapter: Option<String>,
}

/// Thread-safe, multi-tenant model registry.
#[derive(Clone)]
pub struct ControlPlane {
    inner: Arc<RwLock<HashMap<String, ModelRecord>>>,
}

impl ControlPlane {
    /// Create an empty control plane.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register (or replace) a whole model record.
    pub fn register(&self, record: ModelRecord) {
        self.inner
            .write()
            .unwrap()
            .insert(record.name.clone(), record);
    }

    /// Register a single version under an existing (or new) model.
    pub fn register_version(&self, model: &str, version: ModelVersion) {
        let mut inner = self.inner.write().unwrap();
        let record = inner
            .entry(model.to_string())
            .or_insert_with(|| ModelRecord::new(model));
        record.add_version(version);
    }

    /// Fetch a model record by name.
    pub fn get(&self, name: &str) -> Option<ModelRecord> {
        self.inner.read().unwrap().get(name).cloned()
    }

    /// List all registered model names.
    pub fn list(&self) -> Vec<String> {
        self.inner.read().unwrap().keys().cloned().collect()
    }

    /// Number of registered models.
    pub fn len(&self) -> usize {
        self.inner.read().unwrap().len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.read().unwrap().is_empty()
    }

    /// Unregister a model, returning it if present.
    pub fn unregister(&self, name: &str) -> Option<ModelRecord> {
        self.inner.write().unwrap().remove(name)
    }

    /// Resolve a model + optional version/alias + optional LoRA routing key into
    /// a concrete serving target.
    ///
    /// Resolution order for the version:
    /// - If `version_or_alias` is given, it **must** match a known version or
    ///   alias (strict — a wrong explicit request fails rather than silently
    ///   serving a different version).
    /// - If omitted, fall back to the model's `default_version`, then to the
    ///   latest version by `created_at_unix`.
    ///
    /// The adapter is resolved from the routing key against the model's
    /// `adapter_routing` map (LoRA-aware routing).
    pub fn resolve(
        &self,
        model: &str,
        version_or_alias: Option<&str>,
        routing_key: Option<&str>,
    ) -> Option<ResolvedModel> {
        let inner = self.inner.read().unwrap();
        let record = inner.get(model)?;

        let version = match version_or_alias {
            Some(v) => record.resolve_version(v)?, // strict: explicit miss → None
            None => record
                .resolve_version(&record.default_version)
                .or_else(|| record.latest_version())?,
        };

        let adapter = routing_key
            .and_then(|k| record.resolve_adapter(k))
            .map(String::from);

        Some(ResolvedModel {
            model_name: record.name.clone(),
            version: version.version.clone(),
            weight_location: version.weight_location.clone(),
            runtime_profile: version.runtime_profile.clone(),
            adapter,
        })
    }
}

impl Default for ControlPlane {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn version(v: &str, at: u64) -> ModelVersion {
        ModelVersion::new(
            v,
            WeightLocation::Local(PathBuf::from(format!("/models/{v}"))),
        )
        .with_created_at(at)
    }

    #[test]
    fn register_and_get_model() {
        let cp = ControlPlane::new();
        assert!(cp.is_empty());
        cp.register_version("llama-7b", version("v1", 100));
        cp.register_version("llama-7b", version("v2", 200));
        assert_eq!(cp.len(), 1);
        assert_eq!(cp.list(), vec!["llama-7b"]);
        assert!(cp.get("llama-7b").is_some());
    }

    #[test]
    fn default_version_is_first_registered() {
        let cp = ControlPlane::new();
        cp.register_version("m", version("v1", 100));
        cp.register_version("m", version("v2", 200));
        let r = cp.get("m").unwrap();
        assert_eq!(r.default_version, "v1");
    }

    #[test]
    fn set_default_version() {
        let cp = ControlPlane::new();
        cp.register_version("m", version("v1", 100));
        cp.register_version("m", version("v2", 200));
        let mut r = cp.get("m").unwrap();
        assert!(r.set_default("v2"));
        assert!(!r.set_default("nope"));
    }

    #[test]
    fn resolve_by_version_then_alias_then_default_then_latest() {
        let cp = ControlPlane::new();
        cp.register_version("m", version("v1", 100).with_aliases(vec!["stable".into()]));
        cp.register_version("m", version("v2", 200));

        // explicit version
        let r = cp.resolve("m", Some("v2"), None).unwrap();
        assert_eq!(r.version, "v2");

        // alias
        let r = cp.resolve("m", Some("stable"), None).unwrap();
        assert_eq!(r.version, "v1");

        // default (v1, since it was registered first)
        let r = cp.resolve("m", None, None).unwrap();
        assert_eq!(r.version, "v1");

        // missing model → None
        assert!(cp.resolve("missing", None, None).is_none());
    }

    #[test]
    fn resolve_explicit_miss_is_strict_none() {
        let cp = ControlPlane::new();
        cp.register_version("m", version("v1", 100));
        cp.register_version("m", version("v2", 200));
        // A wrong explicit version must fail loudly, not silently fall back.
        assert!(cp.resolve("m", Some("ghost"), None).is_none());
    }

    #[test]
    fn resolve_no_version_falls_back_to_default_then_latest() {
        let cp = ControlPlane::new();
        // Build a record whose default_version is empty so resolution must fall
        // through to the latest version by created_at.
        let mut record = ModelRecord::new("m");
        record.add_version(version("v1", 100));
        record.add_version(version("v3", 200));
        record.add_version(version("v2", 300));
        record.default_version = String::new();
        cp.register(record);

        let r = cp.resolve("m", None, None).unwrap();
        assert_eq!(r.version, "v2"); // latest by created_at
    }

    #[test]
    fn lora_aware_routing_resolves_adapter() {
        let cp = ControlPlane::new();
        cp.register_version("m", version("v1", 100));
        let mut record = cp.get("m").unwrap();
        record.route_adapter("domain:math", "math-adapter");
        record.route_adapter("domain:code", "code-adapter");
        cp.register(record);

        // routing key matches → adapter resolved
        let r = cp.resolve("m", None, Some("domain:math")).unwrap();
        assert_eq!(r.adapter.as_deref(), Some("math-adapter"));

        // routing key matches → different adapter
        let r = cp.resolve("m", None, Some("domain:code")).unwrap();
        assert_eq!(r.adapter.as_deref(), Some("code-adapter"));

        // no routing key → no adapter
        let r = cp.resolve("m", None, None).unwrap();
        assert!(r.adapter.is_none());

        // unknown routing key → no adapter
        let r = cp.resolve("m", None, Some("domain:unknown")).unwrap();
        assert!(r.adapter.is_none());
    }

    #[test]
    fn resolve_carries_runtime_profile_and_location() {
        let cp = ControlPlane::new();
        let profile = RuntimeProfile {
            device: EngineDevice::Cuda(0),
            max_concurrency: 8,
            max_batch_tokens: 4096,
            gpu_memory_mib: 14_000,
            quant_method: QuantMethod::Gguf,
            expected_adapters: vec!["math-adapter".into()],
        };
        cp.register_version(
            "m",
            version("v1", 100)
                .with_profile(profile.clone())
                .with_aliases(vec!["latest".into()]),
        );
        let r = cp.resolve("m", Some("latest"), None).unwrap();
        assert_eq!(r.runtime_profile.device, EngineDevice::Cuda(0));
        assert_eq!(r.runtime_profile.max_concurrency, 8);
        assert_eq!(r.runtime_profile.quant_method, QuantMethod::Gguf);
        assert_eq!(
            r.weight_location,
            WeightLocation::Local(PathBuf::from("/models/v1"))
        );
        assert_eq!(r.runtime_profile.expected_adapters, vec!["math-adapter"]);
    }

    #[test]
    fn unregister_removes_model() {
        let cp = ControlPlane::new();
        cp.register_version("m", version("v1", 100));
        let removed = cp.unregister("m");
        assert!(removed.is_some());
        assert!(cp.is_empty());
        assert!(cp.unregister("m").is_none());
    }
}
