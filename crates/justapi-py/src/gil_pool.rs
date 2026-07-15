//! Dedicated GIL thread-pool for Python handler execution.
//!
//! Non-native routes run user Python code, which (on standard CPython) must
//! acquire the GIL. Dispatching that via `tokio::spawn_blocking` +
//! `Python::attach` deadlocks at high concurrency: many tokio blocking threads
//! all contend for the GIL while the async runtime needs those very threads to
//! make progress (see `DECISIONS.md` ADR-049).
//!
//! The fix: a small, *dedicated* pool of OS threads. Each worker has its own
//! job channel and blocks on it independently, so jobs are dispatched in
//! round-robin and run with true parallelism (no single mutex serializing the
//! deque). The async runtime merely awaits a oneshot result. The GIL/blocking
//! pool interaction that caused the deadlock is eliminated, and GIL contention
//! is bounded to the pool size.
//!
//! On a free-threaded (no-GIL) Python build the GIL acquisition inside
//! `Python::attach` is a no-op, so this same code path yields true parallel
//! Python execution across the pool — automatically, with no per-request
//! branch. The runtime mode is detected **once** at startup (see [`init`]).

use pyo3::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::OnceLock;
use tokio::sync::oneshot;

use crate::native::handlers::fast_dumps;
use crate::native::types::get_helper;
use crate::native::types::NativeResponse;

/// Whether the running interpreter uses the GIL (standard CPython) or is
/// free-threaded (no-GIL build, where `Python::attach` is a no-op).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeMode {
    GilBased,
    GilFree,
}

impl RuntimeMode {
    pub fn from_gil_enabled(enabled: bool) -> Self {
        if enabled {
            RuntimeMode::GilBased
        } else {
            RuntimeMode::GilFree
        }
    }
    pub fn is_gil_free(self) -> bool {
        self == RuntimeMode::GilFree
    }
}

type Job = Box<dyn for<'py> FnOnce(Python<'py>) + Send + 'static>;

struct GilPool {
    /// One sender per worker thread (each worker has its own receiver, so it
    /// can block on `recv` independently and run jobs in parallel).
    senders: Vec<std_mpsc::Sender<Job>>,
    mode: RuntimeMode,
    /// Round-robin dispatch counter.
    next: AtomicUsize,
}

static POOL: OnceLock<GilPool> = OnceLock::new();

fn default_pool_size(mode: RuntimeMode) -> usize {
    // Allow operators to override the GIL-pool size via JUSTAPI_GIL_WORKERS
    // (useful for tuning on free-threaded builds, or forcing more CPython
    // workers despite the per-job GIL contention).
    if let Some(n) = std::env::var("JUSTAPI_GIL_WORKERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
    {
        return n;
    }
    if mode.is_gil_free() {
        // Free-threaded Python: workers execute Python in true parallel, so
        // scale the pool with the hardware.
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).max(2)
    } else {
        // Standard CPython (GIL): only one thread can run Python bytecodes at a
        // time, so spawning N workers only makes them contend for the GIL and
        // pays per-job `PyGILState_Ensure`/`Release` + switch overhead. A single
        // worker that holds the GIL and drains its queue is optimal here.
        1
    }
}

/// Detect the interpreter's GIL mode ONCE. Free-threaded Python exposes
/// `sys._is_gil_enabled == False`; standard CPython exposes `True` (or lacks the
/// attribute, in which case we assume GIL-based — the safe default).
fn detect_mode(py: Python<'_>) -> RuntimeMode {
    let gil_enabled = py
        .import("sys")
        .ok()
        .and_then(|sys| sys.getattr("_is_gil_enabled").ok())
        .and_then(|v| v.extract::<bool>().ok())
        .unwrap_or(true);
    RuntimeMode::from_gil_enabled(gil_enabled)
}

/// Initialize the dedicated GIL pool. Safe to call multiple times; only the
/// first call takes effect. Detects and logs the runtime mode once.
pub fn init(py: Python<'_>, num_threads: Option<usize>) {
    let mode = detect_mode(py);
    let n = num_threads.unwrap_or_else(|| default_pool_size(mode));
    POOL.get_or_init(|| build_pool(n, mode));
    // Warm up the Python helper module ONCE on the main thread, while we still
    // hold the GIL and before any GIL-pool worker can touch it. The helper is
    // backed by a `OnceLock` whose initializer executes Python code
    // (`PyModule::from_code`) that *releases the GIL* mid-initialization. If two
    // pool workers raced on that `OnceLock` they would deadlock: one holds the
    // init lock and has dropped the GIL to compile the module, the other grabs
    // the GIL and blocks on the init lock (ADR-049, part 2). Initializing here
    // on the main thread removes the race entirely. The same applies to the
    // `orjson`/`json` serializer cache in `fast_dumps`, so warm that too.
    let _ = get_helper(py);
    let _ = fast_dumps(py);
    tracing::info!(
        target: "justapi::gil_pool",
        "GIL pool initialized: mode={:?}, workers={} — {}",
        mode,
        n,
        if mode.is_gil_free() {
            "free-threaded Python: pool runs Python in parallel (no GIL)"
        } else {
            "standard CPython: GIL-serialized, deadlock-safe dispatch"
        }
    );
}

fn build_pool(num_threads: usize, mode: RuntimeMode) -> GilPool {
    let mut senders = Vec::with_capacity(num_threads);
    for i in 0..num_threads {
        let (tx, rx) = std_mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name(format!("justapi-gil-{}", i))
            .spawn(move || {
                // Per-job GIL acquire (a no-op on free-threaded builds). The
                // pool is dedicated and bounded, so this never deadlocks with
                // the tokio runtime the way `spawn_blocking` + `Python::attach`
                // did (ADR-049).
                while let Ok(job) = rx.recv() {
                    Python::attach(job);
                }
            })
            .expect("failed to spawn justapi GIL pool worker");
        senders.push(tx);
    }
    GilPool { senders, mode, next: AtomicUsize::new(0) }
}

/// Env-gated profiler for the `run_python` round-trip (closure build + channel
/// send + worker GIL execution + oneshot recv). Activated only when
/// `JUSTAPI_PROFILE` is set; otherwise a cheap `OnceLock` boolean load.
fn profile_gil_pool(total_ns: u64) {
    fn enabled() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| std::env::var_os("JUSTAPI_PROFILE").is_some())
    }
    if !enabled() {
        return;
    }
    use std::sync::Mutex;
    static STATS: std::sync::OnceLock<Mutex<(u64, u128)>> = std::sync::OnceLock::new();
    let mut s = STATS.get_or_init(|| Mutex::new((0u64, 0u128))).lock().unwrap();
    s.0 += 1;
    s.1 += total_ns as u128;
    if s.0.is_multiple_of(100_000) {
        eprintln!("[profile-pool] n={} avg_run_python={}ns", s.0, (s.1 / s.0 as u128) as u64,);
    }
}

/// Dispatch a Python-executing closure to the dedicated GIL pool and await its
/// `NativeResponse`. Replaces `tokio::task::spawn_blocking(|| Python::attach(...))`.
pub async fn run_python<F>(f: F) -> anyhow::Result<NativeResponse>
where
    F: for<'py> FnOnce(Python<'py>) -> NativeResponse + Send + 'static,
{
    let _t0 = std::time::Instant::now();
    let pool = POOL.get_or_init(|| {
        build_pool(default_pool_size(RuntimeMode::GilBased), RuntimeMode::GilBased)
    });
    let n = pool.senders.len();
    let idx = pool.next.fetch_add(1, Ordering::Relaxed) % n;
    let (resp_tx, resp_rx) = oneshot::channel();
    let job: Job = Box::new(move |py| {
        // SAFETY / panic policy: a genuine Rust panic inside `f` (e.g. an
        // `unwrap` in user or framework code) must NOT unwind across pyo3's FFI
        // boundary — that is undefined behaviour and, with a single GIL worker,
        // would wedge every Python route on its oneshot forever. We do NOT wrap
        // each job in `catch_unwind` here: doing so inhibits optimization of the
        // hot closure and measured as a ~3x throughput regression on the GIL
        // path. Instead the workspace root `[profile.release]` builds with
        // `panic = "abort"`, so a panic safely aborts the
        // whole process (no unwind, no UB) and the process supervisor
        // (Docker/k8s, systemd) restarts it. See DECISIONS.md ("Panic policy at
        // the GIL FFI boundary"). Python exceptions are a separate, safe path
        // (surfaced as `PyErr`, not panics) handled by the caller.
        let resp = f(py);
        let _ = resp_tx.send(resp);
    });
    pool.senders[idx].send(job).map_err(|_| anyhow::anyhow!("gil pool closed"))?;
    let resp = resp_rx.await.map_err(|_| anyhow::anyhow!("gil pool worker dropped"))?;
    profile_gil_pool(_t0.elapsed().as_nanos() as u64);
    Ok(resp)
}

/// The detected runtime mode (GIL-based or free-threaded), if the pool has been
/// initialized.
pub fn mode() -> Option<RuntimeMode> {
    POOL.get().map(|p| p.mode)
}
