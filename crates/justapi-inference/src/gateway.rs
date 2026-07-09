//! K8s AI inference gateway — KV-aware + LoRA-aware routing *inside the gateway*.
//!
//! Composes the control-plane ([`ControlPlane`]) and the request router
//! ([`Router`]) into a deployable gateway unit: incoming inference requests are
//! resolved to a concrete model version + adapter, then steered to the best
//! serving replica. The gateway adds the Kubernetes-specific concerns the raw
//! router does not own:
//!
//! - **KV-aware default strategy** — the gateway routes by *lowest KV-cache
//!   pressure* by default, so hot replicas (large prefix caches, long in-flight
//!   decodes) are not overloaded. This is the "KV-aware routing inside the
//!   gateway" property (Dynamo / AIBrix behaviour).
//! - **Readiness gating** — a pod must be K8s-*Ready* to receive traffic; a
//!   not-Ready pod is excluded even if its liveness probe is fine. Readiness is
//!   reflected into the router's `healthy` flag (standard K8s mapping:
//!   not-Ready ⇒ not a routing candidate).
//! - **Endpoint resolution** — the routed replica is turned into a Kubernetes
//!   service DNS name (`<replica>.<namespace>.svc.cluster.local` by default) the
//!   gateway forwards the request to. The template is configurable and supports
//!   `{replica}`, `{model}`, `{version}`, `{namespace}`.
//!
//! This is pure decision logic (no network, no GPU) — the same structural
//! approach as the rest of the control plane. A K8s executor fulfils the
//! emitted routing decisions and reports pod readiness/load back via
//! [`InferenceGateway::set_ready`] / [`InferenceGateway::update_load`].

use crate::control_plane::{ControlPlane, ResolvedModel};
use crate::router::{Replica, RouteDecision, RouteRequest, Router, RoutingStrategy};

/// Configuration for the K8s inference gateway.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Kubernetes namespace the gateway serves (used in endpoint resolution and
    /// for isolating per-namespace routing).
    pub namespace: String,
    /// Service-DNS template. Supports `{replica}`, `{model}`, `{version}`,
    /// `{namespace}`. Defaults to `"{replica}.{namespace}.svc.cluster.local"`.
    pub service_template: String,
    /// Replica selection strategy. Defaults to [`RoutingStrategy::LowestKvPressure`]
    /// (the gateway's KV-aware default).
    pub strategy: RoutingStrategy,
    /// Whether K8s readiness gates routing (not-Ready pods excluded). Default true.
    pub require_ready: bool,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            namespace: "justapi".to_string(),
            service_template: "{replica}.{namespace}.svc.cluster.local".to_string(),
            strategy: RoutingStrategy::LowestKvPressure,
            require_ready: true,
        }
    }
}

/// The outcome of gateway routing: which replica + its Kubernetes endpoint.
#[derive(Debug, Clone)]
pub struct GatewayDecision {
    /// Chosen replica id (maps to a pod/deployment in the cluster).
    pub replica: String,
    /// Fully resolved model target (version + adapter).
    pub resolved: ResolvedModel,
    /// Kubernetes service endpoint the gateway should forward the request to.
    pub endpoint: String,
}

/// The K8s inference gateway: control-plane resolution + KV-aware/LoRA-aware
/// routing with readiness gating and endpoint resolution.
pub struct InferenceGateway {
    cp: ControlPlane,
    router: Router,
    config: GatewayConfig,
}

impl InferenceGateway {
    /// Create a gateway bound to `config`.
    pub fn new(config: GatewayConfig) -> Self {
        let cp = ControlPlane::new();
        let router = Router::new(config.strategy);
        Self { cp, router, config }
    }

    /// Borrow the embedded control plane (to register model versions/adapters).
    pub fn control_plane(&self) -> &ControlPlane {
        &self.cp
    }

    /// Borrow the embedded router (for advanced inspection).
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Register (or replace) a serving replica with the gateway.
    pub fn register_replica(&self, replica: Replica) {
        self.router.register_replica(replica);
    }

    /// Report a replica's live load back from the cluster.
    pub fn update_load(
        &self,
        id: &str,
        active_sequences: usize,
        kv_pressure_pct: f32,
        ready: bool,
    ) {
        // Readiness is folded into `healthy` so the router's candidate filter
        // enforces it; a not-Ready pod is not a routing candidate.
        let healthy = if self.config.require_ready {
            ready
        } else {
            true
        };
        self.router
            .update_load(id, active_sequences, kv_pressure_pct, healthy);
    }

    /// K8s readiness-probe callback: mark a pod Ready/NotReady for traffic.
    pub fn set_ready(&self, id: &str, ready: bool) {
        if let Some(r) = self.router.replicas().into_iter().find(|r| r.id == id) {
            self.update_load(id, r.active_sequences, r.kv_pressure_pct, ready);
        }
    }

    /// Remove a replica (pod deleted / drained).
    pub fn remove_replica(&self, id: &str) {
        self.router.remove_replica(id);
    }

    /// Route an inference request to the best serving replica.
    ///
    /// Resolves the model via the control plane, selects a replica with the
    /// configured (KV-aware / LoRA-aware) strategy, and returns the replica plus
    /// its Kubernetes endpoint. Returns `None` if the model is unknown or no
    /// replica can serve it (no capacity / not Ready).
    pub fn route(&self, request: &RouteRequest) -> Option<GatewayDecision> {
        let decision: RouteDecision = self.router.route(&self.cp, request)?;
        let endpoint = self.endpoint_for(&decision.replica, &decision.resolved);
        Some(GatewayDecision {
            replica: decision.replica,
            resolved: decision.resolved,
            endpoint,
        })
    }

    /// Resolve a replica id + resolved model into a Kubernetes service endpoint.
    fn endpoint_for(&self, replica: &str, resolved: &ResolvedModel) -> String {
        self.config
            .service_template
            .replace("{replica}", replica)
            .replace("{model}", &resolved.model_name)
            .replace("{version}", &resolved.version)
            .replace("{namespace}", &self.config.namespace)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_plane::{ModelVersion, WeightLocation};
    use crate::engine::EngineDevice;
    use std::path::PathBuf;

    fn gateway() -> InferenceGateway {
        let g = InferenceGateway::new(GatewayConfig {
            namespace: "llm-prod".to_string(),
            ..Default::default()
        });
        // Register a model with two adapter routes.
        g.control_plane().register_version(
            "llama-7b",
            ModelVersion::new("v1", WeightLocation::Local(PathBuf::from("/m/v1"))),
        );
        let mut record = g.control_plane().get("llama-7b").unwrap();
        record.route_adapter("domain:math", "math-adapter");
        record.route_adapter("domain:code", "code-adapter");
        g.control_plane().register(record);
        g
    }

    fn replica(id: &str, active: usize, kv: f32, adapters: &[&str]) -> Replica {
        Replica {
            id: id.to_string(),
            model: "llama-7b".to_string(),
            version: "v1".to_string(),
            device: EngineDevice::Cuda(0),
            max_concurrency: 4,
            active_sequences: active,
            kv_pressure_pct: kv,
            healthy: true,
            loaded_adapters: adapters.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn route_resolves_kubernetes_endpoint() {
        let g = gateway();
        g.register_replica(replica("llama-7b-v1-0", 0, 10.0, &[]));
        let d = g
            .route(&RouteRequest {
                model: "llama-7b".into(),
                version: None,
                routing_key: None,
            })
            .unwrap();
        assert_eq!(d.replica, "llama-7b-v1-0");
        assert_eq!(d.endpoint, "llama-7b-v1-0.llm-prod.svc.cluster.local");
        assert_eq!(d.resolved.version, "v1");
    }

    #[test]
    fn namespace_appears_in_endpoint() {
        let g = InferenceGateway::new(GatewayConfig {
            namespace: "team-a".to_string(),
            service_template: "justapi-{replica}.{namespace}.svc.cluster.local".to_string(),
            ..Default::default()
        });
        g.control_plane().register_version(
            "llama-7b",
            ModelVersion::new("v1", WeightLocation::Local(PathBuf::from("/m"))),
        );
        g.register_replica(replica("llama-7b-v1-0", 0, 10.0, &[]));
        let d = g
            .route(&RouteRequest {
                model: "llama-7b".into(),
                ..Default::default()
            })
            .unwrap();
        assert!(d.endpoint.contains("team-a"));
        assert!(d.endpoint.starts_with("justapi-llama-7b-v1-0"));
    }

    #[test]
    fn readiness_gating_excludes_not_ready_pod() {
        let g = gateway();
        g.register_replica(replica("hot", 0, 10.0, &[]));
        g.register_replica(replica("cool", 0, 10.0, &[]));
        // Drain "hot" (not Ready) → traffic must go to "cool".
        g.set_ready("hot", false);
        let d = g
            .route(&RouteRequest {
                model: "llama-7b".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(d.replica, "cool");

        // Drain both → no capacity.
        g.set_ready("cool", false);
        assert!(g
            .route(&RouteRequest {
                model: "llama-7b".into(),
                ..Default::default()
            })
            .is_none());
    }

    #[test]
    fn kv_pressure_default_routes_to_coolest() {
        // Gateway default strategy is LowestKvPressure.
        let g = gateway();
        g.register_replica(replica("hot", 1, 90.0, &[]));
        g.register_replica(replica("cool", 1, 20.0, &[]));
        let d = g
            .route(&RouteRequest {
                model: "llama-7b".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(d.replica, "cool");
    }

    #[test]
    fn lora_aware_routing_inside_gateway() {
        let g = gateway();
        // Only r2 holds the math-adapter → math routing key must land there.
        g.register_replica(replica("r1", 0, 10.0, &[]));
        g.register_replica(replica("r2", 0, 10.0, &["math-adapter"]));
        let d = g
            .route(&RouteRequest {
                model: "llama-7b".into(),
                routing_key: Some("domain:math".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(d.replica, "r2");
        assert_eq!(d.resolved.adapter.as_deref(), Some("math-adapter"));

        // code-adapter held by nobody → no capacity.
        assert!(g
            .route(&RouteRequest {
                model: "llama-7b".into(),
                routing_key: Some("domain:code".into()),
                ..Default::default()
            })
            .is_none());
    }
}
