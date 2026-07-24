use std::sync::Mutex;

const DEFAULT_ARENA_SIZE: usize = 16 * 1024; // 16KB per request arena

/// A bump allocator for request-scoped data.
/// All allocations live until the arena is reset.
/// No individual deallocations — the entire arena is cleared at once.
pub struct RequestArena {
    buffer: Box<[u8]>,
    cursor: usize,
}

impl RequestArena {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_ARENA_SIZE)
    }

    pub fn with_capacity(cap: usize) -> Self {
        let buffer = vec![0u8; cap].into_boxed_slice();
        Self { buffer, cursor: 0 }
    }

    /// Allocate a byte slice from the arena. Returns `None` if out of space.
    pub fn alloc(&mut self, bytes: &[u8]) -> Option<&[u8]> {
        let len = bytes.len();
        let start = self.cursor;
        let end = start + len;
        if end > self.buffer.len() {
            return None;
        }
        self.buffer[start..end].copy_from_slice(bytes);
        self.cursor = end;
        Some(&self.buffer[start..end])
    }

    /// Allocate a string slice from the arena.
    pub fn alloc_str(&mut self, s: &str) -> Option<&str> {
        self.alloc(s.as_bytes()).map(|b| unsafe {
            // SAFETY: `b` is a byte-slice copy of `s.as_bytes()` which is
            // guaranteed valid UTF-8 by virtue of `s: &str`.  The arena
            // cannot modify the bytes, so `from_utf8_unchecked` is sound.
            std::str::from_utf8_unchecked(b)
        })
    }

    /// Reset the arena, making all allocated memory available again.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    pub fn used(&self) -> usize {
        self.cursor
    }

    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    pub fn available(&self) -> usize {
        self.buffer.len() - self.cursor
    }
}

impl Default for RequestArena {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe arena wrapper using std::sync::Mutex.
/// Each connection task owns one of these; the mutex ensures safety
/// even when the arena is temporarily shared across await points.
pub struct SharedArena {
    inner: Mutex<RequestArena>,
}

impl SharedArena {
    pub fn new() -> Self {
        Self { inner: Mutex::new(RequestArena::new()) }
    }

    pub fn reset(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).reset();
    }

    pub fn alloc_str(&self, s: &str) -> Option<String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.alloc_str(s).map(|r| r.to_string())
    }
}

impl Default for SharedArena {
    fn default() -> Self {
        Self::new()
    }
}

/// A pool of reusable byte buffers for response bodies.
/// Reduces allocation overhead by recycling buffers instead of
/// allocating new ones for every response.
pub struct BufferPool {
    buckets: [Mutex<Vec<Vec<u8>>>; 4],
}

const BUCKET_SIZES: [usize; 4] = [1024, 4096, 16384, 65536];

impl BufferPool {
    pub fn new() -> Self {
        Self { buckets: std::array::from_fn(|_| Mutex::new(Vec::new())) }
    }

    fn bucket_index(min_size: usize) -> usize {
        for (i, &size) in BUCKET_SIZES.iter().enumerate() {
            if min_size <= size {
                return i;
            }
        }
        BUCKET_SIZES.len() - 1
    }

    /// Acquire a buffer with at least `min_size` capacity.
    pub fn acquire(&self, min_size: usize) -> Vec<u8> {
        let idx = Self::bucket_index(min_size);
        let mut pool = self.buckets[idx].lock().unwrap_or_else(|e| e.into_inner());
        pool.pop().unwrap_or_else(|| Vec::with_capacity(BUCKET_SIZES[idx]))
    }

    /// Return a buffer to the pool for reuse.
    pub fn release(&self, mut buf: Vec<u8>) {
        buf.clear();
        let idx = Self::bucket_index(buf.capacity());
        let mut pool = self.buckets[idx].lock().unwrap_or_else(|e| e.into_inner());
        if pool.len() < 128 {
            pool.push(buf);
        }
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_alloc() {
        let mut arena = RequestArena::new();
        let data = arena.alloc(b"hello").unwrap();
        assert_eq!(data, b"hello");
        assert_eq!(arena.used(), 5);
    }

    #[test]
    fn test_arena_str() {
        let mut arena = RequestArena::new();
        let s = arena.alloc_str("hello world").unwrap();
        assert_eq!(s, "hello world");
    }

    #[cfg(miri)]
    #[test]
    fn miri_unsafe_utf8_validity() {
        // This test exercises the only `unsafe` block in the production
        // code path (`from_utf8_unchecked`).  Under miri stacked-borrows
        // the provenance rules are enforced: the arena buffer's borrow
        // must remain alive and the bytes must remain valid UTF-8.
        let mut arena = RequestArena::new();
        let s = arena.alloc_str("Hello, Miri!").unwrap();
        assert_eq!(s, "Hello, Miri!");
        // End the borrow before the next alloc to avoid aliasing UB.
        let _ = s;
        // Subsequent allocations must also produce valid UTF-8.
        let s2 = arena.alloc_str("Another string").unwrap();
        assert_eq!(s2, "Another string");
    }

    #[cfg(miri)]
    #[test]
    fn miri_alloc_exact_sized() {
        // Allocate at each power-of-two boundary up to arena capacity
        // to exercise the bump-allocator offset arithmetic under miri.
        let mut arena = RequestArena::new();
        let mut total = 0;
        for i in 0..10 {
            let size = 1usize << i;
            if total + size > arena.capacity() {
                break;
            }
            let data = arena.alloc(&vec![0xABu8; size]);
            assert!(data.is_some());
            let data = data.unwrap();
            assert_eq!(data.len(), size);
            assert!(data.iter().all(|&b| b == 0xAB));
            total += size;
        }
    }

    #[test]
    fn test_arena_reset() {
        let mut arena = RequestArena::new();
        arena.alloc(b"hello").unwrap();
        assert_eq!(arena.used(), 5);
        arena.reset();
        assert_eq!(arena.used(), 0);
        let data = arena.alloc(b"world").unwrap();
        assert_eq!(data, b"world");
    }

    #[test]
    fn test_arena_exhaustion_returns_none() {
        let mut arena = RequestArena::with_capacity(10);
        assert!(arena.alloc(b"1234567890").is_some());
        assert!(arena.alloc(b"x").is_none());
    }

    #[test]
    fn test_buffer_pool_acquire_release() {
        let pool = BufferPool::new();
        let buf = pool.acquire(100);
        assert!(buf.capacity() >= 100);
        pool.release(buf);
        let buf2 = pool.acquire(100);
        assert!(buf2.capacity() >= 100);
        assert!(buf2.is_empty());
    }

    #[test]
    fn test_buffer_pool_bucket_sizing() {
        let pool = BufferPool::new();
        let small = pool.acquire(50);
        assert_eq!(small.capacity(), 1024);
        let large = pool.acquire(20000);
        assert_eq!(large.capacity(), 65536);
    }

    #[test]
    fn test_shared_arena() {
        let arena = SharedArena::new();
        arena.reset();
        let s = arena.alloc_str("/hello").unwrap();
        assert_eq!(s, "/hello");
    }
}
