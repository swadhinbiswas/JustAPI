//! LLM request router — LoRA-aware and KV-pressure-aware replica selection.
//!
//! Pure decision logic (no network, no GPU). Built on top of the [`ControlPlane`]
//! registry: a request names a model (+ optional version + LoRA routing key), the
//! control plane resolves it to a concrete [`ResolvedModel`], and the router picks
//! the best *replica* currently able to serve it.
//!
//! - **LoRA-aware:** if a routing key resolved to an adapter, only replicas that
//!   already have that adapter resident are considered (avoids a cold LoRA load).
//! - **KV-aware:** among healthy replicas with spare capacity, selection honors
//!   the configured [`RoutingStrategy`] (least-loaded, lowest KV pressure, or
//!   round-robin) so hot replicas are not overloaded.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::control_plane::{ControlPlane, ResolvedModel};
use crate::engine::EngineDevice;

/// A single serving replica (one model version on one device/process).
#[derive(Debug, Clone)]
pub struct Replica {
    /// Unique replica id (e.g. "llama-7b-v2-gpu0").
    pub id: String,
    /// Model name this replica serves.
    pub model: String,
    /// Model version this replica serves.
    pub version: String,
    /// Device the replica is pinned to.
    pub device: EngineDevice,
    /// Max concurrent sequences this replica admits.
    pub max_concurrency: usize,
    /// Currently active sequences (drives load-aware selection).
    pub active_sequences: usize,
    /// KV-cache pressure as a percentage (0–100), drives KV-aware selection.
    pub kv_pressure_pct: f32,
    /// Whether the replica is healthy and accepting traffic.
    pub healthy: bool,
    /// Adapter names currently resident on this replica.
    pub loaded_adapters: Vec<String>,
}

impl Replica {
    /// Spare capacity before the replica hits `max_concurrency`.
    pub fn available_capacity(&self) -> usize {
        self.max_concurrency.saturating_sub(self.active_sequences)
    }

    /// A replica is overloaded when it has no spare concurrency or its KV cache
    /// is near saturation.
    pub fn is_overloaded(&self) -> bool {
        self.active_sequences >= self.max_concurrency || self.kv_pressure_pct >= 95.0
    }
}

/// An incoming routing request (what the client asked for).
#[derive(Debug, Clone, Default)]
pub struct RouteRequest {
    pub model: String,
    pub version: Option<String>,
    pub routing_key: Option<String>,
}

/// The outcome of routing: which replica + the fully resolved model target.
#[derive(Debug, Clone)]
pub struct RouteDecision {
    pub replica: String,
    pub resolved: ResolvedModel,
}

/// How the router breaks ties among candidate replicas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStrategy {
    /// Fewest active sequences first.
    LeastLoaded,
    /// Lowest KV-cache pressure first.
    LowestKvPressure,
    /// Round-robin across healthy candidates (spreads load evenly).
    RoundRobin,
}

/// The router: owns its view of live replicas and selects among them.
pub struct Router {
    strategy: RoutingStrategy,
    replicas: Mutex<Vec<Replica>>,
    rr: AtomicUsize,
}

impl Router {
    /// Create a router with the given strategy.
    pub fn new(strategy: RoutingStrategy) -> Self {
        Self {
            strategy,
            replicas: Mutex::new(Vec::new()),
            rr: AtomicUsize::new(0),
        }
    }

    /// Register or replace a replica by id.
    pub fn register_replica(&self, replica: Replica) {
        let mut guard = self.replicas.lock().unwrap();
        if let Some(slot) = guard.iter_mut().find(|r| r.id == replica.id) {
            *slot = replica;
        } else {
            guard.push(replica);
        }
    }

    /// Update a replica's live load metrics (no-op if the id is unknown).
    pub fn update_load(
        &self,
        id: &str,
        active_sequences: usize,
        kv_pressure_pct: f32,
        healthy: bool,
    ) {
        let mut guard = self.replicas.lock().unwrap();
        if let Some(r) = guard.iter_mut().find(|r| r.id == id) {
            r.active_sequences = active_sequences;
            r.kv_pressure_pct = kv_pressure_pct;
            r.healthy = healthy;
        }
    }

    /// Report which adapters are resident on a replica (no-op if unknown).
    pub fn set_loaded_adapters(&self, id: &str, adapters: Vec<String>) {
        let mut guard = self.replicas.lock().unwrap();
        if let Some(r) = guard.iter_mut().find(|r| r.id == id) {
            r.loaded_adapters = adapters;
        }
    }

    /// Remove a replica by id.
    pub fn remove_replica(&self, id: &str) -> Option<Replica> {
        let mut guard = self.replicas.lock().unwrap();
        let idx = guard.iter().position(|r| r.id == id)?;
        Some(guard.remove(idx))
    }

    /// All currently registered replicas (snapshot).
    pub fn replicas(&self) -> Vec<Replica> {
        self.replicas.lock().unwrap().clone()
    }

    /// Replicas eligible to serve `resolved`: matching model + version, healthy,
    /// not overloaded, and — if an adapter is required — already holding it.
    fn candidates(&self, resolved: &ResolvedModel) -> Vec<Replica> {
        self.replicas
            .lock()
            .unwrap()
            .iter()
            .filter(|r| {
                r.model == resolved.model_name
                    && r.version == resolved.version
                    && r.healthy
                    && !r.is_overloaded()
                    && resolved
                        .adapter
                        .as_ref()
                        .map(|a| r.loaded_adapters.iter().any(|la| la == a))
                        .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    /// Route a request: resolve it via the control plane, then select the best
    /// replica. Returns `None` if the model is unknown or no replica can serve it.
    pub fn route(&self, cp: &ControlPlane, request: &RouteRequest) -> Option<RouteDecision> {
        let resolved = cp.resolve(
            &request.model,
            request.version.as_deref(),
            request.routing_key.as_deref(),
        )?;
        let mut candidates = self.candidates(&resolved);
        if candidates.is_empty() {
            return None;
        }
        let chosen = match self.strategy {
            RoutingStrategy::LeastLoaded => candidates
                .into_iter()
                .min_by_key(|r| r.active_sequences)
                .unwrap(),
            RoutingStrategy::LowestKvPressure => candidates
                .into_iter()
                .min_by(|a, b| {
                    a.kv_pressure_pct
                        .partial_cmp(&b.kv_pressure_pct)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap(),
            RoutingStrategy::RoundRobin => {
                let n = candidates.len();
                let idx = self.rr.fetch_add(1, Ordering::Relaxed) % n;
                candidates.swap_remove(idx)
            }
        };
        Some(RouteDecision {
            replica: chosen.id,
            resolved,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_plane::{ControlPlane, WeightLocation};
    use std::path::PathBuf;

    fn cp_with_model() -> ControlPlane {
        let cp = ControlPlane::new();
        cp.register_version(
            "llama-7b",
            crate::control_plane::ModelVersion::new(
                "v1",
                WeightLocation::Local(PathBuf::from("/models/v1")),
            ),
        );
        let mut record = cp.get("llama-7b").unwrap();
        record.route_adapter("domain:math", "math-adapter");
        record.route_adapter("domain:code", "code-adapter");
        cp.register(record);
        cp
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
    fn routes_to_matching_healthy_replica() {
        let cp = cp_with_model();
        let router = Router::new(RoutingStrategy::LeastLoaded);
        router.register_replica(replica("r1", 1, 10.0, &[]));
        let d = router
            .route(
                &cp,
                &RouteRequest {
                    model: "llama-7b".into(),
                    version: None,
                    routing_key: None,
                },
            )
            .unwrap();
        assert_eq!(d.replica, "r1");
        assert_eq!(d.resolved.version, "v1");
    }

    #[test]
    fn least_loaded_picks_fewest_active() {
        let cp = cp_with_model();
        let router = Router::new(RoutingStrategy::LeastLoaded);
        router.register_replica(replica("busy", 3, 10.0, &[]));
        router.register_replica(replica("idle", 0, 80.0, &[]));
        let d = router
            .route(
                &cp,
                &RouteRequest {
                    model: "llama-7b".into(),
                    version: None,
                    routing_key: None,
                },
            )
            .unwrap();
        assert_eq!(d.replica, "idle"); // fewer active sequences wins
    }

    #[test]
    fn lowest_kv_pressure_picks_emptier_cache() {
        let cp = cp_with_model();
        let router = Router::new(RoutingStrategy::LowestKvPressure);
        router.register_replica(replica("hot", 1, 90.0, &[]));
        router.register_replica(replica("cool", 1, 20.0, &[]));
        let d = router
            .route(
                &cp,
                &RouteRequest {
                    model: "llama-7b".into(),
                    version: None,
                    routing_key: None,
                },
            )
            .unwrap();
        assert_eq!(d.replica, "cool");
    }

    #[test]
    fn lora_aware_only_considers_replicas_with_adapter() {
        let cp = cp_with_model();
        let router = Router::new(RoutingStrategy::LeastLoaded);
        // Two replicas: only r2 has the math-adapter resident.
        router.register_replica(replica("r1", 0, 10.0, &[]));
        router.register_replica(replica("r2", 0, 10.0, &["math-adapter"]));

        // Request with the math routing key → must land on r2.
        let d = router
            .route(
                &cp,
                &RouteRequest {
                    model: "llama-7b".into(),
                    version: None,
                    routing_key: Some("domain:math".into()),
                },
            )
            .unwrap();
        assert_eq!(d.replica, "r2");
        assert_eq!(d.resolved.adapter.as_deref(), Some("math-adapter"));

        // Request with a routing key whose adapter nobody has → no capacity.
        assert!(router
            .route(
                &cp,
                &RouteRequest {
                    model: "llama-7b".into(),
                    version: None,
                    routing_key: Some("domain:code".into()),
                }
            )
            .is_none());
    }

    #[test]
    fn no_capacity_returns_none() {
        let cp = cp_with_model();
        let router = Router::new(RoutingStrategy::LeastLoaded);
        // Replica exists but is overloaded.
        router.register_replica(replica("r1", 4, 99.0, &[]));
        assert!(router
            .route(
                &cp,
                &RouteRequest {
                    model: "llama-7b".into(),
                    version: None,
                    routing_key: None,
                }
            )
            .is_none());
    }

    #[test]
    fn unknown_model_returns_none() {
        let cp = cp_with_model();
        let router = Router::new(RoutingStrategy::LeastLoaded);
        assert!(router
            .route(
                &cp,
                &RouteRequest {
                    model: "nope".into(),
                    version: None,
                    routing_key: None,
                }
            )
            .is_none());
    }

    #[test]
    fn round_robin_cycles_through_candidates() {
        let cp = cp_with_model();
        let router = Router::new(RoutingStrategy::RoundRobin);
        router.register_replica(replica("a", 0, 10.0, &[]));
        router.register_replica(replica("b", 0, 10.0, &[]));
        router.register_replica(replica("c", 0, 10.0, &[]));

        let picks: Vec<String> = (0..6)
            .map(|_| {
                router
                    .route(
                        &cp,
                        &RouteRequest {
                            model: "llama-7b".into(),
                            version: None,
                            routing_key: None,
                        },
                    )
                    .unwrap()
                    .replica
            })
            .collect();
        // Round-robin over 3 replicas should cycle a, b, c, a, b, c.
        assert_eq!(picks, vec!["a", "b", "c", "a", "b", "c"]);
    }

    #[test]
    fn unhealthy_replica_excluded() {
        let cp = cp_with_model();
        let router = Router::new(RoutingStrategy::LeastLoaded);
        router.register_replica(replica("down", 0, 0.0, &[]));
        // Mark r1 unhealthy via update_load.
        router.update_load("down", 0, 0.0, false);
        assert!(router
            .route(
                &cp,
                &RouteRequest {
                    model: "llama-7b".into(),
                    version: None,
                    routing_key: None,
                }
            )
            .is_none());
    }
}
