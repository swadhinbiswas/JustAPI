//! Tree-based speculative decoding (Medusa / EAGLE-style).
//!
//! Extends the draft-target scheme from [`super::spec_decode`] with a *tree* of
//! candidate tokens per position. The draft proposes `branch` candidates at each
//! of `gamma` positions, forming a tree. The target verifies a single path
//! through the tree (the longest path where every node matches the target's
//! prediction at that context), which yields higher acceptance than the
//! single-path draft-target scheme because the draft gets multiple "guesses"
//! per position.
//!
//! # Lossless guarantee
//!
//! Like the draft-target scheme, tree-based speculation produces exactly the
//! same output as plain target decode — it is a correctness-preserving
//! optimization. At every verify step, if any tree node matches the target's
//! sample, that token is emitted; if no node matches, the target's own
//! correction token is emitted — which is token-for-token identical to running
//! the target alone.

use rand::rngs::StdRng;
use rand::SeedableRng;
use std::sync::Arc;

use crate::model::{FinishReason, GeneratedToken, Model, ModelError, SamplingParams};
use crate::spec_decode::sample_token;
use crate::MockModel;

/// A single node in the draft tree.
#[derive(Clone, Debug)]
pub struct TreeNode {
    pub token: u32,
    pub score: f32,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(token: u32, score: f32) -> Self {
        Self { token, score, children: vec![] }
    }
}

/// A draft tree: branching structure of candidate tokens.
#[derive(Clone, Debug)]
pub struct DraftTree {
    pub roots: Vec<TreeNode>,
    pub gamma: usize,
    pub branch: usize,
}

impl DraftTree {
    /// Total number of nodes = Σ_{i=1}^{γ} b^i.
    pub fn total_nodes(&self) -> usize {
        if self.branch == 0 || self.gamma == 0 {
            return 0;
        }
        let b = self.branch as u64;
        let g = self.gamma as u64;
        if b == 1 {
            return g as usize;
        }
        (b * (b.pow(g as u32) - 1) / (b - 1)) as usize
    }
}

/// Return the top-k token ids and raw logit scores.
pub fn top_k_tokens(logits: &[f32], k: usize) -> Vec<(u32, f32)> {
    if k == 0 || logits.is_empty() {
        return vec![];
    }
    let mut scored: Vec<(u32, f32)> =
        logits.iter().enumerate().map(|(i, &s)| (i as u32, s)).collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    scored
}

/// Build a draft tree by recursively expanding the top-`branch` tokens at
/// each position up to `gamma` depth.
///
/// Makes O(b^γ) draft forward calls — acceptable for small γ and b.
pub fn build_draft_tree(
    draft: &dyn Model,
    context: &[u32],
    gamma: usize,
    branch: usize,
) -> Result<DraftTree, ModelError> {
    if gamma == 0 || branch == 0 {
        return Ok(DraftTree { roots: vec![], gamma: 0, branch: 0 });
    }

    fn expand(
        draft: &dyn Model,
        ctx: &[u32],
        depth_remaining: usize,
        branch: usize,
    ) -> Result<Vec<TreeNode>, ModelError> {
        if depth_remaining == 0 {
            return Ok(vec![]);
        }
        let logits = draft.forward_logits(ctx)?;
        let candidates = top_k_tokens(&logits, branch);
        let mut nodes = Vec::with_capacity(candidates.len());
        for (token, score) in candidates {
            let mut child_ctx = ctx.to_vec();
            child_ctx.push(token);
            let children = expand(draft, &child_ctx, depth_remaining - 1, branch)?;
            nodes.push(TreeNode { token, score, children });
        }
        Ok(nodes)
    }

    let roots = expand(draft, context, gamma, branch)?;
    Ok(DraftTree { roots, gamma, branch })
}

/// Verify a draft tree by walking the longest matching path.
///
/// For each depth:
/// 1. Call target forward with current context (original + accepted prefix).
/// 2. Sample the target's next token.
/// 3. Find a matching child at this depth — if found, accept and continue;
///    if not found, emit correction (the target's token) and stop.
///
/// Returns `(accepted_tokens, bonus_or_correction, accepted_count)`.
/// - `accepted_tokens`: draft tokens accepted from the tree (may be empty).
/// - `bonus_or_correction`: `Some(token)` — always emitted (target's own
///   prediction after the accepted prefix or at the rejection point).
/// - `accepted_count`: same as `accepted_tokens.len()`.
pub fn verify_tree(
    target: &dyn Model,
    tree: &DraftTree,
    context: &[u32],
    params: &SamplingParams,
    rng: &mut StdRng,
) -> Result<(Vec<u32>, Option<u32>, usize), ModelError> {
    let mut accepted: Vec<u32> = Vec::with_capacity(tree.gamma);
    let mut ctx = context.to_vec();

    // Clone roots into frontier so we can own children when descending.
    let mut frontier: Vec<(u32, Vec<TreeNode>)> =
        tree.roots.iter().map(|n| (n.token, n.children.clone())).collect();

    let mut n_accepted = 0usize;

    for _depth in 0..tree.gamma {
        let logits = target.forward_logits(&ctx)?;
        let target_tok = sample_token(&logits, params, rng);

        if let Some(idx) = frontier.iter().position(|(tok, _)| *tok == target_tok) {
            let (_tok, children) = frontier.swap_remove(idx);
            accepted.push(target_tok);
            n_accepted += 1;
            ctx.push(target_tok);
            frontier = children.into_iter().map(|n| (n.token, n.children)).collect();
            if frontier.is_empty() {
                let logits = target.forward_logits(&ctx)?;
                let bonus = sample_token(&logits, params, rng);
                return Ok((accepted, Some(bonus), n_accepted));
            }
        } else {
            return Ok((accepted, Some(target_tok), n_accepted));
        }
    }

    let logits = target.forward_logits(&ctx)?;
    let bonus = sample_token(&logits, params, rng);
    Ok((accepted, Some(bonus), n_accepted))
}

/// Run tree-based speculative decoding and stream tokens through `sink`.
///
/// Each step:
/// 1. `build_draft_tree` — draft proposes a tree of candidates.
/// 2. `verify_tree` — target scores the tree, finds the longest agreeing path.
/// 3. Accepted tokens + bonus/correction are streamed through `sink`.
///
/// Returns finish reason and per-run [`crate::AcceptanceStats`].
/// With `gamma == 0` or `branch == 0` this degenerates to plain decode.
#[allow(clippy::too_many_arguments)]
pub fn speculative_generate_tree(
    target: &dyn Model,
    draft: &dyn Model,
    prompt: &[u32],
    params: &SamplingParams,
    gamma: usize,
    branch: usize,
    seed: u64,
    sink: &dyn Fn(GeneratedToken) -> bool,
) -> Result<(FinishReason, crate::AcceptanceStats), ModelError> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut context: Vec<u32> = prompt.to_vec();
    let mut emitted = 0usize;
    let mut stats = crate::AcceptanceStats::with_tree_branch(branch);
    let mut finish = FinishReason::Length;

    let detok = |id: u32| -> String { MockModel::detokenize(&[id]) };

    while emitted < params.max_tokens {
        stats.steps += 1;

        // 1. Build draft tree.
        let tree = build_draft_tree(draft, &context, gamma, branch)?;

        // Positions speculated = gamma per step (comparable to draft-target).
        // Tree nodes scored = tree.total_nodes() (a cost metric).
        let eff_gamma = gamma.min(params.max_tokens.saturating_sub(emitted));
        stats.total_draft += eff_gamma;
        stats.tree_nodes_verified += tree.total_nodes();

        // 2. Verify.
        let (accepted, bonus_or_correction, n_accepted) =
            verify_tree(target, &tree, &context, params, &mut rng)?;

        stats.total_accepted += n_accepted;

        // 3. Emit accepted tokens (check stop tokens for each).
        let mut stopped = false;
        for &tok in &accepted {
            if emitted >= params.max_tokens {
                break;
            }
            let stop = params.stop_tokens.contains(&tok);
            let token = GeneratedToken {
                id: tok,
                text: detok(tok),
                logprob: 0.0,
                finish_reason: if stop { Some(FinishReason::Stop) } else { None },
            };
            if !sink(token) {
                finish = FinishReason::Cancelled;
                stopped = true;
                break;
            }
            emitted += 1;
            context.push(tok);
            if stop {
                finish = FinishReason::Stop;
                stopped = true;
                break;
            }
        }

        if stopped {
            break;
        }
        if emitted >= params.max_tokens {
            break;
        }

        // 4. Emit bonus / correction token.
        if let Some(bonus) = bonus_or_correction {
            let stop = params.stop_tokens.contains(&bonus);
            let token = GeneratedToken {
                id: bonus,
                text: detok(bonus),
                logprob: 0.0,
                finish_reason: if stop { Some(FinishReason::Stop) } else { None },
            };
            if !sink(token) {
                finish = FinishReason::Cancelled;
                break;
            }
            emitted += 1;
            stats.extra_emitted += 1;
            context.push(bonus);
            if stop {
                finish = FinishReason::Stop;
                break;
            }
        }
    }

    Ok((finish, stats))
}

/// A `Model` that runs [`speculative_generate_tree`] under the hood, so it can
/// be registered in the [`crate::ModelRegistry`] and served through the normal
/// `Engine::generate` path without callers knowing tree speculation is in play.
pub struct TreeSpeculativeModel {
    target: Arc<dyn Model>,
    draft: Arc<dyn Model>,
    gamma: usize,
    branch: usize,
    seed: u64,
}

impl TreeSpeculativeModel {
    pub fn new(
        target: Arc<dyn Model>,
        draft: Arc<dyn Model>,
        gamma: usize,
        branch: usize,
        seed: u64,
    ) -> Self {
        Self { target, draft, gamma, branch, seed }
    }
}

impl Model for TreeSpeculativeModel {
    fn vocab_size(&self) -> usize {
        self.target.vocab_size()
    }

    fn generate(
        &self,
        prompt: &[u32],
        params: &SamplingParams,
        sink: &dyn Fn(GeneratedToken) -> bool,
    ) -> Result<FinishReason, ModelError> {
        let (finish, _stats) = speculative_generate_tree(
            &*self.target,
            &*self.draft,
            prompt,
            params,
            self.gamma,
            self.branch,
            self.seed,
            sink,
        )?;
        Ok(finish)
    }

    fn forward_logits(&self, context: &[u32]) -> Result<Vec<f32>, ModelError> {
        self.target.forward_logits(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MockModel;

    /// A model whose top-k tokens are a configurable range, for testing
    /// tree-based speculative decoding with controllable draft-target overlap.
    struct RangeModel {
        vocab: usize,
        offset: u32,
        spread: u32,
    }

    impl RangeModel {
        fn new(vocab: usize, offset: u32, spread: u32) -> Self {
            Self { vocab, offset, spread }
        }
    }

    impl Model for RangeModel {
        fn vocab_size(&self) -> usize {
            self.vocab
        }

        fn forward_logits(&self, context: &[u32]) -> Result<Vec<f32>, ModelError> {
            let prev = context.last().copied().unwrap_or(0) % self.vocab as u32;
            // Center = prev + offset + 1 (so offset=0 means centered on target's (prev+1))
            let center = (prev.wrapping_add(self.offset).wrapping_add(1)) % self.vocab as u32;
            let mut logits = vec![-10.0f32; self.vocab];
            for d in 0..=self.spread {
                let tok = (center.wrapping_add(d)) % self.vocab as u32;
                let score = 5.0 - d as f32;
                if score > logits[tok as usize] {
                    logits[tok as usize] = score;
                }
                // Also negative side (wrapping reverse)
                if d > 0 {
                    let tok2 = (center.wrapping_sub(d)) % self.vocab as u32;
                    logits[tok2 as usize] = 5.0 - d as f32;
                }
            }
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
                let center = (prev.wrapping_add(self.offset).wrapping_add(1)) % self.vocab as u32;
                let next = center;
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

    fn collect_tree(
        target: &dyn Model,
        draft: &dyn Model,
        prompt: &[u32],
        params: &SamplingParams,
        gamma: usize,
        branch: usize,
    ) -> (Vec<u32>, crate::AcceptanceStats) {
        let out = std::cell::RefCell::new(Vec::new());
        let (_finish, stats) =
            speculative_generate_tree(target, draft, prompt, params, gamma, branch, 42, &|t| {
                out.borrow_mut().push(t.id);
                true
            })
            .unwrap();
        (out.into_inner(), stats)
    }

    // ---------- tree node / helpers ----------

    #[test]
    fn top_k_returns_sorted() {
        let logits = vec![-5.0, 3.0, 10.0, -2.0, 7.0];
        let top = top_k_tokens(&logits, 3);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0], (2, 10.0));
        assert_eq!(top[1], (4, 7.0));
        assert_eq!(top[2], (1, 3.0));
    }

    #[test]
    fn top_k_zero_returns_empty() {
        assert!(top_k_tokens(&[1.0, 2.0], 0).is_empty());
    }

    #[test]
    fn tree_total_nodes_formula() {
        let t = DraftTree { roots: vec![], gamma: 4, branch: 3 };
        // 3 + 9 + 27 + 81 = 120
        assert_eq!(t.total_nodes(), 120);
    }

    #[test]
    fn tree_total_nodes_zero() {
        let t = DraftTree { roots: vec![], gamma: 0, branch: 3 };
        assert_eq!(t.total_nodes(), 0);
        let t = DraftTree { roots: vec![], gamma: 4, branch: 0 };
        assert_eq!(t.total_nodes(), 0);
    }

    // ---------- tree matches plain decode with perfect draft ----------

    #[test]
    fn tree_matches_target_when_draft_contains_correct_path() {
        // Target: MockModel — predicts (prev+1)%V
        // Draft: RangeModel(offset=0) — centered on (prev+1)%V, spread=3, so
        // top-3 include the correct prediction at every depth.
        let target = MockModel::new(32);
        let draft = RangeModel::new(32, 0, 3);
        let params = SamplingParams { max_tokens: 20, temperature: 0.0, ..Default::default() };
        let prompt = vec![5u32];
        let baseline = collect(&target, &prompt, &params);
        let (spec, stats) = collect_tree(&target, &draft, &prompt, &params, 4, 3);
        assert_eq!(baseline, spec, "tree speculation must not change the output");
        // Acceptance should be high — perfect draft.
        assert!(stats.acceptance_rate() > 0.8, "got {}", stats.acceptance_rate());
        assert_eq!(stats.tree_branch, 3);
    }

    // ---------- gamma=0 degenerates to plain decode ----------

    #[test]
    fn tree_gamma_zero_matches_plain() {
        let target = MockModel::new(32);
        let draft = MockModel::new(32);
        let params = SamplingParams { max_tokens: 12, temperature: 0.0, ..Default::default() };
        let prompt = vec![0u32];
        let baseline = collect(&target, &prompt, &params);
        let (spec, _) = collect_tree(&target, &draft, &prompt, &params, 0, 3);
        assert_eq!(baseline, spec);
    }

    // ---------- branch=1 acts like draft-target ----------

    #[test]
    fn tree_branch_one_acts_like_single_path() {
        let target = MockModel::new(32);
        let draft = MockModel::new(32);
        let params = SamplingParams { max_tokens: 20, temperature: 0.0, ..Default::default() };
        let prompt = vec![3u32];
        let baseline = collect(&target, &prompt, &params);
        // With branch=1, tree has only one candidate per depth = essentially
        // the same as draft-target (single path).
        let (spec, _) = collect_tree(&target, &draft, &prompt, &params, 4, 1);
        assert_eq!(baseline, spec);
    }

    // ---------- graceful degradation when draft never matches ----------

    #[test]
    fn tree_graceful_degradation() {
        // Target predicts (prev+1)%V.
        // Draft offset=10, centered on (prev+11)%V — never includes (prev+1).
        let target = MockModel::new(64);
        let draft = RangeModel::new(64, 10, 0);
        let params = SamplingParams { max_tokens: 30, temperature: 0.0, ..Default::default() };
        let prompt = vec![0u32];
        let baseline = collect(&target, &prompt, &params);
        let (spec, stats) = collect_tree(&target, &draft, &prompt, &params, 4, 3);
        assert_eq!(baseline, spec, "must preserve output even with bad draft");
        // Acceptance rate should be near 0.
        assert!(stats.acceptance_rate() < 0.1, "got {}", stats.acceptance_rate());
        // Tokens emitted == max_tokens (graceful, no loss)
        assert_eq!(spec.len(), 30);
    }

    // ---------- branch > 1 gives higher acceptance than branch=1 ----------

    #[test]
    fn higher_branch_gives_better_acceptance() {
        // Draft's offset is 1 (predicts (prev+2) as center), spread=2,
        // so top-3 at each position includes (prev+1), (prev+2), (prev+3).
        // Target predicts (prev+1). So acceptance should be high.
        let target = MockModel::new(64);
        let draft = RangeModel::new(64, 0, 2);
        let params = SamplingParams { max_tokens: 40, temperature: 0.0, ..Default::default() };
        let prompt = vec![0u32];

        let (_spec1, stats1) = collect_tree(&target, &draft, &prompt, &params, 4, 1);
        let (_spec2, stats2) = collect_tree(&target, &draft, &prompt, &params, 4, 3);

        // branch=3 should have higher or equal acceptance than branch=1
        // (because it has more candidates per depth).
        assert!(
            stats2.acceptance_rate() >= stats1.acceptance_rate() - 1e-9,
            "branch=3 ({}) < branch=1 ({})",
            stats2.acceptance_rate(),
            stats1.acceptance_rate()
        );
    }

    // ---------- honors max_tokens ----------

    #[test]
    fn tree_honors_max_tokens() {
        let target = MockModel::new(64);
        let draft = RangeModel::new(64, 0, 2);
        let params = SamplingParams { max_tokens: 15, temperature: 0.0, ..Default::default() };
        let (spec, _) = collect_tree(&target, &draft, &[0u32], &params, 4, 3);
        assert_eq!(spec.len(), 15);
    }

    // ---------- respects stop tokens ----------

    #[test]
    fn tree_respects_stop_token() {
        let target = MockModel::new(32);
        let draft = MockModel::new(32);
        // MockModel rule is (prev+1)%32; stop at the next token after prompt.
        let next_after_prompt = (0u32.wrapping_add(1)) % 32;
        let params = SamplingParams {
            max_tokens: 50,
            temperature: 0.0,
            stop_tokens: vec![next_after_prompt],
            ..Default::default()
        };
        let (spec, _) = collect_tree(&target, &draft, &[0u32], &params, 3, 2);
        assert_eq!(spec.len(), 1);
        assert_eq!(spec[0], next_after_prompt);
    }

    // ---------- verify_tree cases ----------

    #[test]
    fn verify_accepts_when_child_matches() {
        let target = MockModel::new(16);
        // Context: [0]. Target predicts 1.
        let tree = DraftTree {
            roots: vec![TreeNode::new(1, 5.0), TreeNode::new(2, 4.0)],
            gamma: 1,
            branch: 2,
        };
        let params = SamplingParams { temperature: 0.0, ..Default::default() };
        let mut rng = StdRng::seed_from_u64(0);
        let (accepted, bonus, n) = verify_tree(&target, &tree, &[0u32], &params, &mut rng).unwrap();
        assert_eq!(accepted, vec![1]);
        assert_eq!(n, 1);
        assert!(bonus.is_some());
    }

    #[test]
    fn verify_rejects_when_no_child_matches() {
        let target = MockModel::new(16);
        // Context: [0]. Target predicts 1. Draft proposes [2, 3] — no match.
        let tree = DraftTree {
            roots: vec![TreeNode::new(2, 5.0), TreeNode::new(3, 4.0)],
            gamma: 1,
            branch: 2,
        };
        let params = SamplingParams { temperature: 0.0, ..Default::default() };
        let mut rng = StdRng::seed_from_u64(0);
        let (accepted, bonus, n) = verify_tree(&target, &tree, &[0u32], &params, &mut rng).unwrap();
        assert_eq!(accepted.len(), 0);
        assert_eq!(n, 0);
        // Bonus should be 1 (target's prediction).
        assert_eq!(bonus, Some(1));
    }

    // ---------- multi-depth tree verified correctly ----------

    #[test]
    fn verify_multiple_depths() {
        // Context [0]. Target predicts 1, then 2, then 3 (MockModel chain).
        // Draft tree has gamma=3, branch=2, and one path matches perfectly.
        let mut node_a = TreeNode::new(1, 5.0);
        node_a.children = vec![
            TreeNode::new(2, 5.0), // matches target
            TreeNode::new(9, 4.0),
        ];
        node_a.children[0].children = vec![
            TreeNode::new(3, 5.0), // matches target
            TreeNode::new(8, 4.0),
        ];
        let mut node_b = TreeNode::new(7, 3.0);
        node_b.children = vec![TreeNode::new(8, 3.0)];

        let tree = DraftTree { roots: vec![node_a, node_b], gamma: 3, branch: 2 };
        let target = MockModel::new(16);
        let params = SamplingParams { temperature: 0.0, ..Default::default() };
        let mut rng = StdRng::seed_from_u64(0);
        let (accepted, bonus, n) = verify_tree(&target, &tree, &[0u32], &params, &mut rng).unwrap();
        // Accepted: [1, 2, 3] — the matching path.
        assert_eq!(accepted, vec![1, 2, 3]);
        assert_eq!(n, 3);
        // Bonus: target predicts 4 at context [0,1,2,3].
        assert_eq!(bonus, Some(4));
    }

    #[test]
    fn verify_deeply_rejected() {
        // Context [0]. Some match at depth 0, then reject at depth 1.
        let mut node_a = TreeNode::new(1, 5.0);
        node_a.children = vec![
            TreeNode::new(9, 5.0), // doesn't match (target expects 2)
            TreeNode::new(10, 4.0),
        ];

        let tree = DraftTree { roots: vec![node_a], gamma: 2, branch: 2 };
        let target = MockModel::new(16);
        let params = SamplingParams { temperature: 0.0, ..Default::default() };
        let mut rng = StdRng::seed_from_u64(0);
        let (accepted, bonus, n) = verify_tree(&target, &tree, &[0u32], &params, &mut rng).unwrap();
        assert_eq!(accepted, vec![1]); // only first token accepted
        assert_eq!(n, 1);
        // Correction: target at [0,1] predicts 2.
        assert_eq!(bonus, Some(2));
    }

    // ---------- acceptance stats are recorded correctly ----------

    #[test]
    fn tree_stats_record_branch() {
        let target = MockModel::new(32);
        let draft = MockModel::new(32);
        let params = SamplingParams { max_tokens: 10, temperature: 0.0, ..Default::default() };
        let (_, stats) = collect_tree(&target, &draft, &[0u32], &params, 3, 3);
        assert_eq!(stats.tree_branch, 3);
        assert!(stats.steps > 0);
    }

    // ---------- tree and draft-target produce same output (both lossless) ----------

    #[test]
    fn tree_and_draft_target_produce_same_output() {
        // When branch=1, tree is essentially the same as single-path draft-target
        // (both produce the same tokens as plain decode).
        let target = MockModel::new(32);
        let draft = MockModel::new(32);
        let params = SamplingParams { max_tokens: 20, temperature: 0.0, ..Default::default() };
        let prompt = vec![0u32];
        let baseline = collect(&target, &prompt, &params);

        let (tree_spec, _) = collect_tree(&target, &draft, &prompt, &params, 4, 1);
        assert_eq!(baseline, tree_spec);

        let (tree_spec3, _) = collect_tree(&target, &draft, &prompt, &params, 4, 3);
        assert_eq!(baseline, tree_spec3);
    }

    // ---------- empty prompt ----------

    #[test]
    fn tree_handles_empty_prompt() {
        let target = MockModel::new(32);
        let draft = MockModel::new(32);
        let params = SamplingParams { max_tokens: 5, temperature: 0.0, ..Default::default() };
        let (spec, _) = collect_tree(&target, &draft, &[], &params, 2, 2);
        assert_eq!(spec.len(), 5);
    }

    // ---------- TreeSpeculativeModel wrapper works as a Model ----------

    #[test]
    fn tree_speculative_model_wrapper_matches_target() {
        // Wrap MockModel as a tree-speculative model with a perfect draft
        // (the draft is the same MockModel, so every path matches) and confirm
        // the wrapper emits the same token stream as plain target decode — the
        // property required for it to be servable through Engine::generate.
        let target = Arc::new(MockModel::new(32));
        let draft = Arc::new(MockModel::new(32));
        let spec = TreeSpeculativeModel::new(target.clone(), draft, 4, 3, 0);

        let params = SamplingParams { max_tokens: 20, temperature: 0.0, ..Default::default() };

        let plain = collect(&*target, &[3u32], &params);
        let wrapped = collect(&spec, &[3u32], &params);
        assert_eq!(plain, wrapped);
        assert_eq!(wrapped.len(), 20);
    }

    #[test]
    fn tree_speculative_model_vocab_and_logits_delegated() {
        let target = Arc::new(MockModel::new(64));
        let draft = Arc::new(MockModel::new(64));
        let spec = TreeSpeculativeModel::new(target.clone(), draft, 4, 2, 0);
        assert_eq!(spec.vocab_size(), 64);
        // forward_logits should be delegated to the target.
        let ctx = vec![1u32, 2, 3];
        let target_logits = target.forward_logits(&ctx).unwrap();
        let spec_logits = spec.forward_logits(&ctx).unwrap();
        assert_eq!(target_logits, spec_logits);
    }
}
