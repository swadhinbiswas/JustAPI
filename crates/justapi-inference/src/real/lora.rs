//! LoRA adapter registry and routing.
//!
//! Pure data structures — no weight tensors. The registry decides which adapter
//! to activate for a given request and can resolve multi-LoRA routing keys.
//! Actual weight loading is handled by `model.rs` behind the `real` feature.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// LoRA rank configuration for a single adapter.
#[derive(Debug, Clone)]
pub struct LoraConfig {
    /// Adapter rank (typically 8–128).
    pub rank: usize,
    /// Alpha scaling factor (often equal to rank).
    pub alpha: usize,
    /// Dropout probability (0.0 = no dropout; used during training only).
    pub dropout: f32,
    /// Target module name patterns (e.g. "q_proj", "v_proj", "gate_proj").
    pub target_modules: Vec<String>,
}

/// A loaded LoRA adapter (weights live in `model.rs` behind `real` feature).
#[derive(Debug, Clone)]
pub struct LoraAdapter {
    /// Unique adapter name (e.g. "adapter_v2_math").
    pub name: String,
    /// Optional base model hash — adapters are only valid for matching bases.
    pub base_model_hash: Option<String>,
    /// LoRA hyper-parameters.
    pub config: LoraConfig,
    /// Routing key used by the multi-LoRA scheduler. If set, requests matching
    /// this key are routed to this adapter.
    pub routing_key: Option<String>,
    /// Adapter is active and ready to serve.
    pub active: bool,
}

impl LoraAdapter {
    /// Create a new adapter with sensible defaults.
    pub fn new(name: impl Into<String>, rank: usize, alpha: usize) -> Self {
        Self {
            name: name.into(),
            base_model_hash: None,
            config: LoraConfig {
                rank,
                alpha,
                dropout: 0.0,
                target_modules: Vec::new(),
            },
            routing_key: None,
            active: true,
        }
    }

    /// Builder-style setter for routing key.
    pub fn with_routing_key(mut self, key: impl Into<String>) -> Self {
        self.routing_key = Some(key.into());
        self
    }

    /// Builder-style setter for base model hash.
    pub fn with_base_model(mut self, hash: impl Into<String>) -> Self {
        self.base_model_hash = Some(hash.into());
        self
    }

    /// Builder-style setter for target modules.
    pub fn with_targets(mut self, modules: Vec<String>) -> Self {
        self.config.target_modules = modules;
        self
    }

    /// Effective scaling factor: `alpha / rank`.
    pub fn scaling(&self) -> f32 {
        self.config.alpha as f32 / self.config.rank.max(1) as f32
    }
}

/// Thread-safe registry for managing multiple LoRA adapters.
#[derive(Clone)]
pub struct LoraRegistry {
    inner: Arc<RwLock<LoraRegistryInner>>,
}

struct LoraRegistryInner {
    adapters: HashMap<String, LoraAdapter>,
    /// Routing-key → adapter-name lookup (built on register/unregister).
    routing: HashMap<String, String>,
}

impl LoraRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(LoraRegistryInner {
                adapters: HashMap::new(),
                routing: HashMap::new(),
            })),
        }
    }

    /// Register an adapter. Replaces any existing adapter with the same name.
    pub fn register(&self, adapter: LoraAdapter) {
        let mut inner = self.inner.write().unwrap();
        if let Some(key) = &adapter.routing_key {
            inner.routing.insert(key.clone(), adapter.name.clone());
        }
        inner.adapters.insert(adapter.name.clone(), adapter);
    }

    /// Unregister an adapter by name.
    pub fn unregister(&self, name: &str) -> Option<LoraAdapter> {
        let mut inner = self.inner.write().unwrap();
        let adapter = inner.adapters.remove(name)?;
        if let Some(key) = &adapter.routing_key {
            inner.routing.remove(key);
        }
        Some(adapter)
    }

    /// Look up an adapter by name.
    pub fn get(&self, name: &str) -> Option<LoraAdapter> {
        self.inner.read().unwrap().adapters.get(name).cloned()
    }

    /// Look up an adapter by routing key.
    pub fn get_by_routing_key(&self, key: &str) -> Option<LoraAdapter> {
        let inner = self.inner.read().unwrap();
        let name = inner.routing.get(key)?;
        inner.adapters.get(name).cloned()
    }

    /// List all active adapter names.
    pub fn active_adapters(&self) -> Vec<String> {
        self.inner
            .read()
            .unwrap()
            .adapters
            .values()
            .filter(|a| a.active)
            .map(|a| a.name.clone())
            .collect()
    }

    /// List all adapter names (active + inactive).
    pub fn all_adapters(&self) -> Vec<String> {
        self.inner
            .read()
            .unwrap()
            .adapters
            .keys()
            .cloned()
            .collect()
    }

    /// Number of registered adapters.
    pub fn len(&self) -> usize {
        self.inner.read().unwrap().adapters.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.read().unwrap().adapters.is_empty()
    }

    /// Activate an adapter by name. Returns `false` if not found.
    pub fn activate(&self, name: &str) -> bool {
        let mut inner = self.inner.write().unwrap();
        if let Some(a) = inner.adapters.get_mut(name) {
            a.active = true;
            true
        } else {
            false
        }
    }

    /// Deactivate an adapter by name. Returns `false` if not found.
    pub fn deactivate(&self, name: &str) -> bool {
        let mut inner = self.inner.write().unwrap();
        if let Some(a) = inner.adapters.get_mut(name) {
            a.active = false;
            true
        } else {
            false
        }
    }

    /// Resolve which adapter to use for a request.
    ///
    /// Priority: explicit adapter name → routing key → `None` (base model).
    pub fn resolve(
        &self,
        adapter_name: Option<&str>,
        routing_key: Option<&str>,
    ) -> Option<LoraAdapter> {
        if let Some(name) = adapter_name {
            let adapter = self.get(name)?;
            return if adapter.active { Some(adapter) } else { None };
        }
        if let Some(key) = routing_key {
            let adapter = self.get_by_routing_key(key)?;
            return if adapter.active { Some(adapter) } else { None };
        }
        None
    }

    /// Check if any registered adapter targets a given module.
    pub fn active_for_module(&self, module_name: &str) -> bool {
        self.inner
            .read()
            .unwrap()
            .adapters
            .values()
            .filter(|a| a.active)
            .any(|a| a.config.target_modules.iter().any(|m| m == module_name))
    }
}

impl Default for LoraRegistry {
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

    #[test]
    fn adapter_new_and_scaling() {
        let a = LoraAdapter::new("math", 64, 128);
        assert_eq!(a.name, "math");
        assert_eq!(a.config.rank, 64);
        assert_eq!(a.config.alpha, 128);
        assert!((a.scaling() - 2.0).abs() < f32::EPSILON);
        assert!(a.active);
    }

    #[test]
    fn adapter_builder_chain() {
        let a = LoraAdapter::new("code", 32, 32)
            .with_routing_key("task:code")
            .with_base_model("llama-7b-hash")
            .with_targets(vec!["q_proj".into(), "v_proj".into()]);
        assert_eq!(a.routing_key.as_deref(), Some("task:code"));
        assert_eq!(a.base_model_hash.as_deref(), Some("llama-7b-hash"));
        assert_eq!(a.config.target_modules, vec!["q_proj", "v_proj"]);
    }

    #[test]
    fn registry_register_and_get() {
        let reg = LoraRegistry::new();
        assert!(reg.is_empty());
        reg.register(LoraAdapter::new("a", 8, 8));
        reg.register(LoraAdapter::new("b", 16, 16));
        assert_eq!(reg.len(), 2);
        assert!(reg.get("a").is_some());
        assert!(reg.get("c").is_none());
    }

    #[test]
    fn registry_routing_key() {
        let reg = LoraRegistry::new();
        reg.register(LoraAdapter::new("math_adapter", 32, 32).with_routing_key("domain:math"));
        let found = reg.get_by_routing_key("domain:math");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "math_adapter");
        assert!(reg.get_by_routing_key("domain:code").is_none());
    }

    #[test]
    fn registry_activate_deactivate() {
        let reg = LoraRegistry::new();
        reg.register(LoraAdapter::new("a", 8, 8));
        assert!(reg.activate("a"));
        assert!(reg.deactivate("a"));
        assert!(!reg.get("a").unwrap().active);
        assert!(reg.active_adapters().is_empty());
        assert_eq!(reg.all_adapters(), vec!["a"]);
    }

    #[test]
    fn registry_resolve_priority() {
        let reg = LoraRegistry::new();
        reg.register(LoraAdapter::new("explicit", 8, 8).with_routing_key("r1"));
        reg.register(LoraAdapter::new("routed", 8, 8).with_routing_key("r2"));

        // Explicit name takes priority.
        let a = reg.resolve(Some("explicit"), Some("r2"));
        assert_eq!(a.unwrap().name, "explicit");

        // Fallback to routing key.
        let a = reg.resolve(None, Some("r2"));
        assert_eq!(a.unwrap().name, "routed");

        // No match → None.
        assert!(reg.resolve(None, None).is_none());
    }

    #[test]
    fn registry_resolve_inactive_returns_none() {
        let reg = LoraRegistry::new();
        reg.register(LoraAdapter::new("a", 8, 8).with_routing_key("k"));
        reg.deactivate("a");
        assert!(reg.resolve(Some("a"), None).is_none());
        assert!(reg.resolve(None, Some("k")).is_none());
    }

    #[test]
    fn registry_active_for_module() {
        let reg = LoraRegistry::new();
        reg.register(
            LoraAdapter::new("a", 8, 8).with_targets(vec!["q_proj".into(), "v_proj".into()]),
        );
        assert!(reg.active_for_module("q_proj"));
        assert!(reg.active_for_module("v_proj"));
        assert!(!reg.active_for_module("gate_proj"));
    }

    #[test]
    fn registry_unregister() {
        let reg = LoraRegistry::new();
        reg.register(LoraAdapter::new("a", 8, 8).with_routing_key("k1"));
        let removed = reg.unregister("a");
        assert!(removed.is_some());
        assert!(reg.is_empty());
        assert!(reg.get_by_routing_key("k1").is_none());
    }
}
