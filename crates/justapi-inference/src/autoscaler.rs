//! LLM-specific autoscaler — scaling decisions from LLM health signals.
//!
//! Unlike a generic HTTP autoscaler (which watches QPS), this watches the
//! signals that actually matter for LLM serving:
//!
//! - **TTFT** (time to first token) — prefill latency / saturation.
//! - **tokens/sec** — decode throughput per replica.
//! - **queue depth** — pending requests waiting for a slot.
//! - **KV-cache pressure** — the binding memory constraint for long contexts.
//!
//! Pure decision logic (no timers, no metrics collection, no orchestration).
//! The [`Autoscaler`] is stateful only for cooldown/flap protection; pair it
//! with the metrics pipeline and the multi-replica supervisor.

/// Live LLM-serving signals fed to the autoscaler each tick.
#[derive(Debug, Clone, Copy)]
pub struct LlmMetrics {
    /// Decode throughput per replica (tokens/sec).
    pub tokens_per_sec: f32,
    /// Time to first token, milliseconds.
    pub ttft_ms: f32,
    /// Number of requests waiting for a free slot.
    pub queue_depth: usize,
    /// KV-cache pressure across replicas (0–100%).
    pub kv_pressure_pct: f32,
}

/// Tuning for the autoscaler.
#[derive(Debug, Clone, Copy)]
pub struct AutoscalerConfig {
    /// Never scale below this many replicas.
    pub min_replicas: usize,
    /// Never scale above this many replicas.
    pub max_replicas: usize,
    /// TTFT above which we consider the fleet saturated.
    pub target_ttft_ms: f32,
    /// KV-cache pressure (0–100) above which we consider the fleet saturated.
    pub target_kv_pressure_pct: f32,
    /// Queue depth at/above which we scale up.
    pub queue_depth_scale_up: usize,
    /// Queue depth at/below which we may scale down.
    pub queue_depth_scale_down: usize,
    /// Ticks to wait after a scaling action before another is allowed
    /// (prevents flapping on noisy metrics).
    pub cooldown_ticks: usize,
}

impl Default for AutoscalerConfig {
    fn default() -> Self {
        Self {
            min_replicas: 1,
            max_replicas: 8,
            target_ttft_ms: 500.0,
            target_kv_pressure_pct: 80.0,
            queue_depth_scale_up: 16,
            queue_depth_scale_down: 2,
            cooldown_ticks: 10,
        }
    }
}

/// What the autoscaler wants the supervisor to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleDecision {
    /// Add replicas up to `desired`.
    ScaleUp(usize),
    /// Remove replicas down to `desired`.
    ScaleDown(usize),
    /// Keep `desired` replicas (no change).
    Hold(usize),
}

/// Stateful LLM autoscaler.
pub struct Autoscaler {
    config: AutoscalerConfig,
    current: usize,
    cooldown: usize,
}

impl Autoscaler {
    /// Create an autoscaler starting at `config.min_replicas`.
    pub fn new(config: AutoscalerConfig) -> Self {
        let current = config.min_replicas.max(1).min(config.max_replicas.max(1));
        Self { config, current, cooldown: 0 }
    }

    /// Current desired replica count.
    pub fn current(&self) -> usize {
        self.current
    }

    /// Whether the fleet is currently in a cooldown window.
    pub fn in_cooldown(&self) -> bool {
        self.cooldown > 0
    }

    /// Decide the next action from the latest metrics.
    ///
    /// - **Scale up** when KV pressure, TTFT, or queue depth breach their
    ///   saturation thresholds.
    /// - **Scale down** when all signals are comfortably under half their
    ///   targets and queue depth is low.
    /// - **Hold** otherwise, or while in cooldown.
    pub fn decide(&mut self, m: &LlmMetrics) -> ScaleDecision {
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return ScaleDecision::Hold(self.current);
        }

        let saturated = m.kv_pressure_pct >= self.config.target_kv_pressure_pct
            || m.ttft_ms >= self.config.target_ttft_ms
            || m.queue_depth >= self.config.queue_depth_scale_up;

        let underused = m.kv_pressure_pct <= self.config.target_kv_pressure_pct * 0.5
            && m.ttft_ms <= self.config.target_ttft_ms * 0.5
            && m.queue_depth <= self.config.queue_depth_scale_down;

        let desired = if saturated {
            (self.current + 1).min(self.config.max_replicas)
        } else if underused {
            self.current.saturating_sub(1).max(self.config.min_replicas)
        } else {
            self.current
        };

        if desired == self.current {
            ScaleDecision::Hold(self.current)
        } else if desired > self.current {
            self.current = desired;
            self.cooldown = self.config.cooldown_ticks;
            ScaleDecision::ScaleUp(desired)
        } else {
            self.current = desired;
            self.cooldown = self.config.cooldown_ticks;
            ScaleDecision::ScaleDown(desired)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AutoscalerConfig {
        AutoscalerConfig {
            min_replicas: 1,
            max_replicas: 4,
            target_ttft_ms: 500.0,
            target_kv_pressure_pct: 80.0,
            queue_depth_scale_up: 16,
            queue_depth_scale_down: 2,
            cooldown_ticks: 2,
        }
    }

    fn metrics(kv: f32, ttft: f32, q: usize) -> LlmMetrics {
        LlmMetrics { tokens_per_sec: 50.0, ttft_ms: ttft, queue_depth: q, kv_pressure_pct: kv }
    }

    #[test]
    fn scales_up_on_kv_pressure() {
        let mut a = Autoscaler::new(cfg());
        let d = a.decide(&metrics(95.0, 100.0, 0));
        assert_eq!(d, ScaleDecision::ScaleUp(2));
        assert!(a.in_cooldown());
    }

    #[test]
    fn scales_up_on_ttft() {
        let mut a = Autoscaler::new(cfg());
        let d = a.decide(&metrics(10.0, 1200.0, 0));
        assert_eq!(d, ScaleDecision::ScaleUp(2));
    }

    #[test]
    fn scales_up_on_queue_depth() {
        let mut a = Autoscaler::new(cfg());
        let d = a.decide(&metrics(10.0, 100.0, 40));
        assert_eq!(d, ScaleDecision::ScaleUp(2));
    }

    #[test]
    fn holds_when_healthy() {
        let mut a = Autoscaler::new(cfg());
        let d = a.decide(&metrics(40.0, 200.0, 1));
        assert_eq!(d, ScaleDecision::Hold(1));
    }

    #[test]
    fn scales_down_when_underused() {
        let mut a = Autoscaler::new(cfg());
        // First bump to 2, then drain metrics to trigger a scale down.
        let _ = a.decide(&metrics(95.0, 100.0, 0));
        // cooldown ticks
        let _ = a.decide(&metrics(10.0, 100.0, 0));
        let _ = a.decide(&metrics(10.0, 100.0, 0));
        let d = a.decide(&metrics(10.0, 100.0, 0));
        // still underused and out of cooldown → ScaleDown back to 1
        assert_eq!(d, ScaleDecision::ScaleDown(1));
    }

    #[test]
    fn never_exceeds_max() {
        let mut a = Autoscaler::new(cfg());
        // Drive up to max with cooldown ticks between.
        for _ in 0..10 {
            let _ = a.decide(&metrics(99.0, 2000.0, 100));
        }
        assert_eq!(a.current(), 4);
        let d = a.decide(&metrics(99.0, 2000.0, 100));
        assert_eq!(d, ScaleDecision::Hold(4));
    }

    #[test]
    fn never_below_min() {
        let mut a = Autoscaler::new(cfg());
        // Underused from the start → stays at min.
        for _ in 0..6 {
            let _ = a.decide(&metrics(5.0, 50.0, 0));
        }
        assert_eq!(a.current(), 1);
        let d = a.decide(&metrics(5.0, 50.0, 0));
        assert_eq!(d, ScaleDecision::Hold(1));
    }

    #[test]
    fn cooldown_prevents_flapping() {
        let mut a = Autoscaler::new(cfg());
        assert_eq!(a.decide(&metrics(95.0, 100.0, 0)), ScaleDecision::ScaleUp(2));
        // Immediately after scaling up we are in cooldown; metrics flipping to
        // healthy must NOT trigger an instant scale-down.
        assert_eq!(a.decide(&metrics(5.0, 50.0, 0)), ScaleDecision::Hold(2));
        assert_eq!(a.decide(&metrics(5.0, 50.0, 0)), ScaleDecision::Hold(2));
        // After cooldown expires, healthy metrics can scale down.
        let d = a.decide(&metrics(5.0, 50.0, 0));
        assert_eq!(d, ScaleDecision::ScaleDown(1));
    }
}
