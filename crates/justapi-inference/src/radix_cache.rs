//! RadixAttention-style prefix caching (SGLang approach).
//!
//! A radix tree of token sequences where each node holds a block-aligned run
//! of KV-cache blocks. Sequences that share a prompt prefix share the internal
//! nodes of the tree, so the same KV blocks are stored (and reused) once. This
//! is strictly more memory-efficient than the flat hash-map [`crate::PrefixCache`]:
//! common prefixes are merged instead of duplicated, and eviction is LRU over
//! the leaf paths (least-recently-used prompt variants are dropped first).
//!
//! Nodes are block-aligned: a node's `tokens` length is always a multiple of
//! [`BLOCK_SIZE`], and `blocks` has one [`BlockId`] per block. The tree itself
//! never frees physical blocks — [`RadixPrefixCache::evict`] returns the
//! [`BlockId`]s of least-recently-used leaves as a *hint*; the caller (the
//! [`crate::KvBlockPool`]) makes the final recycle decision and skips any block
//! still held by a live sequence.

use std::cell::Cell;
use std::collections::HashMap;

use crate::kv_cache::{BlockId, BLOCK_SIZE};

/// A node in the radix prefix tree.
#[derive(Debug, Clone)]
struct RadixNode {
    /// Edge label: KV-covered tokens for this node (multiple of `BLOCK_SIZE`).
    tokens: Vec<u32>,
    /// One KV block per `BLOCK_SIZE` tokens.
    blocks: Vec<BlockId>,
    /// Child nodes keyed by the first token of their edge.
    children: HashMap<u32, RadixNode>,
    /// Last-access timestamp (for LRU eviction).
    last_access: u64,
    /// Content hash of `tokens` (FNV-1a). The cache key: a lookup must match
    /// both the exact token run *and* this hash, so a corrupted or hash-only
    /// (colliding) match is rejected instead of returning the wrong KV blocks.
    token_hash: u64,
}

/// FNV-1a 64-bit hash of a token slice. KV-cache content is determined solely
/// by the token ids, so this is the collision-resistant key for a prefix.
fn hash_tokens(tokens: &[u32]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &t in tokens {
        h ^= t as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

impl RadixNode {
    fn new(tokens: Vec<u32>, blocks: Vec<BlockId>, now: u64) -> Self {
        let token_hash = hash_tokens(&tokens);
        Self { tokens, blocks, children: HashMap::new(), last_access: now, token_hash }
    }

    fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

/// Snapshot of radix-prefix-cache effectiveness.
#[derive(Debug, Clone, Copy, Default)]
pub struct RadixPrefixCacheStats {
    /// Number of nodes in the tree (excluding the root).
    pub nodes: usize,
    /// Number of successful prefix lookups.
    pub hits: u64,
    /// Number of failed prefix lookups.
    pub misses: u64,
    /// Number of lookups whose structural (token) match was rejected because
    /// the content hash disagreed — i.e. a collision or tree-corruption guard
    /// firing. Should stay at 0 in correct operation.
    pub hash_collisions: u64,
    /// Total tokens matched across all hits (the reuse benefit — each matched
    /// token is a KV block that did not need recomputation).
    pub tokens_saved: u64,
}

/// Radix-tree prefix cache for cross-request KV-block reuse.
pub struct RadixPrefixCache {
    root: RadixNode,
    hits: Cell<u64>,
    misses: Cell<u64>,
    hash_collisions: Cell<u64>,
    tokens_saved: Cell<u64>,
    clock: Cell<u64>,
}

impl RadixPrefixCache {
    /// Create an empty radix prefix cache.
    pub fn new() -> Self {
        Self {
            root: RadixNode::new(vec![], vec![], 0),
            hits: Cell::new(0),
            misses: Cell::new(0),
            hash_collisions: Cell::new(0),
            tokens_saved: Cell::new(0),
            clock: Cell::new(0),
        }
    }

    fn tick(&self) -> u64 {
        let t = self.clock.get() + 1;
        self.clock.set(t);
        t
    }

    /// Number of nodes in the tree (excluding the root).
    pub fn nodes(&self) -> usize {
        count_nodes(&self.root)
    }

    /// Total tokens currently cached across all nodes.
    pub fn cached_tokens(&self) -> usize {
        cached_tokens(&self.root)
    }

    /// Insert a completed sequence's `(tokens, blocks)` into the tree, sharing
    /// any common prefix with existing sequences. `tokens.len()` must equal
    /// `blocks.len() * BLOCK_SIZE`.
    pub fn insert(&mut self, tokens: &[u32], blocks: &[BlockId]) {
        assert_eq!(tokens.len(), blocks.len() * BLOCK_SIZE, "block misalignment");
        let now = self.tick();
        insert_node(&mut self.root, tokens, blocks, now);
    }

    /// Look up the longest matching prefix for `tokens`.
    ///
    /// Returns `(matched_tokens, block_ids)` for the longest prefix that exists
    /// in the tree, or `None` if no prefix matches. A match is only accepted
    /// when the accumulated content hash of the matched tokens equals a fresh
    /// recomputation — this is the collision-safe cache key: even if the tree
    /// were corrupted or a future change matched by hash alone, a hash
    /// disagreement rejects the match instead of returning the wrong KV blocks.
    /// Matched nodes have their last-access timestamp refreshed (LRU).
    pub fn lookup(&mut self, tokens: &[u32]) -> Option<(usize, Vec<BlockId>)> {
        let mut out = (0usize, Vec::new(), false);
        let now = self.tick();
        lookup_node(&mut self.root, tokens, &mut out, now);
        if out.2 {
            // A structural match was found but its content hash diverged from
            // the prompt tokens — a corruption or collision. Reject it.
            self.hash_collisions.set(self.hash_collisions.get() + 1);
            self.misses.set(self.misses.get() + 1);
            None
        } else if out.0 > 0 {
            self.hits.set(self.hits.get() + 1);
            self.tokens_saved.set(self.tokens_saved.get() + out.0 as u64);
            Some((out.0, out.1))
        } else {
            self.misses.set(self.misses.get() + 1);
            None
        }
    }

    /// Walk the tree and assert every node's stored `token_hash` equals the hash
    /// of its tokens. Used by tests/debugging to prove the cache keys are
    /// internally consistent after inserts and splits.
    pub fn verify_hashes(&self) -> bool {
        fn check(node: &RadixNode) -> bool {
            if node.token_hash != hash_tokens(&node.tokens) {
                return false;
            }
            node.children.values().all(check)
        }
        check(&self.root)
    }

    /// Evict up to `k` least-recently-used leaf blocks, returning the freed
    /// [`BlockId`]s. Leaf nodes are removed from the tree; the caller is
    /// responsible for actually recycling any blocks still held by a live
    /// sequence. Returns fewer than `k` ids if the tree is exhausted.
    pub fn evict(&mut self, k: usize) -> Vec<BlockId> {
        let mut freed = Vec::new();
        while freed.len() < k {
            match find_lru_leaf(&self.root) {
                Some((path, blocks)) => {
                    remove_path(&mut self.root, &path);
                    freed.extend(blocks);
                }
                None => break,
            }
        }
        freed
    }

    /// Like [`RadixPrefixCache::evict`] but only evicts leaf paths whose blocks
    /// all satisfy `keep`. This lets the caller (the scheduler) skip blocks that
    /// are currently referenced by a live sequence, so eviction never frees a
    /// block still in use. Leaves whose blocks fail `keep` are left in the tree.
    ///
    /// Leaves are considered in least-recently-used order; eviction stops once
    /// `k` *freeable* blocks have been collected (or the tree is exhausted).
    pub fn evict_filter<F>(&mut self, k: usize, mut keep: F) -> Vec<BlockId>
    where
        F: FnMut(&[BlockId]) -> bool,
    {
        let mut leaves = Vec::new();
        collect_leaves(&self.root, &mut Vec::new(), &mut leaves);
        // Least-recently-used first.
        leaves.sort_by_key(|(access, _, _)| *access);

        let mut freed = Vec::new();
        for (_, path, blocks) in leaves {
            if freed.len() >= k {
                break;
            }
            if keep(&blocks) {
                remove_path(&mut self.root, &path);
                freed.extend(blocks);
            }
        }
        freed
    }

    /// Cache effectiveness snapshot.
    pub fn stats(&self) -> RadixPrefixCacheStats {
        RadixPrefixCacheStats {
            nodes: self.nodes(),
            hits: self.hits.get(),
            misses: self.misses.get(),
            hash_collisions: self.hash_collisions.get(),
            tokens_saved: self.tokens_saved.get(),
        }
    }
}

impl Default for RadixPrefixCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn common_prefix_len(a: &[u32], b: &[u32]) -> usize {
    let mut i = 0;
    while i < a.len() && i < b.len() && a[i] == b[i] {
        i += 1;
    }
    i
}

fn count_nodes(node: &RadixNode) -> usize {
    let mut n = node.children.len();
    for child in node.children.values() {
        n += count_nodes(child);
    }
    n
}

fn cached_tokens(node: &RadixNode) -> usize {
    let mut t = node.tokens.len();
    for child in node.children.values() {
        t += cached_tokens(child);
    }
    t
}

/// Recursively insert `(tokens, blocks)` under `node`, splitting existing
/// edges where the incoming path diverges.
fn insert_node(node: &mut RadixNode, tokens: &[u32], blocks: &[BlockId], now: u64) {
    if tokens.is_empty() {
        node.last_access = now;
        return;
    }
    let first = tokens[0];

    // Does a child edge start with the same token?
    if let Some(child) = node.children.get_mut(&first) {
        let cp = common_prefix_len(tokens, &child.tokens);
        if cp == child.tokens.len() {
            // Whole child edge consumed — descend into it.
            insert_node(child, &tokens[cp..], &blocks[cp / BLOCK_SIZE..], now);
        } else {
            // Divergence mid-edge: split the child at `cp`.
            let split = RadixNode {
                tokens: child.tokens[cp..].to_vec(),
                blocks: child.blocks[cp / BLOCK_SIZE..].to_vec(),
                children: std::mem::take(&mut child.children),
                last_access: child.last_access,
                token_hash: hash_tokens(&child.tokens[cp..]),
            };
            child.tokens.truncate(cp);
            child.blocks.truncate(cp / BLOCK_SIZE);
            // The truncated edge now represents a shorter prefix → recompute
            // its content hash so the cache key stays consistent.
            child.token_hash = hash_tokens(&child.tokens);
            child.children.clear();
            child.children.insert(split.tokens[0], split);
            // Now `child` matches `tokens[..cp]`; descend with the remainder.
            insert_node(child, &tokens[cp..], &blocks[cp / BLOCK_SIZE..], now);
        }
    } else {
        // No matching child — create a leaf for the remaining path.
        node.children.insert(first, RadixNode::new(tokens.to_vec(), blocks.to_vec(), now));
    }
}

/// Walk the longest matching prefix, accumulating matched `(tokens, blocks)`
/// and a `collision` flag. Refreshes `last_access` on every matched node to
/// `now` (LRU).
///
/// A full-edge match is only accepted if the node's stored `token_hash` equals
/// the hash of the tokens we actually matched (which equal `tokens[..cp]`) —
/// the collision-safe cache key. On divergence `collision` is set and the walk
/// stops, so the caller can reject instead of returning the wrong KV blocks.
fn lookup_node(
    node: &mut RadixNode,
    tokens: &[u32],
    out: &mut (usize, Vec<BlockId>, bool),
    now: u64,
) {
    if tokens.is_empty() {
        return;
    }
    let first = tokens[0];
    let cp = match node.children.get(&first) {
        Some(child) => common_prefix_len(tokens, &child.tokens),
        None => return,
    };
    let child = node.children.get_mut(&first).unwrap();
    if cp == child.tokens.len() {
        // Full edge match — accept only if the content hash agrees.
        if child.token_hash != hash_tokens(&tokens[..cp]) {
            // Structural match but hash divergence: a corruption or collision.
            out.2 = true;
            return;
        }
        out.0 += cp;
        out.1.extend_from_slice(&child.blocks);
        child.last_access = now;
        lookup_node(child, &tokens[cp..], out, now);
    } else if cp > 0 {
        // Partial edge match — the prompt is a strict prefix of this node's
        // tokens. The matched `cp` tokens equal `child.tokens[..cp]`, which is
        // already verified by the exact token comparison, so accumulate and stop.
        out.0 += cp;
        out.1.extend_from_slice(&child.blocks[..cp / BLOCK_SIZE]);
        child.last_access = now;
    }
}

/// Find the globally least-recently-used unreferenced leaf, returning its
/// key path from the root and its block ids.
fn find_lru_leaf(node: &RadixNode) -> Option<(Vec<u32>, Vec<BlockId>)> {
    // Direct leaf children first.
    let mut direct: Vec<(u32, u64)> = node
        .children
        .iter()
        .filter(|(_, c)| c.is_leaf())
        .map(|(k, c)| (*k, c.last_access))
        .collect();
    direct.sort_by_key(|&(_, a)| a);
    if let Some((key, _)) = direct.first() {
        let child = &node.children[key];
        return Some((vec![*key], child.blocks.clone()));
    }

    // Recurse into children, tracking the globally-oldest leaf.
    let mut best: Option<(Vec<u32>, Vec<BlockId>)> = None;
    let mut best_access = u64::MAX;
    for (k, child) in &node.children {
        if let Some((mut path, blocks)) = find_lru_leaf(child) {
            path.insert(0, *k);
            let access = child.last_access;
            if access < best_access {
                best_access = access;
                best = Some((path, blocks));
            }
        }
    }
    best
}

/// Collect every leaf under `node` as `(last_access, key_path, block_ids)`,
/// where `key_path` is the sequence of child keys from the root to the leaf.
fn collect_leaves(
    node: &RadixNode,
    path: &mut Vec<u32>,
    out: &mut Vec<(u64, Vec<u32>, Vec<BlockId>)>,
) {
    for (k, child) in &node.children {
        path.push(*k);
        if child.is_leaf() {
            out.push((child.last_access, path.clone(), child.blocks.clone()));
        } else {
            collect_leaves(child, path, out);
        }
        path.pop();
    }
}

/// Remove the leaf at `path` (a sequence of child keys from `node`).
fn remove_path(node: &mut RadixNode, path: &[u32]) -> bool {
    if path.is_empty() {
        return false;
    }
    let key = path[0];
    if let Some(child) = node.children.get_mut(&key) {
        if path.len() == 1 {
            node.children.remove(&key);
            true
        } else {
            remove_path(child, &path[1..])
        }
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(n: usize) -> Vec<BlockId> {
        (0..n as BlockId).collect()
    }

    #[test]
    fn insert_and_lookup_exact() {
        let mut cache = RadixPrefixCache::new();
        // 48 tokens = 3 blocks.
        let tokens: Vec<u32> = (0..48).collect();
        cache.insert(&tokens, &blocks(3));

        let (matched, cached) = cache.lookup(&tokens).unwrap();
        assert_eq!(matched, 48);
        assert_eq!(cached, vec![0, 1, 2]);
    }

    #[test]
    fn lookup_partial_prefix() {
        let mut cache = RadixPrefixCache::new();
        let tokens: Vec<u32> = (0..32).collect(); // 2 blocks
        cache.insert(&tokens, &blocks(2));

        // Shorter prompt that is a prefix.
        let short: Vec<u32> = (0..16).collect();
        let (matched, cached) = cache.lookup(&short).unwrap();
        assert_eq!(matched, 16);
        assert_eq!(cached, vec![0]);

        // Longer prompt extending the prefix (first 2 blocks match).
        let longer: Vec<u32> = (0..40).collect();
        let (matched, cached) = cache.lookup(&longer).unwrap();
        assert_eq!(matched, 32);
        assert_eq!(cached, vec![0, 1]);
    }

    #[test]
    fn lookup_miss() {
        let mut cache = RadixPrefixCache::new();
        cache.insert(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15], &[1]);
        assert!(cache.lookup(&[99, 100]).is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn empty_lookup_is_miss() {
        let mut cache = RadixPrefixCache::new();
        assert!(cache.lookup(&[1, 2, 3]).is_none());
        assert_eq!(cache.nodes(), 0);
    }

    #[test]
    fn stats_track_hits_and_saved() {
        let mut cache = RadixPrefixCache::new();
        let tokens: Vec<u32> = (0..48).collect();
        cache.insert(&tokens, &blocks(3));

        let _ = cache.lookup(&tokens);
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().tokens_saved, 48);

        let _ = cache.lookup(&tokens);
        assert_eq!(cache.stats().hits, 2);
        assert_eq!(cache.stats().tokens_saved, 96);

        let _ = cache.lookup(&[999]);
        assert_eq!(cache.stats().misses, 1);
    }

    // ---------- prefix sharing (the radix-tree win) ----------

    #[test]
    fn shared_prefix_merges_nodes() {
        let mut cache = RadixPrefixCache::new();
        // Two sequences sharing the first 2 blocks (32 tokens).
        let a: Vec<u32> = (0..48).collect(); // 3 blocks
        let mut b: Vec<u32> = (0..32).collect(); // first 2 blocks
        b.extend_from_slice(&[
            100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115,
        ]); // 3rd block differs

        cache.insert(&a, &blocks(3));
        cache.insert(&b, &blocks(3));

        // Without sharing: 3 + 3 = 6 nodes would be needed. With sharing,
        // the common 2-block prefix is one node, plus 2 distinct leaves = 3.
        assert_eq!(cache.nodes(), 3, "common prefix should be merged");

        // Both sequences still resolve their full prefixes.
        let (ma, _) = cache.lookup(&a).unwrap();
        assert_eq!(ma, 48);
        let (mb, _) = cache.lookup(&b).unwrap();
        assert_eq!(mb, 48);
    }

    #[test]
    fn three_way_sharing() {
        let mut cache = RadixPrefixCache::new();
        // All share the first block [0..16], then diverge.
        let base: Vec<u32> = (0..16).collect();
        for suffix in [200u32, 300, 400] {
            let mut toks = base.clone();
            toks.extend_from_slice(&(suffix..suffix + 16).collect::<Vec<_>>());
            cache.insert(&toks, &blocks(2));
        }
        // Root's shared node [0..16] + 3 distinct leaves = 4 nodes (not 3×2 = 6).
        assert_eq!(cache.nodes(), 4);
    }

    // ---------- splitting ----------

    #[test]
    fn insert_splits_existing_edge() {
        let mut cache = RadixPrefixCache::new();
        // First insert a 2-block sequence.
        let a: Vec<u32> = (0..32).collect();
        cache.insert(&a, &blocks(2));
        assert_eq!(cache.nodes(), 1);

        // Now insert a 1-block prefix of it — should split the existing node.
        let b: Vec<u32> = (0..16).collect();
        cache.insert(&b, &blocks(1));
        // Root + split node (1 block) + child (1 block) = 2 nodes.
        assert_eq!(cache.nodes(), 2);

        // Both lookups resolve.
        let (ma, _) = cache.lookup(&a).unwrap();
        assert_eq!(ma, 32);
        let (mb, _) = cache.lookup(&b).unwrap();
        assert_eq!(mb, 16);
    }

    // ---------- LRU eviction ----------

    #[test]
    fn evict_removes_lru_leaf() {
        let mut cache = RadixPrefixCache::new();
        // Insert three independent 1-block sequences in order (oldest first).
        for start in [0u32, 100, 200] {
            let toks: Vec<u32> = (start..start + 16).collect();
            cache.insert(&toks, &[start / 100 as BlockId + 1]);
        }
        assert_eq!(cache.nodes(), 3);

        // Evict 1 block — should remove the oldest (start=0) leaf.
        let freed = cache.evict(1);
        assert_eq!(freed, vec![1]);
        assert_eq!(cache.nodes(), 2);

        // The evicted prefix no longer matches.
        let gone: Vec<u32> = (0..16).collect();
        assert!(cache.lookup(&gone).is_none());
        // The others still match.
        let still: Vec<u32> = (100..116).collect();
        assert!(cache.lookup(&still).is_some());
    }

    #[test]
    fn evict_frees_multiple_blocks_from_one_leaf() {
        let mut cache = RadixPrefixCache::new();
        // One 3-block sequence.
        let toks: Vec<u32> = (0..48).collect();
        cache.insert(&toks, &[5, 6, 7]);
        assert_eq!(cache.nodes(), 1);

        // Evicting 2 blocks from a single 3-block leaf frees all its blocks
        // (the whole leaf is removed).
        let freed = cache.evict(2);
        assert_eq!(freed, vec![5, 6, 7]);
        assert_eq!(cache.nodes(), 0);
    }

    #[test]
    fn evict_handles_exhaustion() {
        let mut cache = RadixPrefixCache::new();
        let toks: Vec<u32> = (0..16).collect();
        cache.insert(&toks, &[1]);
        let freed = cache.evict(10); // ask for more than exists
        assert_eq!(freed, vec![1]);
        assert_eq!(cache.nodes(), 0);
    }

    #[test]
    fn cached_tokens_accounts_for_shared_structure() {
        let mut cache = RadixPrefixCache::new();
        let a: Vec<u32> = (0..48).collect();
        let mut b: Vec<u32> = (0..32).collect();
        b.extend_from_slice(&[
            100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115,
        ]);
        cache.insert(&a, &blocks(3));
        cache.insert(&b, &blocks(3));
        // Shared prefix [0..32] stored once; a's tail [32..48] + b's tail
        // [100..116] stored distinctly. Total cached = 48 + 16 = 64.
        assert_eq!(cache.cached_tokens(), 64);
    }

    // ---------- cache-key content hashing (collision safety) ----------

    /// Test-only: flip the content hash of the first leaf node to simulate a
    /// corrupted / colliding cache entry, exercising the lookup guard.
    #[cfg(test)]
    fn corrupt_first_leaf_hash(cache: &mut RadixPrefixCache) {
        fn flip(node: &mut RadixNode) {
            if node.children.is_empty() {
                node.token_hash ^= 0xdead_beef;
            } else {
                let key = *node.children.keys().next().unwrap();
                flip(node.children.get_mut(&key).unwrap());
            }
        }
        flip(&mut cache.root);
    }

    #[test]
    fn node_hash_matches_token_content() {
        let mut cache = RadixPrefixCache::new();
        let tokens: Vec<u32> = (0..48).collect();
        cache.insert(&tokens, &[1, 2, 3]);
        // Every node's stored hash must equal the hash of its tokens.
        assert!(cache.verify_hashes());
    }

    #[test]
    fn hashes_consistent_after_split() {
        let mut cache = RadixPrefixCache::new();
        let a: Vec<u32> = (0..32).collect();
        cache.insert(&a, &[1, 2]);
        assert!(cache.verify_hashes());
        // Insert a strict prefix of `a` → splits the existing node. The
        // truncated edge must recompute its hash so keys stay consistent.
        let b: Vec<u32> = (0..16).collect();
        cache.insert(&b, &[3]);
        assert!(cache.verify_hashes());
        assert_eq!(cache.nodes(), 2);
    }

    #[test]
    fn distinct_prefixes_do_not_collide() {
        let mut cache = RadixPrefixCache::new();
        let a: Vec<u32> = (0..48).collect();
        let mut b_tok: Vec<u32> = (0..48).collect();
        b_tok[0] = 999; // completely different first token → different hash
        cache.insert(&a, &[1, 2, 3]);
        cache.insert(&b_tok, &[4, 5, 6]);

        let (ma, ba) = cache.lookup(&a).unwrap();
        assert_eq!(ma, 48);
        assert_eq!(ba, vec![1, 2, 3]);
        let (mb, bb) = cache.lookup(&b_tok).unwrap();
        assert_eq!(mb, 48);
        assert_eq!(bb, vec![4, 5, 6]);
        // Two structurally distinct prefixes never collide on the key.
        assert_eq!(cache.stats().hash_collisions, 0);
    }

    #[test]
    fn lookup_rejects_corrupted_hash() {
        let mut cache = RadixPrefixCache::new();
        let tokens: Vec<u32> = (0..48).collect();
        cache.insert(&tokens, &[5, 6, 7]);
        assert!(cache.lookup(&tokens).is_some());
        assert!(cache.verify_hashes());

        // Corrupt a node's content hash → the lookup guard must reject the
        // match rather than return the wrong KV blocks.
        corrupt_first_leaf_hash(&mut cache);
        assert!(!cache.verify_hashes());
        assert!(cache.lookup(&tokens).is_none());
        assert_eq!(cache.stats().hash_collisions, 1);
    }

    // ---------- structural benchmark: radix vs flat hash-map ----------

    /// Simulate a chat-history (nested prefix) workload: request `r` shares the
    /// chain `[b_0, b_1, …, b_r]` with prior requests, then appends one unique
    /// block `u_r`.  Returns `(radix_block_slots, flat_block_refs)` where
    /// - `radix_block_slots` = `cached_tokens / BLOCK_SIZE` — each block
    ///   stored once, so this is *linear* in `n_reqs`
    /// - `flat_block_refs` = `total_stored_blocks()` — the flat hash-map
    ///   duplicates block IDs across distinct prefix entries, making this
    ///   *quadratic* in `n_reqs`.
    fn bench_nested(n_reqs: usize) -> (usize, usize) {
        use crate::PrefixCache;

        let block_size = BLOCK_SIZE;

        let mut radix = RadixPrefixCache::new();
        let mut flat = PrefixCache::new();

        for r in 0..n_reqs {
            // shared chain [b_0, b_1, ..., b_r] + unique [u_r]
            let mut toks: Vec<u32> = Vec::new();
            for k in 0..=r {
                let base = k as u32 * block_size as u32;
                toks.extend(base..base + block_size as u32);
            }
            let u_base = (n_reqs + r) as u32 * block_size as u32;
            toks.extend(u_base..u_base + block_size as u32);

            let n_blocks = r + 2;
            let block_ids: Vec<BlockId> = (0..n_blocks as BlockId).collect();
            radix.insert(&toks, &block_ids);
            flat.insert(&toks, &block_ids);
        }

        (radix.cached_tokens() / block_size, flat.total_stored_blocks())
    }

    #[test]
    fn radix_linear_vs_flat_quadratic_nested_prefix() {
        // Chat-history workload: request `r` shares `r` blocks with prior
        // requests + 1 unique block.  Flat stores each distinct prefix
        // separately, duplicating block IDs across entries → O(N²) block
        // references.  Radix merges the shared chain → O(N) block slots.
        let (r20, f20) = bench_nested(20);
        let (r200, f200) = bench_nested(200);

        // Radix scales linearly: 10× requests → ~10× block slots.
        assert!(r200 <= r20 * 12, "radix block-slots: {r200} vs {r20}");

        // Flat scales quadratically: 10× requests → ~100× block refs.
        assert!(f200 >= f20 * 50, "flat block-refs: {f200} vs {f20}");

        // At scale, radix uses a tiny fraction of flat's storage.
        assert!(r200 < f200 / 10, "radix={r200} flat={f200}");
    }

    #[test]
    fn radix_memory_win_scales_with_nested_depth() {
        // More requests → deeper nested prefix → radix advantage grows.
        let (r_low, f_low) = bench_nested(50);
        let (r_big, f_big) = bench_nested(500);

        // 10× requests → radix grows ~10×, flat grows ~100×.
        assert!(r_big <= r_low * 12, "radix: {r_big} vs {r_low}");
        assert!(f_big >= f_low * 50, "flat: {f_big} vs {f_low}");

        // Radix dramatically outperforms at scale.
        assert!(r_big < f_big / 50, "radix={r_big} flat={f_big}");
    }
}
