//! Paged KV-cache manager for PagedAttention.
//!
//! Lean, pure-Rust data-structure layer — no Candle tensors, no GPU memory.
//! The model implementation (behind the `real` feature) maps [`BlockId`]s to
//! the actual tensor storage.
//!
//! ## Architecture
//!
//! - [`KvBlockPool`] — fixed-size block allocator with clock-sweep eviction.
//! - [`Sequence`] — a generation session holding a contiguous run of blocks.
//! - [`PrefixCache`] — hash-based prefix index for cross-request KV reuse.
//!
//! Blocks live in one of three states:
//!
//! 1. **Free** — available for allocation (in `free_list`).
//! 2. **In-use** — `ref_count > 0`, referenced by at least one [`Sequence`].
//! 3. **Cached** — `ref_count == 0` but still tracked by the [`PrefixCache`]
//!    and reclaimable via eviction.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Number of token positions per KV-cache block (PagedAttention standard).
pub const BLOCK_SIZE: usize = 16;

/// Maximum addressable blocks.
pub const MAX_BLOCKS: usize = 1 << 24;

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

/// Opaque identifier for a single KV-cache block in the pool.
pub type BlockId = u32;

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// Snapshot of pool usage.
#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    pub total: usize,
    pub allocated: usize,
    pub cached: usize,
    pub free: usize,
    pub pressure_pct: f32,
    pub evictions: u64,
}

/// Snapshot of prefix-cache effectiveness.
#[derive(Debug, Clone, Copy)]
pub struct PrefixCacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
}

// ---------------------------------------------------------------------------
// Per-block metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct BlockMeta {
    ref_count: u32,
    pinned: bool,
    /// Token-content hash used by the prefix cache. Zero means no cached
    /// content (so the block is either free or in-use with uncacheable data).
    hash: u64,
}

impl BlockMeta {
    const fn new() -> Self {
        Self { ref_count: 0, pinned: false, hash: 0 }
    }
}

// ---------------------------------------------------------------------------
// KvBlockPool
// ---------------------------------------------------------------------------

/// Fixed-size pool of KV-cache blocks with clock-sweep eviction.
///
/// When the free list is exhausted, [`alloc`](KvBlockPool::alloc) triggers a
/// clock-sweep scan to evict cacheable (ref\_count == 0) blocks.
pub struct KvBlockPool {
    metas: Vec<BlockMeta>,
    free: Vec<BlockId>,
    clock_hand: usize,
    eviction_count: u64,
    allocated_count: usize,
    cached_count: usize,
}

impl KvBlockPool {
    /// Create a pool with `capacity` pre-allocated blocks.
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.clamp(1, MAX_BLOCKS);
        let metas = vec![BlockMeta::new(); capacity];
        let free: Vec<BlockId> = (0..capacity as BlockId).rev().collect();
        Self { metas, free, clock_hand: 0, eviction_count: 0, allocated_count: 0, cached_count: 0 }
    }

    /// Total number of blocks the pool manages.
    pub fn capacity(&self) -> usize {
        self.metas.len()
    }

    /// Number of blocks currently allocated (in-use + cached).
    pub fn allocated(&self) -> usize {
        self.allocated_count
    }

    /// Number of blocks currently in-use (ref\_count > 0).
    pub fn in_use(&self) -> usize {
        self.allocated_count.saturating_sub(self.cached_count)
    }

    /// Number of cacheable (ref\_count == 0) but still tracked blocks.
    pub fn cached(&self) -> usize {
        self.cached_count
    }

    /// Number of blocks immediately available (free list).
    pub fn available(&self) -> usize {
        self.free.len()
    }

    /// Memory pressure as a percentage of total blocks.
    pub fn pressure_pct(&self) -> f32 {
        100.0 - (self.free.len() as f32 / self.metas.len() as f32 * 100.0)
    }

    /// Snapshot of current pool statistics.
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            total: self.metas.len(),
            allocated: self.allocated_count,
            cached: self.cached_count,
            free: self.free.len(),
            pressure_pct: self.pressure_pct(),
            evictions: self.eviction_count,
        }
    }

    /// Allocate one block from the pool.
    ///
    /// If the free list is empty, tries to evict a cacheable block. Returns
    /// `None` if no blocks can be freed (all pinned or in-use).
    pub fn alloc(&mut self) -> Option<BlockId> {
        // Fast path: free list has capacity.
        if let Some(block) = self.free.pop() {
            let meta = &mut self.metas[block as usize];
            debug_assert_eq!(meta.ref_count, 0);
            meta.ref_count = 1;
            meta.hash = 0;
            self.allocated_count += 1;
            return Some(block);
        }

        // Slow path: try clock-sweep eviction.
        // evict_one already decrements allocated_count and cached_count.
        let evicted = self.evict_one()?;
        let meta = &mut self.metas[evicted as usize];
        debug_assert_eq!(meta.ref_count, 0);
        meta.ref_count = 1;
        meta.hash = 0;
        self.allocated_count += 1;
        Some(evicted)
    }

    /// Increment the reference count of an allocated (in-use) block.
    ///
    /// # Panics
    ///
    /// Panics if `block` is out of range or not currently in-use.
    pub fn retain(&mut self, block: BlockId) {
        let meta = &mut self.metas[block as usize];
        assert!(meta.ref_count > 0, "retain on non-allocated block {block}");
        meta.ref_count = meta.ref_count.saturating_add(1);
    }

    /// Transition a cached block (ref\_count == 0) back to in-use.
    ///
    /// Called when a new sequence adopts blocks from the prefix cache.
    ///
    /// # Panics
    ///
    /// Panics if `block` is out of range or already in-use.
    pub fn reclaim(&mut self, block: BlockId) {
        let meta = &mut self.metas[block as usize];
        assert_eq!(meta.ref_count, 0, "reclaim on in-use block {block}");
        meta.ref_count = 1;
        self.cached_count = self.cached_count.saturating_sub(1);
    }

    /// Release a reference to a block.
    ///
    /// When `ref_count` reaches zero the block transitions to the **cached**
    /// state (it stays tracked until evicted). Pass the token-content `hash`
    /// so the prefix cache can match against it later.
    pub fn release(&mut self, block: BlockId, hash: u64) {
        let meta = &mut self.metas[block as usize];
        assert!(meta.ref_count > 0, "release on non-allocated block {block}");
        meta.ref_count = meta.ref_count.saturating_sub(1);
        if meta.ref_count == 0 {
            meta.hash = hash;
            self.cached_count += 1;
        }
    }

    /// Immediately return a *cached* (ref\_count == 0) block to the free list,
    /// undoing the cacheable state. Unlike [`KvBlockPool::release`], this makes
    /// the block reusable on the very next [`alloc`](KvBlockPool::alloc) without
    /// waiting for a clock-sweep — used by the scheduler's prefix-cache
    /// eviction path (`RadixPrefixCache::evict_filter`) to reclaim physical
    /// blocks the instant they leave the prefix tree.
    ///
    /// # Panics
    ///
    /// Panics if `block` is currently in-use (`ref_count > 0`).
    pub fn free_cached(&mut self, block: BlockId) {
        let meta = &mut self.metas[block as usize];
        assert_eq!(meta.ref_count, 0, "free_cached on in-use block {block}");
        meta.pinned = false;
        meta.hash = 0;
        self.cached_count = self.cached_count.saturating_sub(1);
        self.free.push(block);
    }

    /// Mark a block as pinned (never evicted by the clock-sweep).
    ///
    /// A pinned block is protected from eviction regardless of its reference
    /// count — this is how the scheduler keeps cached KV blocks (which are in
    /// the `cached` state, `ref_count == 0`) resident while they are still
    /// tracked by the prefix cache. Eviction of pinned blocks is driven
    /// explicitly by [`KvBlockPool::free_cached`].
    pub fn pin(&mut self, block: BlockId) {
        let meta = &mut self.metas[block as usize];
        meta.pinned = true;
    }

    /// Unpin a block.
    pub fn unpin(&mut self, block: BlockId) {
        self.metas[block as usize].pinned = false;
    }

    /// Returns `true` if the block is in-use by at least one reference.
    pub fn is_in_use(&self, block: BlockId) -> bool {
        self.metas[block as usize].ref_count > 0
    }

    /// Token-content hash stored for a cached block (0 if uncacheable/free).
    pub fn block_hash(&self, block: BlockId) -> u64 {
        self.metas[block as usize].hash
    }

    // -- eviction -----------------------------------------------------------

    /// Evict a single cacheable block via clock-sweep.
    ///
    /// Scans from `clock_hand` until it finds a block with `ref_count == 0`
    /// and `pinned == false`. That block is moved to the free list.
    fn evict_one(&mut self) -> Option<BlockId> {
        let n = self.metas.len();
        let start = self.clock_hand;
        for offset in 0..n {
            let idx = (start + offset) % n;
            self.clock_hand = (idx + 1) % n;
            let meta = &self.metas[idx];
            if meta.ref_count > 0 || meta.pinned {
                continue;
            }
            // Found a cacheable block — evict it.
            self.metas[idx] = BlockMeta::new();
            self.free.push(idx as BlockId);
            self.eviction_count += 1;
            self.allocated_count = self.allocated_count.saturating_sub(1);
            self.cached_count = self.cached_count.saturating_sub(1);
            return Some(idx as BlockId);
        }
        None
    }

    /// Try to evict `target` cacheable blocks (best-effort).
    pub fn evict(&mut self, target: usize) -> usize {
        let mut evicted = 0;
        for _ in 0..target {
            if self.evict_one().is_some() {
                evicted += 1;
            } else {
                break;
            }
        }
        evicted
    }
}

// ---------------------------------------------------------------------------
// Sequence
// ---------------------------------------------------------------------------

/// A generation session tracking its contiguous range of KV-cache blocks.
///
/// Each [`Sequence`] is created by the scheduler and lives for the duration of
/// a single request (or a chunk of a request in chunked prefill).
pub struct Sequence {
    id: u64,
    blocks: Vec<BlockId>,
    num_tokens: usize,
}

impl Sequence {
    /// Create a new empty sequence.
    pub fn new(id: u64) -> Self {
        Self { id, blocks: Vec::new(), num_tokens: 0 }
    }

    /// Opaque sequence identifier.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Allocated block IDs in order.
    pub fn blocks(&self) -> &[BlockId] {
        &self.blocks
    }

    /// Number of allocated blocks.
    pub fn num_blocks(&self) -> usize {
        self.blocks.len()
    }

    /// Number of tokens generated so far.
    pub fn num_tokens(&self) -> usize {
        self.num_tokens
    }

    /// Allocate one additional block and append it to this sequence.
    ///
    /// Returns the new block's ID, or `None` if the pool is exhausted.
    pub fn grow(&mut self, pool: &mut KvBlockPool) -> Option<BlockId> {
        let block = pool.alloc()?;
        self.blocks.push(block);
        Some(block)
    }

    /// Record that `n` tokens have been generated (advances the logical
    /// cursor without allocating new blocks).
    pub fn advance(&mut self, n: usize) {
        self.num_tokens += n;
    }

    /// Set the token count directly (used when restoring from a cached prefix).
    pub fn set_num_tokens(&mut self, n: usize) {
        self.num_tokens = n;
    }

    /// Restore blocks and token count from a prefix-cache hit.
    ///
    /// Calls [`KvBlockPool::reclaim`] for each block to transition it from
    /// cacheable to in-use.
    pub fn restore_from_cache(
        &mut self,
        blocks: &[BlockId],
        num_tokens: usize,
        pool: &mut KvBlockPool,
    ) {
        self.blocks = blocks.to_vec();
        self.num_tokens = num_tokens;
        for &b in &self.blocks {
            pool.reclaim(b);
        }
    }
}

// ---------------------------------------------------------------------------
// Prefix cache
// ---------------------------------------------------------------------------

/// FNV-1a hash of a token slice.
fn hash_tokens(tokens: &[u32]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &t in tokens {
        h ^= t as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Hash-based prefix cache for KV-block reuse.
///
/// The cache stores every *prefix* (first N blocks) of completed sequences.
/// When a new prompt arrives the cache is queried from longest to shortest
/// prefix until a match is found.
///
/// This uses the observation that LLM serving has high prefix overlap
/// (system prompt, chat history, few-shot examples). The hash-map approach
/// is simple, fast for lookup, and handles partial reuse correctly — the
/// downside is that it cannot match *arbitrary* shared substrings (only
/// aligned prefixes). A radix-tree upgrade is tracked for a future
/// optimization pass.
pub struct PrefixCache {
    /// Maps `prefix_hash(tokens, n_blocks)` → `Vec<BlockId>`.
    entries: HashMap<u64, Vec<BlockId>>,
    /// Maps `prefix_hash` → number of tokens the prefix covers.
    prefix_lengths: HashMap<u64, usize>,
    hits: std::cell::Cell<u64>,
    misses: std::cell::Cell<u64>,
}

impl PrefixCache {
    /// Create an empty prefix cache.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            prefix_lengths: HashMap::new(),
            hits: std::cell::Cell::new(0),
            misses: std::cell::Cell::new(0),
        }
    }

    /// Compute the rolling hash for the first `num_tokens` of `tokens`.
    fn prefix_hash(tokens: &[u32], num_tokens: usize) -> u64 {
        let end = tokens.len().min(num_tokens);
        hash_tokens(&tokens[..end])
    }

    /// Number of entries in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Total KV-block references stored across all prefix entries. Because each
    /// prefix entry stores the full block list for its length, a block that
    /// appears in many shared prefixes is counted once per entry — so this
    /// grows super-linearly for nested (chat-history-style) prompts. Compare
    /// with [`RadixPrefixCache::cached_tokens`], which stores each block once.
    pub fn total_stored_blocks(&self) -> usize {
        self.entries.values().map(|blocks| blocks.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Cache statistics.
    pub fn stats(&self) -> PrefixCacheStats {
        PrefixCacheStats {
            entries: self.entries.len(),
            hits: self.hits.get(),
            misses: self.misses.get(),
        }
    }

    /// Insert the blocks for a completed sequence into the cache.
    ///
    /// Every prefix (first 1 block, first 2 blocks, …, all blocks) is stored
    /// under its token-content hash.
    ///
    /// The caller **must** call [`KvBlockPool::release`] separately to
    /// transition the blocks to the cacheable state.
    pub fn insert(&mut self, tokens: &[u32], blocks: &[BlockId]) {
        for n in 1..=blocks.len() {
            let num_tokens = n * BLOCK_SIZE;
            let h = Self::prefix_hash(tokens, num_tokens);
            self.entries.entry(h).or_insert_with(|| {
                self.prefix_lengths.insert(h, num_tokens);
                blocks[..n].to_vec()
            });
        }
    }

    /// Look up the longest matching prefix.
    ///
    /// Returns `(num_tokens_matched, block_ids)` for the longest prefix that
    /// exists in the cache, or `None` if no prefix matches.
    pub fn lookup(&self, tokens: &[u32]) -> Option<(usize, &[BlockId])> {
        let max_blocks = tokens.len().div_ceil(BLOCK_SIZE);
        // Search from longest to shortest so we get the best match.
        for n in (1..=max_blocks).rev() {
            let num_tokens = n * BLOCK_SIZE;
            let h = Self::prefix_hash(tokens, num_tokens);
            if let Some(blocks) = self.entries.get(&h) {
                self.hits.set(self.hits.get() + 1);
                return Some((num_tokens, blocks.as_slice()));
            }
        }
        self.misses.set(self.misses.get() + 1);
        None
    }

    /// Remove an entry from the cache. Returns the blocks that were cached.
    ///
    /// Called by the eviction path in [`KvBlockPool`].
    pub fn remove(&mut self, hash: u64) -> Option<Vec<BlockId>> {
        self.prefix_lengths.remove(&hash);
        self.entries.remove(&hash)
    }
}

impl Default for PrefixCache {
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

    // -- KvBlockPool tests --------------------------------------------------

    #[test]
    fn pool_new_has_capacity() {
        let pool = KvBlockPool::new(64);
        assert_eq!(pool.capacity(), 64);
        assert_eq!(pool.available(), 64);
        assert_eq!(pool.allocated(), 0);
        assert!(pool.pressure_pct() < 1.0);
    }

    #[test]
    fn pool_alloc_and_release() {
        let mut pool = KvBlockPool::new(4);
        let b0 = pool.alloc().unwrap();
        let b1 = pool.alloc().unwrap();
        assert_ne!(b0, b1);
        assert_eq!(pool.allocated(), 2);
        assert_eq!(pool.available(), 2);
        pool.release(b0, 42);
        assert_eq!(pool.in_use(), 1);
        assert_eq!(pool.cached(), 1);
        pool.release(b1, 99);
        assert_eq!(pool.in_use(), 0);
        assert_eq!(pool.cached(), 2);
    }

    #[test]
    fn pool_retain_multiple_references() {
        let mut pool = KvBlockPool::new(4);
        let b = pool.alloc().unwrap();
        pool.retain(b); // ref_count: 1 → 2
        assert!(pool.is_in_use(b));
        pool.release(b, 0); // 2 → 1
        assert!(pool.is_in_use(b));
        pool.release(b, 0); // 1 → 0
        assert!(!pool.is_in_use(b));
        assert_eq!(pool.cached(), 1);
    }

    #[test]
    fn pool_exhaustion_triggers_eviction() {
        let mut pool = KvBlockPool::new(2);
        let b0 = pool.alloc().unwrap();
        let _b1 = pool.alloc().unwrap();
        assert!(pool.alloc().is_none()); // free list empty, nothing to evict

        // Release b0 to cache → now there is a cacheable block.
        pool.release(b0, 42);
        assert_eq!(pool.cached(), 1);

        // Next alloc should evict b0 (b1 is still in-use and thus skipped).
        let b2 = pool.alloc().unwrap();
        assert_eq!(b2, b0); // recycled
        assert_eq!(pool.stats().evictions, 1);
        assert_eq!(pool.in_use(), 2); // b1 + b2
    }

    #[test]
    fn pool_pinned_blocks_are_not_evicted() {
        let mut pool = KvBlockPool::new(2);
        let b0 = pool.alloc().unwrap();
        pool.pin(b0);
        let _b1 = pool.alloc().unwrap();
        pool.release(b0, 42); // b0 is cached, but pinned
        pool.release(_b1, 99); // b1 is cached, unpinned

        // Evict should skip b0 (pinned) and evict b1.
        let evicted = pool.evict(1);
        assert_eq!(evicted, 1);
        assert_eq!(pool.stats().evictions, 1);
        // b0 is still allocated (pinned).
        assert_eq!(pool.allocated(), 1);
    }

    #[test]
    fn pool_evict_up_to_target() {
        let mut pool = KvBlockPool::new(10);
        let blocks: Vec<_> = (0..10).map(|_| pool.alloc().unwrap()).collect();
        // Release all to cache.
        for (i, &b) in blocks.iter().enumerate() {
            pool.release(b, i as u64);
        }
        assert_eq!(pool.cached(), 10);

        let evicted = pool.evict(4);
        assert_eq!(evicted, 4);
        assert_eq!(pool.available(), 4);
        assert_eq!(pool.cached(), 6);
    }

    #[test]
    fn pool_stats_consistent() {
        let mut pool = KvBlockPool::new(8);
        for _ in 0..5 {
            pool.alloc().unwrap();
        }
        let s = pool.stats();
        assert_eq!(s.total, 8);
        assert_eq!(s.allocated, 5);
        assert_eq!(s.free, 3);
    }

    // -- PrefixCache tests --------------------------------------------------

    #[test]
    fn prefix_cache_insert_and_lookup_exact() {
        let mut cache = PrefixCache::new();
        let tokens: Vec<u32> = (0..48).collect(); // 3 blocks
        let blocks = vec![10, 20, 30];
        cache.insert(&tokens, &blocks);

        // Look up entire sequence.
        let (matched, cached) = cache.lookup(&tokens).unwrap();
        assert_eq!(matched, 48); // 3 blocks × 16
        assert_eq!(cached, &[10, 20, 30]);
    }

    #[test]
    fn prefix_cache_partial_match() {
        let mut cache = PrefixCache::new();
        let tokens: Vec<u32> = (0..32).collect(); // 2 blocks
        let blocks = vec![10, 20];
        cache.insert(&tokens, &blocks);

        // Same prefix but shorter.
        let short: Vec<u32> = (0..16).collect(); // 1 block
        let (matched, cached) = cache.lookup(&short).unwrap();
        assert_eq!(matched, 16);
        assert_eq!(cached, &[10]);

        // Longer prompt that extends the prefix.
        let longer: Vec<u32> = (0..40).collect(); // 3 blocks, first 2 match
        let (matched, cached) = cache.lookup(&longer).unwrap();
        assert_eq!(matched, 32);
        assert_eq!(cached, &[10, 20]);
    }

    #[test]
    fn prefix_cache_miss() {
        let mut cache = PrefixCache::new();
        cache.insert(&[0, 1, 2], &[1]);

        let result = cache.lookup(&[99, 100, 101]);
        assert!(result.is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn prefix_cache_empty_is_miss() {
        let cache = PrefixCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        let result = cache.lookup(&[1, 2, 3]);
        assert!(result.is_none());
    }

    #[test]
    fn prefix_cache_tracks_hits() {
        let mut cache = PrefixCache::new();
        let tokens: Vec<u32> = (0..16).collect();
        cache.insert(&tokens, &[7]);

        // First lookup → hit.
        let _ = cache.lookup(&tokens);
        assert_eq!(cache.stats().hits, 1);

        // Second lookup → hit.
        let _ = cache.lookup(&tokens);
        assert_eq!(cache.stats().hits, 2);

        // Miss.
        let _ = cache.lookup(&[99]);
        assert_eq!(cache.stats().misses, 1);
    }

    // -- Sequence tests -----------------------------------------------------

    #[test]
    fn sequence_grow_allocates_blocks() {
        let mut pool = KvBlockPool::new(10);
        let mut seq = Sequence::new(1);
        assert_eq!(seq.num_blocks(), 0);

        let b0 = seq.grow(&mut pool).unwrap();
        let b1 = seq.grow(&mut pool).unwrap();
        assert_eq!(seq.num_blocks(), 2);
        assert_eq!(seq.blocks(), &[b0, b1]);
        assert_eq!(pool.in_use(), 2);
    }

    #[test]
    fn sequence_advance_tracks_tokens() {
        let mut seq = Sequence::new(42);
        assert_eq!(seq.num_tokens(), 0);
        seq.advance(16);
        assert_eq!(seq.num_tokens(), 16);
        seq.advance(8);
        assert_eq!(seq.num_tokens(), 24);
    }

    #[test]
    fn sequence_grow_exhausted_pool_returns_none() {
        let mut pool = KvBlockPool::new(1);
        let mut seq = Sequence::new(1);
        assert!(seq.grow(&mut pool).is_some());
        assert!(seq.grow(&mut pool).is_none()); // no blocks to evict
    }

    // -- Integration: pool + prefix cache + sequence ------------------------

    #[test]
    fn end_to_end_prefix_caching() {
        let mut pool = KvBlockPool::new(16);
        let mut cache = PrefixCache::new();

        // First request: process tokens 0..48, allocating 3 blocks.
        let tokens: Vec<u32> = (0..48).collect();
        let mut seq = Sequence::new(1);
        let blocks: Vec<BlockId> = (0..3).map(|_| seq.grow(&mut pool).unwrap()).collect();

        // Insert into prefix cache and release blocks.
        cache.insert(&tokens, &blocks);
        for &b in &blocks {
            pool.release(b, hash_tokens(&tokens[..16])); // simplified
        }
        assert_eq!(pool.cached(), 3);

        // Second request with same prefix: cache hit.
        let result = cache.lookup(&tokens);
        assert!(result.is_some());
        let (_, cached_blocks) = result.unwrap();
        assert_eq!(cached_blocks, &blocks[..]);

        // The new sequence can skip the first 3 blocks.
        let mut seq2 = Sequence::new(2);
        seq2.set_num_tokens(48); // restored from cache
        assert_eq!(seq2.num_tokens(), 48);
    }
}
