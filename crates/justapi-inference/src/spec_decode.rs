//! Speculative decoding (draft-target, Chen et al. 2023).
//!
//! Standard draft-target scheme: a *draft* model proposes a block of `gamma`
//! tokens, the *target* model scores them in a single forward pass per
//! position, and we accept the longest prefix the target agrees with, then
//! sample one correction token from the target. When the draft is cheap and
//! well-aligned with the target, this yields up to `gamma + 1` tokens per
//! decode step instead of one — exactly the throughput win vLLM/SGLang get from
//! speculative decoding, with no Python GIL on the hot path.
//!
//! Medusa / EAGLE are tree-based refinements of the same verify step; the hook
//! here (`gamma` + draft model) is the foundation they build on.

use std::sync::Arc;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::model::{FinishReason, GeneratedToken, Model, ModelError, SamplingParams};

/// Statistics collected during a speculative decode run — the acceptance rate
/// is the dominant driver of the realized speedup (tokens emitted per step).
#[derive(Debug, Clone, Copy, Default)]
pub struct AcceptanceStats {
    /// Number of verify steps executed.
    pub steps: usize,
    /// Total number of draft tokens proposed (draft-target: `gamma` per step;
    /// tree-based: `gamma` per step — i.e. the number of *positions*
    /// speculated, not the number of tree nodes).
    pub total_draft: usize,
    /// Number of proposed draft tokens the target accepted (excludes the
    /// bonus "gamma + 1" target token emitted after a fully-accepted block).
    pub total_accepted: usize,
    /// Number of bonus target tokens emitted (the "gamma + 1" correction after
    /// a fully-accepted draft block). This is the source of the speedup.
    pub extra_emitted: usize,
    /// Branch factor for a tree-based speculation run. `0` means draft-target
    /// (single-path), `>0` means tree-based (Medusa/EAGLE-style).
    pub tree_branch: usize,
    /// Total tree nodes scored by the target (only meaningful when
    /// `tree_branch > 0`). Measures the verification cost per step.
    pub tree_nodes_verified: usize,
}

impl AcceptanceStats {
    /// Create stats for a tree-based speculation run with the given branch factor.
    pub fn with_tree_branch(branch: usize) -> Self {
        Self { tree_branch: branch, ..Default::default() }
    }

    /// Total tokens emitted (`accepted_draft + bonus`).
    pub fn tokens_emitted(&self) -> usize {
        self.total_accepted + self.extra_emitted
    }

    /// Draft acceptance rate in `[0, 1]`. `1.0` means every proposed token was
    /// accepted (the draft is a perfect predictor at this temperature).
    pub fn acceptance_rate(&self) -> f64 {
        if self.total_draft == 0 {
            0.0
        } else {
            self.total_accepted as f64 / self.total_draft as f64
        }
    }
}

/// Configuration for a speculative-decode run.
#[derive(Clone)]
pub struct SpeculativeConfig {
    /// Number of tokens the draft model proposes per verify step (`gamma`).
    /// `0` degenerates to plain target-only decoding (1 token / step).
    pub gamma: usize,
    /// The cheaper model that proposes candidate tokens.
    pub draft: Arc<dyn Model>,
    /// RNG seed for reproducible sampling during verification.
    pub seed: u64,
}

impl SpeculativeConfig {
    pub fn new(gamma: usize, draft: Arc<dyn Model>, seed: u64) -> Self {
        Self { gamma, draft, seed }
    }
}

/// Sample a single token from `logits` honoring `temperature`/`top_p`/`top_k`.
///
/// Pure-Rust (no Candle) so it works for any `Model`, including [`MockModel`].
/// With `temperature == 0` it returns the argmax (greedy), which makes
/// correctness comparisons against plain target decode deterministic.
pub fn sample_token(logits: &[f32], params: &SamplingParams, rng: &mut StdRng) -> u32 {
    let vocab = logits.len();
    if vocab == 0 {
        return 0;
    }
    if params.temperature < 1e-7 {
        return logits
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i as u32)
            .unwrap_or(0);
    }

    // Temperature scaling.
    let inv = 1.0 / params.temperature.max(1e-7);
    let mut scaled: Vec<f32> = logits.iter().map(|l| l * inv).collect();

    // Top-k truncation.
    if params.top_k > 0 && params.top_k < vocab {
        let mut idx: Vec<usize> = (0..vocab).collect();
        idx.sort_by(|&a, &b| {
            scaled[b].partial_cmp(&scaled[a]).unwrap_or(std::cmp::Ordering::Equal)
        });
        let kth = scaled[idx[params.top_k.min(vocab - 1)]];
        for s in scaled.iter_mut() {
            if *s < kth - 1e-6 {
                *s = f32::NEG_INFINITY;
            }
        }
    }

    // Softmax.
    let max = scaled.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut probs: Vec<f32> = scaled.iter().map(|s| (s - max).exp()).collect();
    let sum: f32 = probs.iter().sum();

    // Top-p (nucleus) truncation.
    if params.top_p < 1.0 && sum > 0.0 {
        let mut order: Vec<usize> = (0..vocab).collect();
        order
            .sort_by(|&a, &b| probs[b].partial_cmp(&probs[a]).unwrap_or(std::cmp::Ordering::Equal));
        let mut cum = 0.0;
        let mut cutoff = 1.0;
        for &i in &order {
            cum += probs[i] / sum;
            if cum >= params.top_p {
                cutoff = probs[i] / sum;
                break;
            }
        }
        for p in probs.iter_mut() {
            if *p / sum < cutoff - 1e-9 {
                *p = 0.0;
            }
        }
    }

    let sum2: f32 = probs.iter().sum();
    if sum2 <= 0.0 {
        return (vocab - 1) as u32;
    }

    // Categorical sample.
    let r: f32 = rng.gen();
    let mut cdf = 0.0;
    for (i, p) in probs.iter().enumerate() {
        cdf += p / sum2;
        if r < cdf {
            return i as u32;
        }
    }
    (vocab - 1) as u32
}

/// Run speculative decoding and stream accepted tokens through `sink`.
///
/// Returns the finish reason and per-run [`AcceptanceStats`]. The implementation
/// matches the standard algorithm: draft proposes `gamma` tokens, target scores
/// and accepts the longest agreeing prefix, then emits one correction token.
/// With `gamma == 0` this is identical to plain target decode.
pub fn speculative_generate(
    target: &dyn Model,
    draft: &dyn Model,
    prompt: &[u32],
    params: &SamplingParams,
    gamma: usize,
    seed: u64,
    sink: &dyn Fn(GeneratedToken) -> bool,
) -> Result<(FinishReason, AcceptanceStats), ModelError> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut context: Vec<u32> = prompt.to_vec();
    let mut emitted = 0usize;
    let mut stats = AcceptanceStats::default();
    let mut finish = FinishReason::Length;

    let detok = |id: u32| -> String { MockModel::detokenize(&[id]) };

    'outer: while emitted < params.max_tokens {
        stats.steps += 1;

        // --- Draft phase: propose up to `gamma` tokens. ---
        let mut draft_tokens: Vec<u32> = Vec::with_capacity(gamma);
        let mut dctx = context.clone();
        for _ in 0..gamma {
            if emitted + draft_tokens.len() >= params.max_tokens {
                break;
            }
            let logits = draft.forward_logits(&dctx)?;
            let tok = sample_token(&logits, params, &mut rng);
            if params.stop_tokens.contains(&tok) {
                break;
            }
            draft_tokens.push(tok);
            dctx.push(tok);
        }
        stats.total_draft += draft_tokens.len();

        // --- Verify phase: target scores the draft block. ---
        let mut tctx = context.clone();
        let mut rejected = false;
        for &dtok in &draft_tokens {
            let tlogits = target.forward_logits(&tctx)?;
            let ttok = sample_token(&tlogits, params, &mut rng);
            if ttok == dtok {
                stats.total_accepted += 1;
                let token = GeneratedToken {
                    id: ttok,
                    text: detok(ttok),
                    logprob: 0.0,
                    finish_reason: None,
                };
                if !sink(token) {
                    finish = FinishReason::Cancelled;
                    break 'outer;
                }
                emitted += 1;
                tctx.push(ttok);
            } else {
                // Reject: emit the target's token as the correction, stop block.
                let token = GeneratedToken {
                    id: ttok,
                    text: detok(ttok),
                    logprob: 0.0,
                    finish_reason: None,
                };
                if !sink(token) {
                    finish = FinishReason::Cancelled;
                    break 'outer;
                }
                emitted += 1;
                tctx.push(ttok);
                rejected = true;
                break;
            }
        }

        context = tctx;
        if rejected {
            continue 'outer;
        }

        // --- All drafts accepted: target emits one more token (gamma + 1). ---
        if emitted < params.max_tokens {
            let tlogits = target.forward_logits(&context)?;
            let ttok = sample_token(&tlogits, params, &mut rng);
            let stop = params.stop_tokens.contains(&ttok);
            let token = GeneratedToken {
                id: ttok,
                text: detok(ttok),
                logprob: 0.0,
                finish_reason: if stop { Some(FinishReason::Stop) } else { None },
            };
            if !sink(token) {
                finish = FinishReason::Cancelled;
                break;
            }
            emitted += 1;
            stats.extra_emitted += 1;
            context.push(ttok);
            if stop {
                finish = FinishReason::Stop;
                break;
            }
        }
    }

    Ok((finish, stats))
}

/// A `Model` that runs [`speculative_generate`] under the hood, so it can be
/// registered in the [`crate::ModelRegistry`] and served through the normal
/// `Engine::generate` path without callers knowing speculation is in play.
pub struct SpeculativeModel {
    target: Arc<dyn Model>,
    config: SpeculativeConfig,
}

impl SpeculativeModel {
    pub fn new(target: Arc<dyn Model>, config: SpeculativeConfig) -> Self {
        Self { target, config }
    }
}

impl Model for SpeculativeModel {
    fn vocab_size(&self) -> usize {
        self.target.vocab_size()
    }

    fn generate(
        &self,
        prompt: &[u32],
        params: &SamplingParams,
        sink: &dyn Fn(GeneratedToken) -> bool,
    ) -> Result<FinishReason, ModelError> {
        let (finish, _stats) = speculative_generate(
            &*self.target,
            &*self.config.draft,
            prompt,
            params,
            self.config.gamma,
            self.config.seed,
            sink,
        )?;
        Ok(finish)
    }

    fn forward_logits(&self, context: &[u32]) -> Result<Vec<f32>, ModelError> {
        self.target.forward_logits(context)
    }
}

use crate::model::MockModel;

#[cfg(test)]
mod tests {
    use super::*;

    /// A stub model whose next-token rule is `(prev + offset) % vocab`, used to
    /// simulate a *different* (imperfect) draft model for acceptance tests.
    struct OffsetModel {
        vocab: usize,
        offset: u32,
    }

    impl Model for OffsetModel {
        fn vocab_size(&self) -> usize {
            self.vocab
        }

        fn forward_logits(&self, context: &[u32]) -> Result<Vec<f32>, ModelError> {
            let prev = context.last().copied().unwrap_or(0) % self.vocab as u32;
            let next = (prev.wrapping_add(self.offset)) % self.vocab as u32;
            let mut logits = vec![-5.0f32; self.vocab];
            logits[next as usize] = 5.0;
            Ok(logits)
        }

        fn generate(
            &self,
            prompt: &[u32],
            params: &SamplingParams,
            sink: &dyn Fn(GeneratedToken) -> bool,
        ) -> Result<FinishReason, ModelError> {
            let mut prev = prompt.last().copied().unwrap_or(0);
            for _ in 0..params.max_tokens {
                let next = (prev.wrapping_add(self.offset)) % self.vocab as u32;
                let stop = params.stop_tokens.contains(&next);
                if !sink(GeneratedToken {
                    id: next,
                    text: MockModel::detokenize(&[next]),
                    logprob: 0.0,
                    finish_reason: if stop { Some(FinishReason::Stop) } else { None },
                }) {
                    return Ok(FinishReason::Cancelled);
                }
                if stop {
                    return Ok(FinishReason::Stop);
                }
                prev = next;
            }
            Ok(FinishReason::Length)
        }
    }

    fn collect(model: &dyn Model, prompt: &[u32], params: &SamplingParams) -> Vec<u32> {
        let out = std::cell::RefCell::new(Vec::new());
        model
            .generate(prompt, params, &|t| {
                out.borrow_mut().push(t.id);
                true
            })
            .unwrap();
        out.into_inner()
    }

    fn collect_spec(
        target: &dyn Model,
        draft: &dyn Model,
        prompt: &[u32],
        params: &SamplingParams,
        gamma: usize,
    ) -> (Vec<u32>, AcceptanceStats) {
        let out = std::cell::RefCell::new(Vec::new());
        let (_finish, stats) =
            speculative_generate(target, draft, prompt, params, gamma, 42, &|t| {
                out.borrow_mut().push(t.id);
                true
            })
            .unwrap();
        (out.into_inner(), stats)
    }

    #[test]
    fn target_only_matches_spec_at_gamma_zero() {
        // gamma 0 must reproduce the target's normal decode exactly.
        let target = MockModel::new(32);
        let draft = MockModel::new(32);
        let params = SamplingParams { max_tokens: 12, temperature: 0.0, ..Default::default() };
        let prompt = vec![0u32];
        let baseline = collect(&target, &prompt, &params);
        let (spec, _) = collect_spec(&target, &draft, &prompt, &params, 0);
        assert_eq!(baseline, spec);
        assert_eq!(spec.len(), 12);
    }

    #[test]
    fn spec_matches_target_when_draft_equals_target() {
        // A perfect draft (=== target) changes no output: speculation is
        // correctness-preserving, only faster.
        let target = MockModel::new(32);
        let draft = MockModel::new(32);
        let params = SamplingParams { max_tokens: 20, temperature: 0.0, ..Default::default() };
        let prompt = vec![5u32];
        let baseline = collect(&target, &prompt, &params);
        let (spec, _) = collect_spec(&target, &draft, &prompt, &params, 4);
        assert_eq!(baseline, spec, "speculation must not change the output");
    }

    #[test]
    fn perfect_draft_yields_gamma_plus_1_per_step() {
        let target = MockModel::new(32);
        let draft = MockModel::new(32);
        let max = 20usize;
        let gamma = 4usize;
        let params = SamplingParams { max_tokens: max, temperature: 0.0, ..Default::default() };
        let (spec, stats) = collect_spec(&target, &draft, &[0u32], &params, gamma);
        assert_eq!(spec.len(), max);
        // With a perfect draft, every step emits gamma+1 tokens, so we need
        // ceil(max / (gamma+1)) steps.
        let expected_steps = max.div_ceil(gamma + 1);
        assert_eq!(stats.steps, expected_steps);
        // All draft tokens accepted (acceptance rate ~ 1.0).
        assert!((stats.acceptance_rate() - 1.0).abs() < 1e-9);
        // Bonus tokens == steps (one gamma+1 token per step).
        assert_eq!(stats.extra_emitted, expected_steps);
        assert_eq!(stats.tokens_emitted(), max);
    }

    #[test]
    fn imperfect_draft_lowers_acceptance_rate() {
        let target = MockModel::new(64);
        // Draft predicts (prev + 7) while target predicts (prev + 1): rarely equal.
        let draft = OffsetModel { vocab: 64, offset: 7 };
        let params = SamplingParams { max_tokens: 40, temperature: 0.0, ..Default::default() };
        let (_spec, stats) = collect_spec(&target, &draft, &[0u32], &params, 5);
        // With temperature 0 and a mismatched draft, acceptance is essentially 0.
        assert!(stats.acceptance_rate() < 0.2, "got {}", stats.acceptance_rate());
    }

    #[test]
    fn spec_honors_max_tokens_with_imperfect_draft() {
        let target = MockModel::new(64);
        let draft = OffsetModel { vocab: 64, offset: 3 };
        let max = 15usize;
        let params = SamplingParams { max_tokens: max, temperature: 0.0, ..Default::default() };
        let (spec, _) = collect_spec(&target, &draft, &[0u32], &params, 3);
        assert_eq!(spec.len(), max);
    }

    #[test]
    fn spec_respects_stop_token() {
        let target = MockModel::new(32);
        let draft = MockModel::new(32);
        // MockModel rule is (prev+1)%32; pick `stop` at the next token id.
        let next_after_prompt = (0u32.wrapping_add(1)) % 32;
        let params = SamplingParams {
            max_tokens: 50,
            temperature: 0.0,
            stop_tokens: vec![next_after_prompt],
            ..Default::default()
        };
        let (spec, _) = collect_spec(&target, &draft, &[0u32], &params, 3);
        assert_eq!(spec.len(), 1);
        assert_eq!(spec[0], next_after_prompt);
    }

    #[test]
    fn sampling_token_argmax_is_greedy() {
        let logits = vec![-1.0, 3.0, -2.0, 0.5];
        let params = SamplingParams { temperature: 0.0, ..Default::default() };
        let mut rng = StdRng::seed_from_u64(1);
        assert_eq!(sample_token(&logits, &params, &mut rng), 1);
    }
}
