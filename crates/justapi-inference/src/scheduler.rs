//! Continuous-batching scheduler — interleaves prefill and decode across
//! concurrent sequences to maximise GPU utilisation.
//!
//! ## Model
//!
//! The [`Scheduler`] owns the block pool and the set of running sequences.
//! On each step the caller calls [`schedule`](Scheduler::schedule) which
//! returns a [`Schedule`] describing the work for this forward pass:
//!
//! - **Prefill** sequences consume their prompt (or one chunk of it) and
//!   produce the first token.  These are the compute-heavy steps.
//! - **Decode** sequences generate one token each.  These are memory-bandwidth
//!   bound and benefit greatly from being batched together.
//!
//! Finished sequences are removed on the next [`schedule`] call; their blocks
//! are released to the prefix cache.

use std::collections::{HashMap, VecDeque};

use crate::kv_cache::{BlockId, KvBlockPool, Sequence, BLOCK_SIZE};
use crate::model::SamplingParams;
use crate::radix_cache::{RadixPrefixCache, RadixPrefixCacheStats};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Tunable knobs for the scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum number of sequences in a single batch.
    pub max_num_seqs: usize,
    /// Maximum number of tokens processed in a single forward pass
    /// (prefill tokens + decode sequences count toward this budget).
    pub max_num_batched_tokens: usize,
    /// Maximum number of tokens a single sequence may generate.
    pub max_seq_len: usize,
    /// When `Some(n)`, prompts longer than `n` tokens are split into chunks
    /// of (at most) `n` tokens so that no single prefill starves decode.
    /// `None` disables chunking (the entire prompt is prefilled at once).
    pub chunked_prefill_size: Option<usize>,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_num_seqs: 256,
            max_num_batched_tokens: 4096,
            max_seq_len: 4096,
            chunked_prefill_size: Some(512),
        }
    }
}

// ---------------------------------------------------------------------------
// Inbound request
// ---------------------------------------------------------------------------

/// A new sequence arriving from the HTTP / control-plane layer.
#[derive(Debug, Clone)]
pub struct NewRequest {
    /// Opaque caller-provided identifier (echoed back on the response stream).
    pub id: u64,
    /// Tokenised prompt (already tokenised by the caller's tokenizer).
    pub prompt: Vec<u32>,
    /// Generation parameters.
    pub sampling_params: SamplingParams,
    /// Prefix-cache lookup result: the number of tokens that exist in the
    /// cache so the scheduler can skip computing them.
    pub prefix_cached_tokens: usize,
    /// KV-block IDs restored from the prefix cache (length matches
    /// `prefix_cached_tokens / BLOCK_SIZE`).
    pub cached_blocks: Vec<BlockId>,
}

// ---------------------------------------------------------------------------
// Schedule output
// ---------------------------------------------------------------------------

/// A single prefill sequence for this forward step.
#[derive(Debug)]
pub struct PrefillStep {
    pub seq_id: u64,
    /// The tokens to process in this step (one chunk or the full remainder).
    pub tokens: Vec<u32>,
    /// How many tokens of this prompt have already been computed (from
    /// prefix-cache or earlier chunks).
    pub computed_tokens: usize,
}

/// The set of work the engine should execute in the next forward pass.
#[derive(Debug)]
pub struct Schedule {
    /// Sequences that need a prefill forward pass.
    pub prefill: Vec<PrefillStep>,
    /// Sequence ids that need a single-token decode forward pass.
    pub decode: Vec<u64>,
    /// If `true` the scheduler could not admit a waiting request because
    /// either `max_num_seqs` or `max_num_batched_tokens` would be exceeded.
    /// The caller can use this to produce a 503 / Retry-After.
    pub back_pressure: bool,
}

// ---------------------------------------------------------------------------
// Scheduler statistics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SchedulerStats {
    pub num_waiting: usize,
    pub num_running: usize,
    /// How many of the running sequences are still prefilling.
    pub num_prefilling: usize,
    /// GPU token budget utilisation for the last schedule.
    pub budget_used_pct: f32,
    /// Whether the last schedule hit a resource limit.
    pub back_pressure: bool,
    /// Prefix-cache (RadixAttention) reuse effectiveness — the operator-visible
    /// signal that shared prompts are skipping recomputation.
    pub prefix: RadixPrefixCacheStats,
    /// Number of KV blocks currently resident as cached prefix entries (pinned
    /// in the pool and tracked by the radix tree).
    pub cached_kv_blocks: usize,
}

// ---------------------------------------------------------------------------
// Internal per-sequence state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeqState {
    /// Still processing the prompt (one or more prefill steps remain).
    Prefilling,
    /// Prompt fully consumed; generating tokens one at a time.
    Decoding,
}

struct RunningSeq {
    seq: Sequence,
    /// Full original prompt (kept so we can feed remaining chunks).
    prompt: Vec<u32>,
    params: SamplingParams,
    /// Number of prompt tokens that have been consumed across all prefill
    /// steps so far (including any prefix-cache restoration).
    num_prefilled: usize,
    /// Number of decode tokens produced so far.
    num_generated: usize,
    state: SeqState,
}

// ---------------------------------------------------------------------------
// Transferable sequence (for disaggregated prefill/decode)
// ---------------------------------------------------------------------------

/// A sequence that has finished prefill on one scheduler (pool) and is ready
/// to be handed off to another (e.g. a separate decode scheduler/pool).
///
/// Mirrors the KV-cache transfer in disaggregated serving (DistServe /
/// NVIDIA Dynamo): the prefill GPU computes the prompt's KV cache and ships
/// it to the decode GPU, which then runs token-by-token generation. Here the
/// "KV cache" is the logical `Sequence` (block ids + token count); a real
/// engine would copy the actual tensors and record `num_prefilled` tokens as
/// the transfer volume.
#[derive(Debug, Clone)]
pub struct TransferableSequence {
    /// The internal sequence id assigned by the source (prefill) scheduler.
    pub src_seq_id: u64,
    /// The caller-provided request id (echoed from [`NewRequest::id`]).
    pub req_id: u64,
    /// The full tokenised prompt.
    pub prompt: Vec<u32>,
    /// Generation parameters.
    pub params: SamplingParams,
    /// Number of prompt tokens that were computed during prefill.
    pub num_prefilled: usize,
    /// Tokens already produced (the first generated token is emitted by the
    /// final prefill step, so this is typically 1).
    pub num_generated: usize,
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

/// Owns the block pool, the wait queue, and the running batch of sequences.
///
/// ## Prefix caching
///
/// The scheduler owns a [`RadixPrefixCache`] (RadixAttention-style). On
/// admission it looks up the incoming prompt; any matched prefix reuses the
/// cached KV blocks (the prompt is not recomputed) and only the unique suffix
/// is prefilled. When a sequence finishes, its blocks are inserted into the
/// tree and *pinned* in the pool so the clock-sweep evictor can never recycle
/// them out from under the cache. Under memory pressure the scheduler drives
/// eviction itself via [`RadixPrefixCache::evict_filter`], freeing the LRU
/// leaf's blocks back to the pool — this keeps a single eviction authority and
/// avoids stale block ids.
///
/// Usage cycle (called from the engine's generation thread):
///
/// ```ignore
/// loop {
///     let schedule = scheduler.schedule();
///     if schedule.is_empty() { break; }
///     // engine.forward(schedule) -> Vec<SequenceEvent>
///     scheduler.on_step_complete(events);
/// }
/// ```
pub struct Scheduler {
    config: SchedulerConfig,
    pool: KvBlockPool,
    /// RadixAttention-style prefix cache shared across requests.
    prefix: RadixPrefixCache,
    /// Number of live (running) sequences currently referencing each
    /// prefix-cached block. A block with `prefix_refs == 0` is owned purely by
    /// the tree (pinned in the pool); `== 1` means exactly one live sequence
    /// re-used it. Admission only reuses a block when its live-ref is 0, which
    /// keeps a cached block from being claimed by two concurrent sequences.
    prefix_refs: HashMap<BlockId, usize>,
    running: Vec<RunningSeq>,
    wait_queue: VecDeque<NewRequest>,
    next_seq_id: u64,
    last_back_pressure: bool,
}

impl Scheduler {
    /// Create a new scheduler.
    pub fn new(config: SchedulerConfig, pool: KvBlockPool) -> Self {
        Self {
            config,
            pool,
            prefix: RadixPrefixCache::new(),
            prefix_refs: HashMap::new(),
            running: Vec::new(),
            wait_queue: VecDeque::new(),
            next_seq_id: 1,
            last_back_pressure: false,
        }
    }

    /// Borrow the scheduler's prefix cache (for stats / observability).
    pub fn prefix_cache(&self) -> &RadixPrefixCache {
        &self.prefix
    }

    /// Prefix-cache (RadixAttention) effectiveness snapshot.
    pub fn prefix_cache_stats(&self) -> RadixPrefixCacheStats {
        self.prefix.stats()
    }

    // -- accessors ----------------------------------------------------------

    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }

    pub fn pool(&self) -> &KvBlockPool {
        &self.pool
    }

    pub fn pool_mut(&mut self) -> &mut KvBlockPool {
        &mut self.pool
    }

    pub fn num_waiting(&self) -> usize {
        self.wait_queue.len()
    }

    pub fn num_running(&self) -> usize {
        self.running.len()
    }

    /// Snapshot of scheduler state.
    pub fn stats(&self) -> SchedulerStats {
        let num_prefilling =
            self.running.iter().filter(|s| s.state == SeqState::Prefilling).count();
        SchedulerStats {
            num_waiting: self.wait_queue.len(),
            num_running: self.running.len(),
            num_prefilling,
            budget_used_pct: 0.0,
            back_pressure: self.last_back_pressure,
            prefix: self.prefix.stats(),
            cached_kv_blocks: self.pool.cached(),
        }
    }

    // -- request admission --------------------------------------------------

    /// Enqueue a new generation request.
    ///
    /// Assigns the sequence id the scheduler will use for this request and
    /// returns it so callers can correlate the request with scheduler events
    /// (e.g. map a scheduler seq id → a token stream). The request may be
    /// scheduled immediately if capacity is available, or placed in the wait
    /// queue.
    pub fn add_request(&mut self, mut req: NewRequest) -> u64 {
        let seq_id = self.next_seq_id;
        self.next_seq_id += 1;
        req.id = seq_id;
        self.wait_queue.push_back(req);
        seq_id
    }

    /// The scheduler seq ids currently running (for completion detection by an
    /// external driver).
    pub fn running_ids(&self) -> Vec<u64> {
        self.running.iter().map(|s| s.seq.id()).collect()
    }

    /// How many tokens a running sequence has generated so far, if it is
    /// currently running.
    pub fn running_seq_generated(&self, seq_id: u64) -> Option<usize> {
        self.running.iter().find(|s| s.seq.id() == seq_id).map(|s| s.num_generated)
    }

    // -- scheduling ---------------------------------------------------------

    /// Produce a [`Schedule`] for the next forward pass.
    ///
    /// 1. Cache and remove completed sequences (their KV blocks become reusable
    ///    prefix-cache entries for future requests).
    /// 2. Admit as many waiting requests as the budget allows, reusing any
    ///    cached prefix blocks via the radix tree.
    /// 3. Assign prefill / decode roles.
    pub fn schedule(&mut self) -> Schedule {
        // ---- 1. cache + remove finished / overflow sequences ----
        // Collect the finished sequences' prompts + blocks before dropping them
        // so their KV blocks can be promoted into the prefix cache (this also
        // fixes the block leak the naive `retain` drop would otherwise cause).
        let mut finished = Vec::new();
        self.running.retain(|s| {
            let done = s.num_generated >= s.params.max_tokens
                || s.seq.num_tokens() >= self.config.max_seq_len;
            if done {
                finished.push((s.prompt.clone(), s.seq.blocks().to_vec()));
                false
            } else {
                true
            }
        });
        for (prompt, blocks) in &finished {
            self.cache_completed(prompt, blocks);
        }

        // ---- 2. admit waiting requests ----
        let mut blocked = false;
        while self.running.len() < self.config.max_num_seqs {
            let Some(req_ref) = self.wait_queue.front() else {
                break;
            };

            // Resolve the prefix-cache hit: prefer an explicit caller-provided
            // hint (`cached_blocks`), otherwise consult the radix tree directly.
            let (cached_blocks, cached_tokens) = if !req_ref.cached_blocks.is_empty() {
                (req_ref.cached_blocks.clone(), req_ref.prefix_cached_tokens)
            } else {
                match self.prefix.lookup(&req_ref.prompt) {
                    Some((matched, blocks)) => (blocks, matched),
                    None => (Vec::new(), 0),
                }
            };

            let total_blocks = req_ref.prompt.len().div_ceil(BLOCK_SIZE);
            let cached_n = cached_blocks.len();
            let uncached_blocks = total_blocks.saturating_sub(cached_n);
            // Admission back-pressure uses a token-count estimate (excluding the
            // reusable cached prefix) so the heuristic matches prefill cost.
            let uncached_tokens = req_ref.prompt.len().saturating_sub(cached_tokens);
            let est_total = uncached_tokens + req_ref.sampling_params.max_tokens;
            if est_total > self.config.max_num_batched_tokens * 2 {
                blocked = true;
                break;
            }
            let req = self.wait_queue.pop_front().unwrap();

            // The seq id was assigned in `add_request` and is echoed on
            // `req.id` so external drivers can correlate scheduler events.
            let mut seq = Sequence::new(req.id);

            // Reuse cached prefix blocks (skip recomputing them).
            if !cached_blocks.is_empty() {
                seq.restore_from_cache(&cached_blocks, cached_tokens, &mut self.pool);
                for &b in &cached_blocks {
                    // Block is now owned by a live sequence; unpin and record a
                    // live reference so eviction won't free it under us.
                    self.pool.unpin(b);
                    *self.prefix_refs.entry(b).or_insert(0) += 1;
                }
            }

            // Allocate the uncached tail, triggering radix eviction if the pool
            // is exhausted so blocks become available on the retry.
            let mut alloc_failed = false;
            for _ in 0..uncached_blocks {
                if seq.grow(&mut self.pool).is_none() {
                    let freed = self.prefix.evict_filter(uncached_blocks, |bs| {
                        bs.iter().all(|b| self.prefix_refs.get(b).copied().unwrap_or(0) == 0)
                    });
                    for b in freed {
                        self.pool.unpin(b);
                        self.pool.free_cached(b);
                        self.prefix_refs.remove(&b);
                    }
                    if seq.grow(&mut self.pool).is_none() {
                        alloc_failed = true;
                        break;
                    }
                }
            }
            if alloc_failed {
                blocked = true;
                self.wait_queue.push_front(req);
                break;
            }

            let num_prefilled = cached_n * BLOCK_SIZE;
            let state = if num_prefilled >= req.prompt.len() {
                SeqState::Decoding
            } else {
                SeqState::Prefilling
            };

            self.running.push(RunningSeq {
                seq,
                prompt: req.prompt,
                params: req.sampling_params,
                num_prefilled,
                num_generated: 0,
                state,
            });
        }

        // Back-pressure: true when there are waiting requests that couldn't
        // be admitted (either blocked by estimate or by capacity).
        let back_pressure = blocked || !self.wait_queue.is_empty();

        // ---- 3. build schedule ----
        let mut prefill = Vec::new();
        let mut decode = Vec::new();
        let mut token_budget = self.config.max_num_batched_tokens;

        // Decode capacity: at most one token per running sequence that is
        // in decode mode.
        let decode_count = self.running.iter().filter(|s| s.state == SeqState::Decoding).count();
        let decode_tokens = decode_count.min(token_budget);
        token_budget = token_budget.saturating_sub(decode_tokens);

        // Prefill capacity: fill remaining budget with prefill steps.
        for s in &mut self.running {
            if s.state != SeqState::Prefilling {
                continue;
            }

            let remaining = s.prompt.len().saturating_sub(s.num_prefilled);
            if remaining == 0 {
                s.state = SeqState::Decoding;
                continue;
            }

            let chunk = match self.config.chunked_prefill_size {
                Some(chunk_size) => remaining.min(chunk_size).min(token_budget),
                None => remaining.min(token_budget),
            };
            if chunk == 0 {
                break; // budget exhausted
            }

            let end = s.num_prefilled + chunk;
            let tokens = s.prompt[s.num_prefilled..end].to_vec();
            let computed_tokens = s.num_prefilled;

            token_budget = token_budget.saturating_sub(chunk);
            s.num_prefilled = end;
            if s.num_prefilled >= s.prompt.len() {
                // This chunk finished the prompt — next step will be decode.
                s.state = SeqState::Decoding;
            }

            prefill.push(PrefillStep { seq_id: s.seq.id(), tokens, computed_tokens });
        }

        // Fill decode list.
        for s in &self.running {
            if s.state == SeqState::Decoding && decode.len() < decode_tokens {
                decode.push(s.seq.id());
            }
        }

        self.last_back_pressure = back_pressure;

        Schedule { prefill, decode, back_pressure }
    }

    // -- step completion ----------------------------------------------------

    /// Promote a finished sequence's KV blocks into the prefix cache.
    ///
    /// Inserts the block-aligned prompt prefix into the radix tree (idempotent
    /// for shared prefixes) and pins the blocks in the pool so the clock-sweep
    /// evictor cannot recycle them out from under the cache. Blocks still
    /// referenced by another live sequence are left in-use.
    fn cache_completed(&mut self, prompt: &[u32], blocks: &[BlockId]) {
        // Cache only the block-aligned prompt prefix — generated tokens vary
        // per request and an unaligned tail can't form a fixed shared prefix.
        let cache_blocks = (prompt.len() / BLOCK_SIZE).min(blocks.len());
        if cache_blocks == 0 {
            return;
        }
        let cache_tokens = cache_blocks * BLOCK_SIZE;
        self.prefix.insert(&prompt[..cache_tokens], &blocks[..cache_blocks]);
        for &b in &blocks[..cache_blocks] {
            let entry = self.prefix_refs.entry(b).or_insert(0);
            if *entry == 0 {
                // Fresh or pure-radix block: this sequence is its sole live
                // owner. Release to the cached state and pin for the tree.
                self.pool.release(b, 0);
                self.pool.pin(b);
            } else {
                // Block was reused by this sequence (live-ref == 1). Drop our
                // hold; if no other live sequence references it, pin it back.
                *entry -= 1;
                if *entry == 0 {
                    self.pool.release(b, 0);
                    self.pool.pin(b);
                }
                // else: another live sequence still holds it — leave in-use.
            }
        }
    }

    /// Inform the scheduler that a forward step completed.
    ///
    /// `seq_id` is the sequence that was processed; `num_new_tokens` is how
    /// many tokens were generated (1 for decode, >1 for prefill that
    /// produced multiple tokens via speculation — typically 1 per prefill
    /// step as well since the first generated token is sampled from logits).
    pub fn on_step_complete(&mut self, seq_id: u64, num_new_tokens: usize) {
        if let Some(s) = self.running.iter_mut().find(|s| s.seq.id() == seq_id) {
            s.num_generated += num_new_tokens;
            s.seq.advance(num_new_tokens);

            // After a prefill step that finished the prompt, the next call
            // to schedule will treat this sequence as decode.
        }
    }

    /// Immediately remove a sequence (client disconnected, error, etc.).
    ///
    /// Its blocks are promoted into the prefix cache (reusable by later
    /// requests) rather than leaked.
    pub fn cancel(&mut self, seq_id: u64) {
        if let Some(pos) = self.running.iter().position(|s| s.seq.id() == seq_id) {
            let s = self.running.remove(pos);
            self.cache_completed(&s.prompt, s.seq.blocks());
        }
    }

    /// Return true if no sequences are running or waiting.
    pub fn is_idle(&self) -> bool {
        self.running.is_empty() && self.wait_queue.is_empty()
    }

    // -- disaggregated prefill/decode (KV transfer) -------------------------

    /// Remove every sequence that has *finished prefill* (transitioned to decode
    /// with the full prompt consumed) from this scheduler, releasing its blocks
    /// back to this pool, and return them as [`TransferableSequence`]s ready to
    /// be admitted to another scheduler.
    ///
    /// Sequences still mid-prefill are retained. This is the prefill-side half
    /// of disaggregated P/D; pair with [`Scheduler::admit_transferred`].
    pub fn take_completed_prefill(&mut self) -> Vec<TransferableSequence> {
        let mut out = Vec::new();
        let mut keep = Vec::new();
        for s in self.running.drain(..) {
            let ready = s.state == SeqState::Decoding && s.num_prefilled >= s.prompt.len();
            if ready {
                for &b in s.seq.blocks() {
                    // KV is handed to another pool; clear this pool's live-ref
                    // bookkeeping and release the blocks here.
                    self.prefix_refs.remove(&b);
                    self.pool.release(b, 0);
                }
                out.push(TransferableSequence {
                    src_seq_id: s.seq.id(),
                    // The caller id is not stored on RunningSeq; echo the
                    // internal seq id so callers can correlate if needed.
                    req_id: s.seq.id(),
                    prompt: s.prompt,
                    params: s.params,
                    num_prefilled: s.num_prefilled,
                    num_generated: s.num_generated,
                });
            } else {
                keep.push(s);
            }
        }
        self.running = keep;
        out
    }

    /// Admit a sequence that arrived pre-prefilled from another scheduler
    /// (see [`Scheduler::take_completed_prefill`]). Allocates fresh blocks in
    /// *this* pool for the already-computed token count and resumes decode.
    pub fn admit_transferred(&mut self, t: TransferableSequence) {
        let mut seq = Sequence::new(self.next_seq_id);
        self.next_seq_id += 1;
        let total_tokens = t.num_prefilled + t.num_generated;
        let needed_blocks = total_tokens.div_ceil(BLOCK_SIZE);
        for _ in 0..needed_blocks {
            if seq.grow(&mut self.pool).is_none() {
                // Pool exhausted — drop the transfer. Callers must size the
                // decode pool to accept the expected concurrency.
                return;
            }
        }
        seq.advance(total_tokens);
        self.running.push(RunningSeq {
            seq,
            prompt: t.prompt,
            params: t.params,
            num_prefilled: t.num_prefilled,
            num_generated: t.num_generated,
            state: SeqState::Decoding,
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SamplingParams;

    fn basic_pool() -> KvBlockPool {
        KvBlockPool::new(1024)
    }

    fn default_config() -> SchedulerConfig {
        SchedulerConfig {
            max_num_seqs: 4,
            max_num_batched_tokens: 64,
            max_seq_len: 128,
            chunked_prefill_size: Some(16),
        }
    }

    fn short_prompt(id: u64) -> NewRequest {
        NewRequest {
            id,
            prompt: (0..8).collect(),
            sampling_params: SamplingParams { max_tokens: 16, ..Default::default() },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        }
    }

    #[test]
    fn scheduler_starts_idle() {
        let pool = basic_pool();
        let sched = Scheduler::new(default_config(), pool);
        assert!(sched.is_idle());
        assert_eq!(sched.num_running(), 0);
        assert_eq!(sched.num_waiting(), 0);
    }

    #[test]
    fn schedule_single_prefill() {
        let mut sched = Scheduler::new(default_config(), basic_pool());
        sched.add_request(short_prompt(1));

        let s = sched.schedule();
        assert_eq!(s.prefill.len(), 1);
        assert!(s.decode.is_empty());
        assert!(!s.back_pressure);

        let step = &s.prefill[0];
        assert_eq!(step.seq_id, 1);
        // Chunked prefill caps at 16, prompt is 8 — fits in one chunk.
        assert_eq!(step.tokens.len(), 8);
        assert_eq!(step.computed_tokens, 0);
    }

    #[test]
    fn schedule_chunked_prefill() {
        let cfg = SchedulerConfig {
            max_num_seqs: 4,
            max_num_batched_tokens: 64,
            max_seq_len: 128,
            chunked_prefill_size: Some(4),
        };
        let mut sched = Scheduler::new(cfg, basic_pool());
        sched.add_request(NewRequest {
            id: 1,
            prompt: (0..10).collect(), // 10 tokens
            sampling_params: SamplingParams { max_tokens: 8, ..Default::default() },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        });

        // First schedule: chunk of 4 tokens.
        let s = sched.schedule();
        assert_eq!(s.prefill.len(), 1);
        assert_eq!(s.prefill[0].tokens.len(), 4);
        assert_eq!(s.prefill[0].computed_tokens, 0);

        // After step complete: 0 generated (prefill just computed KV).
        sched.on_step_complete(1, 1);

        // Second schedule: next chunk of 4.
        let s = sched.schedule();
        assert_eq!(s.prefill.len(), 1);
        assert_eq!(s.prefill[0].tokens.len(), 4);
        assert_eq!(s.prefill[0].computed_tokens, 4);

        sched.on_step_complete(1, 1);

        // Third schedule: remaining 2 tokens, then transitions to decode.
        let s = sched.schedule();
        assert_eq!(s.prefill.len(), 1);
        assert_eq!(s.prefill[0].tokens.len(), 2);
        assert_eq!(s.prefill[0].computed_tokens, 8);

        sched.on_step_complete(1, 1);

        // Now the sequence is in decode mode.
        let s = sched.schedule();
        assert!(s.prefill.is_empty());
        assert_eq!(s.decode.len(), 1);
        assert_eq!(s.decode[0], 1);
    }

    #[test]
    fn schedule_multiple_sequences_interleaved() {
        let cfg = SchedulerConfig {
            max_num_seqs: 4,
            max_num_batched_tokens: 32,
            max_seq_len: 128,
            chunked_prefill_size: Some(8),
        };
        let mut sched = Scheduler::new(cfg, basic_pool());

        // Add two requests with different-length prompts.
        // Internal seq_ids are assigned sequentially (1, 2, 3, …).
        sched.add_request(short_prompt(10)); // internal seq_id = 1
        sched.add_request(NewRequest {
            id: 20,
            prompt: (0..16).collect(),
            sampling_params: SamplingParams { max_tokens: 8, ..Default::default() },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        }); // internal seq_id = 2

        // Batch has capacity for both in one step.
        let s = sched.schedule();
        assert_eq!(s.prefill.len(), 2);
        assert!(s.decode.is_empty());

        // Complete first prefill for both.
        for step in &s.prefill {
            sched.on_step_complete(step.seq_id, 1);
        }

        // Second step: seq 1 finishes prefill (prompt=8, all consumed),
        // seq 2 gets second chunk (prompt=16, 8 consumed, 8 remaining).
        let s = sched.schedule();
        assert_eq!(s.prefill.len(), 1);
        assert_eq!(s.prefill[0].seq_id, 2); // internal seq_id
        assert_eq!(s.decode.len(), 1);
        assert_eq!(s.decode[0], 1); // internal seq_id
    }

    #[test]
    fn schedule_back_pressure_when_full() {
        let mut sched = Scheduler::new(
            SchedulerConfig {
                max_num_seqs: 1,
                max_num_batched_tokens: 64,
                max_seq_len: 128,
                chunked_prefill_size: None,
            },
            basic_pool(),
        );

        sched.add_request(short_prompt(1));
        let _ = sched.schedule();
        assert!(!sched.stats().back_pressure);

        // Second request must wait.
        sched.add_request(short_prompt(2));
        let s = sched.schedule();
        // First is still running (just prefilled, not finished).
        assert!(s.back_pressure);
        assert_eq!(sched.num_waiting(), 1);
    }

    #[test]
    fn schedule_finished_sequence_is_removed() {
        let mut sched = Scheduler::new(
            SchedulerConfig {
                max_num_seqs: 4,
                max_num_batched_tokens: 64,
                max_seq_len: 128,
                chunked_prefill_size: None,
            },
            basic_pool(),
        );

        sched.add_request(NewRequest {
            id: 1,
            prompt: (0..4).collect(),
            sampling_params: SamplingParams { max_tokens: 3, ..Default::default() },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        });

        // Prefill step (generates 1 token).
        let s = sched.schedule();
        assert_eq!(s.prefill.len(), 1);
        sched.on_step_complete(1, 1);

        // Decode 1 (2nd token).
        let s = sched.schedule();
        assert_eq!(s.decode.len(), 1);
        sched.on_step_complete(1, 1);

        // Decode 2 (3rd token = max_tokens = 3).
        let s = sched.schedule();
        assert_eq!(s.decode.len(), 1);
        sched.on_step_complete(1, 1);

        // Next schedule should remove it (num_generated >= max_tokens).
        let s = sched.schedule();
        assert!(s.prefill.is_empty());
        assert!(s.decode.is_empty());
        assert_eq!(sched.num_running(), 0);
    }

    #[test]
    fn schedule_cancel_sequence() {
        let mut sched = Scheduler::new(default_config(), basic_pool());
        sched.add_request(short_prompt(1));
        let _ = sched.schedule();
        assert_eq!(sched.num_running(), 1);

        sched.cancel(1);
        assert_eq!(sched.num_running(), 0);
        assert!(sched.is_idle());
    }

    #[test]
    fn schedule_prefix_cache_restoration() {
        let mut pool = KvBlockPool::new(64);
        // Pre-allocate 2 blocks and assign them as "cached".
        let b0 = pool.alloc().unwrap();
        let b1 = pool.alloc().unwrap();
        // Release to simulate them being in the prefix cache.
        pool.release(b0, 42);
        pool.release(b1, 43);

        let mut sched = Scheduler::new(
            SchedulerConfig {
                max_num_seqs: 4,
                max_num_batched_tokens: 64,
                max_seq_len: 128,
                chunked_prefill_size: Some(16),
            },
            pool,
        );

        sched.add_request(NewRequest {
            id: 1,
            prompt: (0..48).collect(),
            sampling_params: SamplingParams { max_tokens: 8, ..Default::default() },
            prefix_cached_tokens: 32, // 2 blocks × 16 tokens
            cached_blocks: vec![b0, b1],
        });

        let s = sched.schedule();
        assert_eq!(s.prefill.len(), 1);
        // Only the uncached remainder (48 - 32 = 16) is prefilled.
        assert_eq!(s.prefill[0].tokens.len(), 16);
        assert_eq!(s.prefill[0].computed_tokens, 32);
    }

    #[test]
    fn schedule_budget_limits_batch_size() {
        let cfg = SchedulerConfig {
            max_num_seqs: 10,
            max_num_batched_tokens: 12,
            max_seq_len: 128,
            chunked_prefill_size: Some(8),
        };
        let mut sched = Scheduler::new(cfg, basic_pool());

        // Add many requests with 8-token prompts.
        for i in 0..5 {
            sched.add_request(short_prompt(i + 1));
        }

        let s = sched.schedule();
        // Budget = 12. First prefill consumes 8, leaving 4 budget.
        // Second prefill gets a 4-token chunk (partial).
        assert_eq!(s.prefill.len(), 2);
        assert_eq!(s.prefill[0].tokens.len(), 8);
        assert_eq!(s.prefill[1].tokens.len(), 4);
        // Wait queue is empty — all 5 were admitted into running.
        // The token budget only limits how many *prefill* in one step, not admission.
        assert!(sched.num_waiting() == 0);
    }

    // ---- radix prefix-cache integration ----------------------------------

    /// Config that prefills the whole prompt in one step (no chunking) so a
    /// request finishes its prefix in a single forward pass.
    fn full_prefix_config() -> SchedulerConfig {
        SchedulerConfig {
            max_num_seqs: 4,
            max_num_batched_tokens: 256,
            max_seq_len: 4096,
            chunked_prefill_size: None,
        }
    }

    /// Drive the scheduler to idle, simulating one generated token per step.
    fn drain(sched: &mut Scheduler) {
        let mut guard = 0;
        while !sched.is_idle() && guard < 2000 {
            let s = sched.schedule();
            for p in &s.prefill {
                sched.on_step_complete(p.seq_id, 1);
            }
            for &d in &s.decode {
                sched.on_step_complete(d, 1);
            }
            guard += 1;
        }
    }

    #[test]
    fn schedule_radix_reuses_shared_prefix() {
        // Two requests with an identical 48-token prompt. The first computes
        // its prefix; the second must reuse it from the radix cache instead of
        // recomputing (no prefill step, hit recorded).
        let mut sched = Scheduler::new(full_prefix_config(), KvBlockPool::new(64));

        // Request 1: full prompt, 1 generated token.
        sched.add_request(NewRequest {
            id: 1,
            prompt: (0..48).collect(),
            sampling_params: SamplingParams { max_tokens: 1, ..Default::default() },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        });
        let s = sched.schedule();
        assert_eq!(s.prefill.len(), 1);
        assert_eq!(s.prefill[0].tokens.len(), 48);
        assert_eq!(s.prefill[0].computed_tokens, 0);
        sched.on_step_complete(1, 1); // generated the 1st token → finished

        // Run again so request 1 is dropped and cached.
        let _ = sched.schedule();
        assert_eq!(sched.prefix_cache().stats().nodes, 1);

        // Request 2: same prompt → full prefix cache hit.
        sched.add_request(NewRequest {
            id: 2,
            prompt: (0..48).collect(),
            sampling_params: SamplingParams { max_tokens: 1, ..Default::default() },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        });
        let s = sched.schedule();
        // Nothing to prefill; the cached prefix covers the whole prompt.
        assert!(s.prefill.is_empty(), "shared prefix should be reused, not prefilled");
        assert_eq!(s.decode.len(), 1);
        // The hit should have been recorded and 48 tokens saved.
        let stats = sched.prefix_cache().stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.tokens_saved, 48);
    }

    #[test]
    fn schedule_radix_reuses_partial_prefix() {
        // Request 2 extends request 1's prompt by one block. Only the new
        // block should be prefilled (computed_tokens == 48).
        let mut sched = Scheduler::new(full_prefix_config(), KvBlockPool::new(64));

        sched.add_request(NewRequest {
            id: 1,
            prompt: (0..48).collect(), // 3 blocks
            sampling_params: SamplingParams { max_tokens: 1, ..Default::default() },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        });
        let s = sched.schedule();
        sched.on_step_complete(s.prefill[0].seq_id, 1);
        let _ = sched.schedule(); // cache request 1

        // Request 2: 4 blocks, first 3 identical to request 1.
        sched.add_request(NewRequest {
            id: 2,
            prompt: (0..64).collect(),
            sampling_params: SamplingParams { max_tokens: 1, ..Default::default() },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        });
        let s = sched.schedule();
        assert_eq!(s.prefill.len(), 1);
        // Only the unique 4th block (16 tokens) is prefilled.
        assert_eq!(s.prefill[0].tokens.len(), 16);
        assert_eq!(s.prefill[0].computed_tokens, 48);
    }

    #[test]
    fn schedule_radix_evicts_under_pressure() {
        // Tiny pool: 6 blocks total. Each request needs 3 blocks and distinct
        // prompts (no cache hits), so the scheduler must evict cached prefixes
        // to make room. It must not deadlock or panic.
        let pool = KvBlockPool::new(6);
        let mut sched = Scheduler::new(full_prefix_config(), pool);

        for i in 0..10u64 {
            // Distinct 48-token prompts: shift the token range per request.
            let base = i as u32 * 100;
            let prompt: Vec<u32> = (base..base + 48).collect();
            sched.add_request(NewRequest {
                id: i,
                prompt,
                sampling_params: SamplingParams { max_tokens: 1, ..Default::default() },
                prefix_cached_tokens: 0,
                cached_blocks: Vec::new(),
            });
        }

        drain(&mut sched);
        // All 10 requests were admitted, computed, and cached without deadlock.
        assert!(sched.is_idle());
        assert_eq!(sched.num_running(), 0);
        assert_eq!(sched.num_waiting(), 0);
    }

    #[test]
    fn schedule_prefix_stats_reflect_reuse() {
        // The SchedulerStats surface must expose prefix reuse so a metrics
        // layer can observe it without reaching into the cache directly.
        let mut sched = Scheduler::new(full_prefix_config(), KvBlockPool::new(64));

        sched.add_request(NewRequest {
            id: 1,
            prompt: (0..48).collect(),
            sampling_params: SamplingParams { max_tokens: 1, ..Default::default() },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        });
        let s = sched.schedule();
        sched.on_step_complete(s.prefill[0].seq_id, 1);
        let _ = sched.schedule(); // cache request 1

        // After caching, resident prefix KV blocks > 0 and stats are visible.
        let cached = sched.stats();
        assert!(cached.cached_kv_blocks > 0, "prefix KV should be resident");
        assert_eq!(cached.prefix.nodes, 1);
        assert_eq!(cached.prefix.hits, 0);
        assert_eq!(cached.prefix.hash_collisions, 0);

        sched.add_request(NewRequest {
            id: 2,
            prompt: (0..48).collect(),
            sampling_params: SamplingParams { max_tokens: 1, ..Default::default() },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        });
        let _ = sched.schedule(); // reuses the cached prefix

        let reused = sched.stats();
        assert_eq!(reused.prefix.hits, 1);
        assert_eq!(reused.prefix.tokens_saved, 48);
        assert_eq!(reused.prefix.hash_collisions, 0);
    }
}
