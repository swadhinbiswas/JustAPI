//! Disaggregated prefill/decode scheduler.
//!
//! Owns two independent [`Scheduler`] instances — one for prefill (compute-heavy,
//! prompt processing) and one for decode (memory-bandwidth-bound, token generation).
//! Sequences that finish prefill are transferred from the prefill pool to the
//! decode pool via the KV-cache transfer protocol (see
//! [`Scheduler::take_completed_prefill`] and [`Scheduler::admit_transferred`]).
//!
//! This enables independent scaling of prefill and decode capacity:
//! - Prefill pool: optimised for a few sequences at a time (high compute).
//! - Decode pool: many sequences batched together for high throughput.

use crate::scheduler::{NewRequest, Schedule, Scheduler, SchedulerConfig, SchedulerStats};

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// Combined statistics for the disaggregated prefill/decode scheduler.
#[derive(Debug, Clone)]
pub struct PdStats {
    /// Stats from the prefill scheduler.
    pub prefill: SchedulerStats,
    /// Stats from the decode scheduler.
    pub decode: SchedulerStats,
    /// Total prompt tokens transferred to the decode pool across all
    /// completed sequences.
    pub total_transferred_tokens: usize,
}

// ---------------------------------------------------------------------------
// PdScheduler
// ---------------------------------------------------------------------------

/// Scheduler that owns independent prefill and decode pools.
///
/// The caller drives the disaggregated loop:
///
/// ```ignore
/// loop {
///     // 1. Schedule prefill work.
///     let pre_sched = pd.schedule_prefill();
///     // engine.forward(pre_sched) ...
///     for step in &pre_sched.prefill {
///         pd.on_prefill_complete(step.seq_id, 1);
///     }
///
///     // 2. Transfer fully-prefilled sequences to decode pool.
///     pd.transfer_completed();
///
///     // 3. Schedule decode work.
///     let dec_sched = pd.schedule_decode();
///     // engine.forward(dec_sched) ...
///     for seq_id in &dec_sched.decode {
///         pd.on_decode_complete(*seq_id, 1);
///     }
/// }
/// ```
pub struct PdScheduler {
    prefill: Scheduler,
    decode: Scheduler,
    total_transferred_tokens: usize,
}

impl PdScheduler {
    /// Create a new disaggregated scheduler with independent prefill and decode
    /// configurations and block pools.
    pub fn new(
        prefill_config: SchedulerConfig,
        prefill_pool: crate::kv_cache::KvBlockPool,
        decode_config: SchedulerConfig,
        decode_pool: crate::kv_cache::KvBlockPool,
    ) -> Self {
        Self {
            prefill: Scheduler::new(prefill_config, prefill_pool),
            decode: Scheduler::new(decode_config, decode_pool),
            total_transferred_tokens: 0,
        }
    }

    // -- accessors ----------------------------------------------------------

    /// Immutable reference to the prefill scheduler.
    pub fn prefill(&self) -> &Scheduler {
        &self.prefill
    }

    /// Immutable reference to the decode scheduler.
    pub fn decode(&self) -> &Scheduler {
        &self.decode
    }

    /// Number of tokens transferred from prefill to decode pool so far.
    pub fn total_transferred_tokens(&self) -> usize {
        self.total_transferred_tokens
    }

    /// Combined statistics snapshot.
    pub fn stats(&self) -> PdStats {
        PdStats {
            prefill: self.prefill.stats(),
            decode: self.decode.stats(),
            total_transferred_tokens: self.total_transferred_tokens,
        }
    }

    /// Return `true` when both schedulers are idle (no running or waiting
    /// sequences).
    pub fn is_idle(&self) -> bool {
        self.prefill.is_idle() && self.decode.is_idle()
    }

    // -- request admission --------------------------------------------------

    /// Enqueue a new generation request into the prefill scheduler.
    pub fn add_request(&mut self, req: NewRequest) {
        self.prefill.add_request(req);
    }

    // -- prefill scheduling -------------------------------------------------

    /// Schedule prefill work for the next forward step.
    ///
    /// Delegates to the prefill [`Scheduler::schedule`].
    pub fn schedule_prefill(&mut self) -> Schedule {
        self.prefill.schedule()
    }

    /// Mark a sequence's prefill step as complete.
    ///
    /// You **must** call [`PdScheduler::transfer_completed`] after all prefill
    /// steps have been reported to move fully-prefilled sequences into the
    /// decode pool.
    pub fn on_prefill_complete(&mut self, seq_id: u64, num_new_tokens: usize) {
        self.prefill.on_step_complete(seq_id, num_new_tokens);
    }

    /// Transfer all sequences that have finished prefill from the prefill
    /// scheduler to the decode scheduler.
    ///
    /// Returns the number of sequences transferred.
    pub fn transfer_completed(&mut self) -> usize {
        let transferred = self.prefill.take_completed_prefill();
        let count = transferred.len();
        for t in &transferred {
            self.total_transferred_tokens = self
                .total_transferred_tokens
                .saturating_add(t.num_prefilled);
        }
        for t in transferred {
            self.decode.admit_transferred(t);
        }
        count
    }

    // -- decode scheduling --------------------------------------------------

    /// Schedule decode work for the next forward step.
    ///
    /// Delegates to the decode [`Scheduler::schedule`].
    pub fn schedule_decode(&mut self) -> Schedule {
        self.decode.schedule()
    }

    /// Mark a sequence's decode step as complete.
    pub fn on_decode_complete(&mut self, seq_id: u64, num_new_tokens: usize) {
        self.decode.on_step_complete(seq_id, num_new_tokens);
    }

    /// Immediately cancel a sequence from both schedulers (client
    /// disconnected, error, etc.).
    pub fn cancel(&mut self, seq_id: u64) {
        self.prefill.cancel(seq_id);
        self.decode.cancel(seq_id);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv_cache::KvBlockPool;
    use crate::model::SamplingParams;

    fn pool(capacity: usize) -> KvBlockPool {
        KvBlockPool::new(capacity)
    }

    fn prefill_cfg() -> SchedulerConfig {
        SchedulerConfig {
            max_num_seqs: 1,
            max_num_batched_tokens: 64,
            max_seq_len: 128,
            chunked_prefill_size: None,
        }
    }

    fn decode_cfg() -> SchedulerConfig {
        SchedulerConfig {
            max_num_seqs: 8,
            max_num_batched_tokens: 128,
            max_seq_len: 128,
            chunked_prefill_size: None,
        }
    }

    fn short_req(id: u64) -> NewRequest {
        NewRequest {
            id,
            prompt: (0..8).collect(),
            sampling_params: SamplingParams {
                max_tokens: 16,
                ..Default::default()
            },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        }
    }

    #[test]
    fn pd_starts_idle() {
        let pd = PdScheduler::new(prefill_cfg(), pool(64), decode_cfg(), pool(128));
        assert!(pd.is_idle());
        assert_eq!(pd.total_transferred_tokens(), 0);
        assert_eq!(pd.prefill().num_running(), 0);
        assert_eq!(pd.decode().num_running(), 0);
    }

    #[test]
    fn pd_single_request_prefill_transfer_decode() {
        let mut pd = PdScheduler::new(prefill_cfg(), pool(64), decode_cfg(), pool(128));
        pd.add_request(short_req(1));

        // ---- prefill step ----
        let s = pd.schedule_prefill();
        assert_eq!(s.prefill.len(), 1);
        assert!(s.decode.is_empty());
        pd.on_prefill_complete(s.prefill[0].seq_id, 1);

        // ---- transfer ----
        let n = pd.transfer_completed();
        assert_eq!(n, 1);
        assert!(pd.prefill().is_idle());
        assert_eq!(pd.total_transferred_tokens(), 8);

        // ---- decode step ----
        let s = pd.schedule_decode();
        assert!(s.prefill.is_empty());
        assert_eq!(s.decode.len(), 1);
        pd.on_decode_complete(s.decode[0], 1);
    }

    #[test]
    fn pd_independent_pool_sizing() {
        // Prefill admits 1 at a time; decode can run 8 concurrently.
        let mut pd = PdScheduler::new(prefill_cfg(), pool(64), decode_cfg(), pool(128));

        pd.add_request(short_req(10));
        pd.add_request(short_req(20));
        pd.add_request(short_req(30));

        // ---- transfer 1st request ----
        let s = pd.schedule_prefill();
        assert_eq!(s.prefill.len(), 1);
        pd.on_prefill_complete(s.prefill[0].seq_id, 1);
        pd.transfer_completed();

        // ---- transfer 2nd request ----
        let s = pd.schedule_prefill();
        assert_eq!(s.prefill.len(), 1);
        pd.on_prefill_complete(s.prefill[0].seq_id, 1);
        pd.transfer_completed();

        // Decode pool has 2 sequences now.
        let s = pd.schedule_decode();
        assert_eq!(s.decode.len(), 2);

        // 3rd request is still being prefilled (admitted one at a time).
        let s = pd.schedule_prefill();
        assert_eq!(s.prefill.len(), 1);
    }

    #[test]
    fn pd_total_transferred_tokens_accounting() {
        let mut pd = PdScheduler::new(prefill_cfg(), pool(64), decode_cfg(), pool(128));

        // 16-token prompt.
        pd.add_request(NewRequest {
            id: 1,
            prompt: (0..16).collect(),
            sampling_params: SamplingParams {
                max_tokens: 8,
                ..Default::default()
            },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        });
        let s = pd.schedule_prefill();
        pd.on_prefill_complete(s.prefill[0].seq_id, 1);
        pd.transfer_completed();
        assert_eq!(pd.total_transferred_tokens(), 16);

        // 8-token prompt.
        pd.add_request(NewRequest {
            id: 2,
            prompt: (0..8).collect(),
            sampling_params: SamplingParams {
                max_tokens: 8,
                ..Default::default()
            },
            prefix_cached_tokens: 0,
            cached_blocks: Vec::new(),
        });
        let s = pd.schedule_prefill();
        pd.on_prefill_complete(s.prefill[0].seq_id, 1);
        pd.transfer_completed();
        assert_eq!(pd.total_transferred_tokens(), 24);
    }

    #[test]
    fn pd_pool_block_allocation_release() {
        let prefill_pool = pool(256);
        let decode_pool = pool(256);

        // Prefill pool: nothing in-use before.
        assert_eq!(prefill_pool.in_use(), 0);
        // Decode pool: nothing in-use before.
        assert_eq!(decode_pool.in_use(), 0);

        let mut pd = PdScheduler::new(prefill_cfg(), prefill_pool, decode_cfg(), decode_pool);
        pd.add_request(short_req(1));
        let s = pd.schedule_prefill();

        // At this point the prefill pool should have blocks in-use.
        assert!(pd.prefill().pool().in_use() > 0);

        pd.on_prefill_complete(s.prefill[0].seq_id, 1);
        pd.transfer_completed();

        // Prefill pool released all its blocks (now cached, not in-use).
        assert_eq!(pd.prefill().pool().in_use(), 0);

        // Decode pool allocated blocks for the transferred sequence.
        assert!(pd.decode().pool().in_use() > 0);
    }
}
