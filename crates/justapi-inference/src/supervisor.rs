//! Multi-replica supervisor — the lifecycle/orchestration layer.
//!
//! Ties the [`Autoscaler`] (how many replicas we want) and the [`Router`]
//! (which replicas can serve a request) to actual replicas. It is pure decision
//! logic: rather than spawning processes, it emits [`SupervisorAction`] intents
//! (start/stop a replica) that an external executor fulfills — that executor is
//! where real process/GPU management lives.
//!
//! Lifecycle:
//! 1. `reconcile(metrics)` asks the autoscaler for a desired replica count and
//!    emits `StartReplica` / `StopReplica` intents to close the gap with the
//!    live set, then syncs the router's replica view.
//! 2. Newly started replicas register in the router as **not-yet-healthy**; the
//!    executor calls `on_replica_started` once the replica is actually serving,
//!    flipping it healthy so the router will route to it.
//! 3. `on_replica_report` feeds live load metrics back in; `on_replica_stopped`
//!    removes a replica the executor terminated.

use std::collections::HashSet;

use crate::engine::EngineDevice;
use crate::{Autoscaler, AutoscalerConfig, LlmMetrics, Replica, Router, RoutingStrategy};

/// The supervisor's view of a single live replica.
#[derive(Debug, Clone)]
pub struct LiveReplica {
    pub id: String,
    /// Whether the replica is healthy and accepting traffic.
    pub healthy: bool,
    /// Current active sequences (drives router load-aware selection).
    pub active_sequences: usize,
    /// KV-cache pressure (0–100).
    pub kv_pressure_pct: f32,
}

/// An intent the supervisor emits for the executor to fulfill.
#[derive(Debug, Clone)]
pub enum SupervisorAction {
    /// Start a new replica for `model` @ `version`.
    StartReplica {
        id: String,
        model: String,
        version: String,
    },
    /// Terminate the replica with `id`.
    StopReplica { id: String },
}

/// Tuning for the supervisor (mirrors the autoscaler knobs plus replica shape).
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// Model this supervisor manages.
    pub model: String,
    /// Version of the model this supervisor manages.
    pub version: String,
    /// Device new replicas are pinned to.
    pub device: EngineDevice,
    pub min_replicas: usize,
    pub max_replicas: usize,
    /// Max concurrent sequences per replica (advertised to the router).
    pub max_concurrency_per_replica: usize,
    pub target_ttft_ms: f32,
    pub target_kv_pressure_pct: f32,
    pub queue_depth_scale_up: usize,
    pub queue_depth_scale_down: usize,
    /// Ticks to wait after a scaling action before another (flap protection).
    pub cooldown_ticks: usize,
}

/// Multi-replica supervisor: owns an [`Autoscaler`] + [`Router`] and drives a
/// live replica set toward the autoscaler's desired count.
pub struct Supervisor {
    config: SupervisorConfig,
    autoscaler: Autoscaler,
    router: Router,
    live: Vec<LiveReplica>,
    next_index: usize,
}

impl Supervisor {
    /// Create a supervisor for `config.model`@`config.version`. The embedded
    /// autoscaler seeds at `min_replicas`.
    pub fn new(config: SupervisorConfig) -> Self {
        let autoscaler = Autoscaler::new(AutoscalerConfig {
            min_replicas: config.min_replicas,
            max_replicas: config.max_replicas,
            target_ttft_ms: config.target_ttft_ms,
            target_kv_pressure_pct: config.target_kv_pressure_pct,
            queue_depth_scale_up: config.queue_depth_scale_up,
            queue_depth_scale_down: config.queue_depth_scale_down,
            cooldown_ticks: config.cooldown_ticks,
        });
        Self {
            config,
            autoscaler,
            router: Router::new(RoutingStrategy::LeastLoaded),
            live: Vec::new(),
            next_index: 0,
        }
    }

    /// Current desired replica count (from the autoscaler).
    pub fn desired(&self) -> usize {
        self.autoscaler.current()
    }

    /// Number of live replicas (healthy or starting).
    pub fn live_count(&self) -> usize {
        self.live.len()
    }

    /// Number of healthy live replicas.
    pub fn healthy_count(&self) -> usize {
        self.live.iter().filter(|r| r.healthy).count()
    }

    /// Access the supervisor's router (so requests can be routed against the
    /// live replica set).
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Reconcile the live replica set with the desired count derived from
    /// `metrics`. Emits start/stop intents and syncs the router.
    pub fn reconcile(&mut self, metrics: &LlmMetrics) -> Vec<SupervisorAction> {
        self.autoscaler.decide(metrics);
        let desired = self.autoscaler.current();
        let mut actions = Vec::new();

        // Scale up: register not-yet-healthy replicas and emit start intents.
        while self.live.len() < desired && self.live.len() < self.config.max_replicas {
            let id = format!(
                "{}-{}-{}",
                self.config.model, self.config.version, self.next_index
            );
            self.next_index += 1;
            self.live.push(LiveReplica {
                id: id.clone(),
                healthy: false,
                active_sequences: 0,
                kv_pressure_pct: 0.0,
            });
            actions.push(SupervisorAction::StartReplica {
                id,
                model: self.config.model.clone(),
                version: self.config.version.clone(),
            });
        }

        // Scale down: prefer removing stuck (unhealthy) replicas first.
        while self.live.len() > desired {
            let idx = self
                .live
                .iter()
                .position(|r| !r.healthy)
                .or_else(|| Some(self.live.len().saturating_sub(1)));
            if let Some(idx) = idx {
                let removed = self.live.remove(idx);
                actions.push(SupervisorAction::StopReplica { id: removed.id });
            } else {
                break;
            }
        }

        self.sync_router();
        actions
    }

    /// Executor reports a replica has finished starting and is now healthy.
    pub fn on_replica_started(&mut self, id: &str) {
        if let Some(r) = self.live.iter_mut().find(|r| r.id == id) {
            r.healthy = true;
        }
        self.sync_router();
    }

    /// Executor reports live load for a replica.
    pub fn on_replica_report(&mut self, id: &str, active_sequences: usize, kv_pressure_pct: f32) {
        if let Some(r) = self.live.iter_mut().find(|r| r.id == id) {
            r.active_sequences = active_sequences;
            r.kv_pressure_pct = kv_pressure_pct;
        }
        self.sync_router();
    }

    /// Executor reports a replica was terminated.
    pub fn on_replica_stopped(&mut self, id: &str) {
        self.live.retain(|r| r.id != id);
        self.router.remove_replica(id);
    }

    /// Push the live replica set into the router (add/update live, drop gone).
    fn sync_router(&mut self) {
        let live_ids: HashSet<String> = self.live.iter().map(|r| r.id.clone()).collect();
        for r in self.router.replicas() {
            if !live_ids.contains(&r.id) {
                self.router.remove_replica(&r.id);
            }
        }
        for lr in &self.live {
            self.router.register_replica(Replica {
                id: lr.id.clone(),
                model: self.config.model.clone(),
                version: self.config.version.clone(),
                device: self.config.device,
                max_concurrency: self.config.max_concurrency_per_replica,
                active_sequences: lr.active_sequences,
                kv_pressure_pct: lr.kv_pressure_pct,
                healthy: lr.healthy,
                loaded_adapters: Vec::new(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SupervisorConfig {
        SupervisorConfig {
            model: "llama-7b".into(),
            version: "v1".into(),
            device: EngineDevice::Cuda(0),
            min_replicas: 1,
            max_replicas: 4,
            max_concurrency_per_replica: 8,
            target_ttft_ms: 500.0,
            target_kv_pressure_pct: 80.0,
            queue_depth_scale_up: 16,
            queue_depth_scale_down: 2,
            cooldown_ticks: 2,
        }
    }

    fn metrics(kv: f32, ttft: f32, q: usize) -> LlmMetrics {
        LlmMetrics {
            tokens_per_sec: 50.0,
            ttft_ms: ttft,
            queue_depth: q,
            kv_pressure_pct: kv,
        }
    }

    #[test]
    fn seeds_min_replicas_on_first_reconcile() {
        let mut s = Supervisor::new(cfg());
        // Healthy metrics → desired stays at min (1). No live yet → start one.
        let actions = s.reconcile(&metrics(10.0, 100.0, 0));
        assert_eq!(s.desired(), 1);
        assert_eq!(s.live_count(), 1);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            SupervisorAction::StartReplica { id, .. } => assert!(id.contains("llama-7b-v1")),
            _ => panic!("expected StartReplica"),
        }
        // Router sees the replica (not yet healthy → not routable).
        assert_eq!(s.router().replicas().len(), 1);
        assert_eq!(s.healthy_count(), 0);
    }

    #[test]
    fn stays_stable_once_replicas_healthy() {
        let mut s = Supervisor::new(cfg());
        let actions = s.reconcile(&metrics(10.0, 100.0, 0));
        let id = match &actions[0] {
            SupervisorAction::StartReplica { id, .. } => id.clone(),
            _ => panic!(),
        };
        s.on_replica_started(&id);
        assert_eq!(s.healthy_count(), 1);
        // Another healthy reconcile → no new actions.
        let actions = s.reconcile(&metrics(10.0, 100.0, 0));
        assert!(actions.is_empty());
        assert_eq!(s.live_count(), 1);
    }

    #[test]
    fn scales_up_under_load() {
        let mut s = Supervisor::new(cfg());
        // First establish the baseline (1 replica, mark healthy).
        let a = s.reconcile(&metrics(10.0, 100.0, 0));
        let id = match &a[0] {
            SupervisorAction::StartReplica { id, .. } => id.clone(),
            _ => panic!(),
        };
        s.on_replica_started(&id);
        // Now saturate: KV pressure high → autoscaler wants more.
        // Cooldown ticks pass between reconciles.
        let _ = s.reconcile(&metrics(90.0, 100.0, 0));
        let _ = s.reconcile(&metrics(90.0, 100.0, 0));
        let actions = s.reconcile(&metrics(90.0, 100.0, 0));
        // Either we already scaled up via the cooldown window or this emits starts.
        assert!(s.desired() >= 2);
        if !actions.is_empty() {
            assert!(actions
                .iter()
                .all(|a| matches!(a, SupervisorAction::StartReplica { .. })));
        }
        assert!(s.live_count() >= 2);
    }

    #[test]
    fn never_exceeds_max() {
        let mut s = Supervisor::new(cfg());
        // Hammer with saturated metrics across many ticks; live must cap at max.
        for _ in 0..12 {
            let _ = s.reconcile(&metrics(99.0, 2000.0, 100));
        }
        assert_eq!(s.live_count(), 4);
        assert_eq!(s.desired(), 4);
    }

    #[test]
    fn scales_down_under_low_load() {
        let mut c = cfg();
        c.max_replicas = 3;
        let mut s = Supervisor::new(c);
        // Bring up to 3 replicas.
        for _ in 0..12 {
            let _ = s.reconcile(&metrics(99.0, 2000.0, 100));
        }
        // Mark them all healthy.
        for r in s.router().replicas() {
            s.on_replica_started(&r.id);
        }
        assert_eq!(s.live_count(), 3);
        assert_eq!(s.healthy_count(), 3);
        // Now underused for many ticks → scale down to min.
        for _ in 0..12 {
            let _ = s.reconcile(&metrics(5.0, 50.0, 0));
        }
        assert_eq!(s.live_count(), 1);
        assert_eq!(s.healthy_count(), 1);
    }

    #[test]
    fn router_sync_reflects_live_set() {
        let mut s = Supervisor::new(cfg());
        let _ = s.reconcile(&metrics(99.0, 2000.0, 100));
        let _ = s.reconcile(&metrics(99.0, 2000.0, 100));
        let _ = s.reconcile(&metrics(99.0, 2000.0, 100));
        // Router replica count must equal the live count (synced each reconcile).
        assert_eq!(s.router().replicas().len(), s.live_count());
        // After stopping one via the executor, the router drops it too.
        let id = s.router().replicas()[0].id.clone();
        s.on_replica_stopped(&id);
        assert_eq!(s.router().replicas().len(), s.live_count());
        assert!(!s.router().replicas().iter().any(|r| r.id == id));
    }
}
