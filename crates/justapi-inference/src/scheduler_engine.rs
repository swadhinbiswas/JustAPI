//! Bridges the [`Scheduler`] (continuous batching + prefix cache) with the
//! [`Engine`] (model registry) for scheduler-backed token generation.
//!
//! Unlike the naive `Engine::generate()` loop (which runs one request to
//! completion inside [`Model::generate`]), the [`SchedulerEngine`] runs a
//! *single* persistent scheduler thread that interleaves all in-flight
//! requests.  This is true continuous batching: concurrent requests share the
//! same `schedule()` → `forward_logits()` → `on_step_complete()` loop, exactly
//! like vLLM's `SchedulerLoop`.
//!
//! # Architecture
//!
//! [`SchedulerEngine::new`] spawns the scheduler loop thread.  Each call to
//! [`generate`](SchedulerEngine::generate):
//!
//! 1. Resolves the model handle and assigns the request a scheduler seq id
//!    via [`Scheduler::add_request`].
//! 2. Registers a per-request `mpsc` channel keyed by seq id.
//! 3. Returns the receiving end; the loop streams tokens into it.
//!
//! The loop thread:
//! - Calls [`Scheduler::schedule`] to get prefill/decode work.
//! - Feeds each step's context through [`Model::forward_logits`] and samples
//!   one token (greedy argmax).
//! - Routes tokens to the right per-request channel.
//! - Calls [`Scheduler::on_step_complete`] and, when a sequence leaves the
//!   running set, sends a final `finish_reason` token and closes its channel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use crate::engine::Engine;
use crate::model::{FinishReason, GeneratedToken, Model, ModelError, SamplingParams};
use crate::scheduler::{NewRequest, Scheduler, SchedulerStats};
use crate::MockModel;

/// Wraps an [`Engine`] and a [`Scheduler`] to produce a scheduler-backed
/// token stream with continuous batching across concurrent requests.
pub struct SchedulerEngine {
    engine: Arc<Engine>,
    scheduler: Arc<Mutex<Scheduler>>,
    /// Per-seq-id token sinks (one per in-flight request).
    senders: Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<GeneratedToken>>>>,
    /// Per-seq-id model handle (a request may target any registered model).
    seq_model: Arc<Mutex<HashMap<u64, Arc<dyn Model>>>>,
    /// Per-seq-id token context for forward passes.
    seq_context: Arc<Mutex<HashMap<u64, Vec<u32>>>>,
    /// Per-seq-id `max_tokens` so we can emit a finish reason.
    seq_max_tokens: Arc<Mutex<HashMap<u64, usize>>>,
    /// Per-seq-id sampling params (temperature / top_p / top_k) for sampling.
    seq_params: Arc<Mutex<HashMap<u64, SamplingParams>>>,
    /// Liveness flag for the scheduler loop thread.
    alive: Arc<AtomicBool>,
}

impl SchedulerEngine {
    /// Create a new scheduler-backed engine and start its scheduler loop.
    pub fn new(engine: Arc<Engine>, scheduler: Arc<Mutex<Scheduler>>) -> Self {
        let senders = Arc::new(Mutex::new(HashMap::new()));
        let seq_model = Arc::new(Mutex::new(HashMap::new()));
        let seq_context = Arc::new(Mutex::new(HashMap::new()));
        let seq_max_tokens = Arc::new(Mutex::new(HashMap::new()));
        let seq_params = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));

        let se = Self {
            engine,
            scheduler,
            senders,
            seq_model,
            seq_context,
            seq_max_tokens,
            seq_params,
            alive,
        };
        se.spawn_loop();
        se
    }

    /// Borrow the engine.
    pub fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    /// Borrow the scheduler.
    pub fn scheduler(&self) -> &Arc<Mutex<Scheduler>> {
        &self.scheduler
    }

    /// Snapshot of scheduler statistics (for metrics / observability).
    pub fn stats(&self) -> SchedulerStats {
        self.scheduler.lock().unwrap().stats()
    }

    /// Generate tokens for a prompt, streaming them over an unbounded channel.
    pub fn generate(
        &self,
        model_name: &str,
        prompt: &[u32],
        params: SamplingParams,
    ) -> Result<mpsc::UnboundedReceiver<GeneratedToken>, ModelError> {
        let model = self
            .engine
            .get(model_name)
            .ok_or_else(|| ModelError::NotFound(model_name.to_string()))?;

        let (tx, rx) = mpsc::unbounded_channel();

        // Assign a scheduler seq id up front so we can route its tokens.
        let seq_id = {
            let mut sched = self.scheduler.lock().unwrap();
            sched.add_request(NewRequest {
                id: 0,
                prompt: prompt.to_vec(),
                sampling_params: params.clone(),
                prefix_cached_tokens: 0,
                cached_blocks: Vec::new(),
            })
        };

        self.senders.lock().unwrap().insert(seq_id, tx);
        self.seq_model.lock().unwrap().insert(seq_id, model.clone());
        self.seq_max_tokens.lock().unwrap().insert(seq_id, params.max_tokens);
        self.seq_params.lock().unwrap().insert(seq_id, params.clone());

        Ok(rx)
    }

    /// Start the persistent scheduler loop thread.
    fn spawn_loop(&self) {
        let scheduler = self.scheduler.clone();
        let senders = self.senders.clone();
        let seq_model = self.seq_model.clone();
        let seq_context = self.seq_context.clone();
        let seq_max_tokens = self.seq_max_tokens.clone();
        let seq_params = self.seq_params.clone();
        let alive = self.alive.clone();

        std::thread::spawn(move || {
            while alive.load(Ordering::Relaxed) {
                let before: Vec<u64> = {
                    let sched = scheduler.lock().unwrap();
                    sched.running_ids()
                };

                let schedule = {
                    let mut sched = scheduler.lock().unwrap();
                    sched.schedule()
                };

                let empty = schedule.prefill.is_empty() && schedule.decode.is_empty();

                if !empty {
                    // --- Prefill steps ---
                    for step in &schedule.prefill {
                        let params = get_seq_params(&seq_params, step.seq_id);
                        let token = match forward_for_seq(
                            step.seq_id,
                            &step.tokens,
                            &seq_model,
                            &seq_context,
                            &params,
                        ) {
                            Some(t) => t,
                            None => continue,
                        };
                        let text = MockModel::detokenize(&[token]);

                        let done = is_seq_done(step.seq_id, &scheduler, &seq_max_tokens);
                        deliver(
                            &senders,
                            step.seq_id,
                            GeneratedToken {
                                id: token,
                                text,
                                logprob: 0.0,
                                finish_reason: if done { Some(FinishReason::Length) } else { None },
                            },
                        );
                    }

                    // --- Decode steps ---
                    for &seq_id in &schedule.decode {
                        let params = get_seq_params(&seq_params, seq_id);
                        let token = match forward_decode(seq_id, &seq_model, &seq_context, &params)
                        {
                            Some(t) => t,
                            None => continue,
                        };
                        let text = MockModel::detokenize(&[token]);

                        let done = is_seq_done(seq_id, &scheduler, &seq_max_tokens);
                        deliver(
                            &senders,
                            seq_id,
                            GeneratedToken {
                                id: token,
                                text,
                                logprob: 0.0,
                                finish_reason: if done { Some(FinishReason::Length) } else { None },
                            },
                        );
                    }

                    // --- Notify scheduler of completed steps ---
                    {
                        let mut sched = scheduler.lock().unwrap();
                        for step in &schedule.prefill {
                            sched.on_step_complete(step.seq_id, 1);
                        }
                        for &seq_id in &schedule.decode {
                            sched.on_step_complete(seq_id, 1);
                        }
                    }
                }

                // --- Detect completions (seq left the running set) ---
                // Runs on EVERY iteration, including when `schedule()` returned
                // no work (that is exactly when a finished sequence was removed
                // and must receive its final finish token).
                let after: Vec<u64> = {
                    let sched = scheduler.lock().unwrap();
                    sched.running_ids()
                };
                for id in &before {
                    if !after.contains(id) {
                        deliver(
                            &senders,
                            *id,
                            GeneratedToken {
                                id: 0,
                                text: String::new(),
                                logprob: 0.0,
                                finish_reason: Some(FinishReason::Length),
                            },
                        );
                        senders.lock().unwrap().remove(id);
                        seq_model.lock().unwrap().remove(id);
                        seq_context.lock().unwrap().remove(id);
                        seq_max_tokens.lock().unwrap().remove(id);
                        seq_params.lock().unwrap().remove(id);
                    }
                }

                if empty {
                    // Nothing to do this cycle; idle briefly to avoid a hot spin.
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        });
    }
}

/// Fetch a sequence's sampling params (defaults to greedy if unknown).
fn get_seq_params(
    seq_params: &Arc<Mutex<HashMap<u64, SamplingParams>>>,
    seq_id: u64,
) -> SamplingParams {
    seq_params.lock().unwrap().get(&seq_id).cloned().unwrap_or_default()
}

/// Sample a token id from logits according to `params`.
///
/// - `temperature == 0` (or effectively 0): greedy argmax.
/// - Otherwise: scale logits by `1/temperature`, optionally keep only the top-`k`
///   logits, apply top-`p` (nucleus) truncation, then draw from the categorical
///   distribution.  `rand` provides the stochastic draw.
fn sample_token_with_params(logits: &[f32], params: &SamplingParams) -> u32 {
    if params.temperature <= f32::EPSILON {
        return argmax(logits);
    }

    let inv_temp = 1.0 / params.temperature;
    let mut scaled: Vec<f32> = logits.iter().map(|&l| l * inv_temp).collect();

    // Top-k: zero out everything outside the k largest logits.
    if params.top_k > 0 && params.top_k < scaled.len() {
        let mut idx: Vec<usize> = (0..scaled.len()).collect();
        let k = params.top_k;
        idx.select_nth_unstable_by(k - 1, |&a, &b| {
            scaled[b].partial_cmp(&scaled[a]).unwrap_or(std::cmp::Ordering::Equal)
        });
        for &i in &idx[k..] {
            scaled[i] = f32::NEG_INFINITY;
        }
    }

    // Numerically stable softmax.
    let max = scaled.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut probs: Vec<f32> = scaled.iter().map(|&s| (s - max).exp()).collect();
    let mut sum = probs.iter().sum::<f32>();
    if sum <= 0.0 {
        return argmax(logits);
    }
    for p in probs.iter_mut() {
        *p /= sum;
    }

    // Top-p (nucleus): keep the smallest prefix of the sorted distribution that
    // reaches cumulative mass `top_p`, renormalise.
    if params.top_p < 1.0 {
        let mut order: Vec<usize> = (0..probs.len()).collect();
        order
            .sort_by(|&a, &b| probs[b].partial_cmp(&probs[a]).unwrap_or(std::cmp::Ordering::Equal));
        let mut cum = 0.0_f32;
        for (rank, &i) in order.iter().enumerate() {
            cum += probs[i];
            if cum >= params.top_p {
                // Zero everything beyond this rank.
                for &j in &order[rank + 1..] {
                    probs[j] = 0.0;
                }
                break;
            }
        }
        sum = probs.iter().sum::<f32>();
        if sum > 0.0 {
            for p in probs.iter_mut() {
                *p /= sum;
            }
        }
    }

    // Draw from the categorical distribution.
    let mut rng = rand::thread_rng();
    let r = rand::Rng::gen::<f32>(&mut rng);
    let mut cum = 0.0_f32;
    for (i, &p) in probs.iter().enumerate() {
        cum += p;
        if r < cum {
            return i as u32;
        }
    }
    argmax(logits)
}

/// Greedy argmax: index of the maximum logit value.
fn argmax(logits: &[f32]) -> u32 {
    let mut best = 0u32;
    let mut best_val = f32::NEG_INFINITY;
    for (i, &v) in logits.iter().enumerate() {
        if v > best_val {
            best_val = v;
            best = i as u32;
        }
    }
    best
}

/// Run a prefill forward pass for `seq_id`, extending its context with the new
/// tokens.  Returns the sampled token, or `None` if the sequence/model is gone.
fn forward_for_seq(
    seq_id: u64,
    new_tokens: &[u32],
    seq_model: &Arc<Mutex<HashMap<u64, Arc<dyn Model>>>>,
    seq_context: &Arc<Mutex<HashMap<u64, Vec<u32>>>>,
    params: &SamplingParams,
) -> Option<u32> {
    let model = seq_model.lock().unwrap().get(&seq_id)?.clone();
    {
        let mut ctx = seq_context.lock().unwrap();
        ctx.entry(seq_id).or_default().extend_from_slice(new_tokens);
    }
    let ctx = seq_context.lock().unwrap();
    let full = ctx.get(&seq_id)?.clone();
    drop(ctx);
    let logits = model.forward_logits(&full).ok()?;
    Some(sample_token_with_params(&logits, params))
}

/// Run a decode forward pass for `seq_id` (context already includes prior
/// tokens), append the sampled token, and return it.
fn forward_decode(
    seq_id: u64,
    seq_model: &Arc<Mutex<HashMap<u64, Arc<dyn Model>>>>,
    seq_context: &Arc<Mutex<HashMap<u64, Vec<u32>>>>,
    params: &SamplingParams,
) -> Option<u32> {
    let model = seq_model.lock().unwrap().get(&seq_id)?.clone();
    let prev_ctx = seq_context.lock().unwrap().get(&seq_id)?.clone();
    let logits = model.forward_logits(&prev_ctx).ok()?;
    let token = sample_token_with_params(&logits, params);
    if let Some(c) = seq_context.lock().unwrap().get_mut(&seq_id) {
        c.push(token);
    }
    Some(token)
}

/// Send a token to a sequence's channel if it is still open.
fn deliver(
    senders: &Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<GeneratedToken>>>>,
    seq_id: u64,
    token: GeneratedToken,
) {
    if let Some(tx) = senders.lock().unwrap().get(&seq_id) {
        let _ = tx.send(token);
    }
}

/// Whether `seq_id` has reached its `max_tokens` budget.
fn is_seq_done(
    seq_id: u64,
    scheduler: &Arc<Mutex<Scheduler>>,
    seq_max_tokens: &Arc<Mutex<HashMap<u64, usize>>>,
) -> bool {
    let max = seq_max_tokens.lock().unwrap().get(&seq_id).copied().unwrap_or(usize::MAX);
    let generated = scheduler.lock().unwrap().running_seq_generated(seq_id);
    match generated {
        Some(n) => n >= max,
        None => false,
    }
}

impl Drop for SchedulerEngine {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineDevice;
    use crate::kv_cache::KvBlockPool;
    use crate::scheduler::SchedulerConfig;

    fn test_setup() -> (Arc<Engine>, Arc<Mutex<Scheduler>>, Arc<SchedulerEngine>) {
        let engine = Arc::new(Engine::new(EngineDevice::Cpu).unwrap());
        engine.register_mock("mock");

        let pool = KvBlockPool::new(256);
        let config = SchedulerConfig {
            max_num_seqs: 4,
            max_num_batched_tokens: 4096,
            max_seq_len: 4096,
            chunked_prefill_size: None,
        };
        let scheduler = Arc::new(Mutex::new(Scheduler::new(config, pool)));
        let se = Arc::new(SchedulerEngine::new(engine.clone(), scheduler.clone()));

        (engine, scheduler, se)
    }

    async fn drain_receiver(
        rx: &mut mpsc::UnboundedReceiver<GeneratedToken>,
    ) -> Vec<GeneratedToken> {
        let mut tokens = Vec::new();
        while let Some(tok) = rx.recv().await {
            if tok.finish_reason.is_some() {
                break;
            }
            tokens.push(tok);
        }
        tokens
    }

    #[tokio::test]
    async fn scheduler_engine_generates_tokens() {
        let (_, _, se) = test_setup();
        let prompt = MockModel::tokenize("hello");
        let params = SamplingParams { max_tokens: 10, temperature: 0.0, ..Default::default() };

        let mut rx = se.generate("mock", &prompt, params).unwrap();
        let tokens = drain_receiver(&mut rx).await;
        assert!(!tokens.is_empty(), "should generate at least one token");
    }

    #[tokio::test]
    async fn scheduler_engine_streams_multiple_tokens() {
        let (_, _, se) = test_setup();
        let prompt = MockModel::tokenize("hello world");
        let params = SamplingParams { max_tokens: 5, temperature: 0.0, ..Default::default() };

        let mut rx = se.generate("mock", &prompt, params).unwrap();
        let tokens = drain_receiver(&mut rx).await;
        assert!(!tokens.is_empty(), "should produce tokens");
        assert!(tokens.iter().all(|t| !t.text.is_empty()), "every token should have text");
    }

    #[tokio::test]
    async fn scheduler_engine_prefix_cache_hit_reuses_blocks() {
        let (_, sched, se) = test_setup();
        // Need >= 16 tokens for at least one full block (BLOCK_SIZE=16).
        let prompt_text = "abcdefghijklmnopqrstuvwxyz0123456789"; // 36 tokens
        let prompt = MockModel::tokenize(prompt_text);
        assert!(prompt.len() >= 16, "prompt must be >= 16 tokens for block caching");

        let params = SamplingParams { max_tokens: 3, temperature: 0.0, ..Default::default() };

        // First request caches its prompt.
        let mut rx1 = se.generate("mock", &prompt, params.clone()).unwrap();
        drain_receiver(&mut rx1).await;

        let stats_before = se.stats();
        assert!(
            stats_before.prefix.nodes > 0,
            "expected at least one prefix cache node after first request, got {}",
            stats_before.prefix.nodes
        );

        // Second request with same prompt → should hit the prefix cache.
        let mut rx2 = se.generate("mock", &prompt, params).unwrap();
        drain_receiver(&mut rx2).await;

        let stats_after = se.stats();
        assert!(
            stats_after.prefix.hits > stats_before.prefix.hits,
            "expected at least one additional prefix cache hit, got {} -> {}",
            stats_before.prefix.hits,
            stats_after.prefix.hits
        );
        let _ = sched;
    }

    #[tokio::test]
    async fn scheduler_engine_batches_concurrent_requests() {
        let (_, _, se) = test_setup();
        let params = SamplingParams { max_tokens: 4, temperature: 0.0, ..Default::default() };

        // Launch two concurrent requests; both should complete.
        let mut rx_a = se.generate("mock", &MockModel::tokenize("hello"), params.clone()).unwrap();
        let mut rx_b = se.generate("mock", &MockModel::tokenize("world"), params.clone()).unwrap();

        let a = drain_receiver(&mut rx_a).await;
        let b = drain_receiver(&mut rx_b).await;

        assert!(!a.is_empty(), "request A should produce tokens");
        assert!(!b.is_empty(), "request B should produce tokens");
    }

    #[tokio::test]
    async fn scheduler_engine_handles_many_concurrent_requests() {
        let (_, _, se) = test_setup();
        let params = SamplingParams { max_tokens: 2, temperature: 0.0, ..Default::default() };

        // Fire 8 concurrent requests (more than max_num_seqs=4 batch limit).
        let mut receivers = Vec::new();
        for i in 0..8u32 {
            let p = MockModel::tokenize(&format!("request-{i}"));
            receivers.push(se.generate("mock", &p, params.clone()).unwrap());
        }

        let mut completed = 0;
        for rx in receivers.iter_mut() {
            let tokens = drain_receiver(rx).await;
            if !tokens.is_empty() {
                completed += 1;
            }
        }
        assert_eq!(completed, 8, "all 8 concurrent requests should complete");
    }

    #[test]
    fn scheduler_engine_stats_accessible() {
        let (_, _, se) = test_setup();
        let stats = se.stats();
        assert_eq!(stats.num_waiting, 0);
        assert_eq!(stats.num_running, 0);
    }

    #[test]
    fn argmax_selects_max() {
        let logits = vec![-1.0, 0.5, 3.0, -2.0];
        assert_eq!(argmax(&logits), 2);
    }

    #[test]
    fn argmax_handles_negative_all() {
        let logits = vec![-10.0, -5.0, -1.0];
        assert_eq!(argmax(&logits), 2);
    }

    #[test]
    fn greedy_sampling_is_argmax() {
        // temperature 0 ⇒ deterministic argmax regardless of distribution.
        let logits = vec![-1.0, 0.5, 3.0, -2.0];
        let params = SamplingParams { temperature: 0.0, ..Default::default() };
        assert_eq!(sample_token_with_params(&logits, &params), 2);
    }

    #[test]
    fn sampling_honors_temperature() {
        // A sharp peak with very low temperature should always pick the peak.
        let logits = vec![-100.0, -100.0, 50.0, -100.0];
        let params = SamplingParams { temperature: 0.01, ..Default::default() };
        for _ in 0..50 {
            assert_eq!(sample_token_with_params(&logits, &params), 2);
        }
    }

    #[test]
    fn sampling_top_k_truncates() {
        // top_k=1 ⇒ always the argmax even with high temperature noise.
        let logits = vec![1.0, 2.0, 10.0, 0.5];
        let params = SamplingParams { temperature: 2.0, top_k: 1, ..Default::default() };
        for _ in 0..50 {
            assert_eq!(sample_token_with_params(&logits, &params), 2);
        }
    }

    #[test]
    fn sampling_uniform_distributes_with_high_temp() {
        // A flat distribution at high temperature should produce variety, not
        // a single token (stochasticity is exercised).
        let logits = vec![0.0, 0.0, 0.0, 0.0];
        let params = SamplingParams { temperature: 1.0, ..Default::default() };
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            seen.insert(sample_token_with_params(&logits, &params));
        }
        assert!(
            seen.len() >= 2,
            "high-temperature flat logits should sample multiple tokens, saw {}",
            seen.len()
        );
    }
}
