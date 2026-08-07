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
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::oneshot;

use crate::native::handlers::fast_dumps;
use crate::native::types::get_helper;

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
    /// Uses tokio's bounded mpsc to avoid blocking the sender (an async
    /// context) when all workers are busy.
    senders: Vec<tokio_mpsc::Sender<Job>>,
    mode: RuntimeMode,
    /// Round-robin dispatch counter.
    next: std::sync::Arc<AtomicUsize>,
}

/// Process ID that owns the current pool. `fork(2)` copies the parent's
/// address space, so the child would inherit a `GilPool` whose worker threads
/// do not exist in the child — every `send` would block forever (observed as
/// 504 timeouts in the child). Any PID change means we are in a forked child
/// (or a new process) and the pool must be rebuilt on its own threads.
static POOL_PID: AtomicU32 = AtomicU32::new(0);
static POOL: std::sync::Mutex<Option<GilPool>> = std::sync::Mutex::new(None);

const CHANNEL_CAP_PER_WORKER: usize = 16;

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

/// Detect the interpreter's GIL mode. Free-threaded builds of CPython
/// (3.13t/3.14t) set the compile-time `Py_GIL_DISABLED` cfg (via
/// pyo3-build-config) — the authoritative signal. The runtime
/// `sys._is_gil_enabled` read is NOT reliable through pyo3's import on
/// free-threaded interpreters (reads true), so it is only a fallback for
/// builds where the cfg is unavailable.
fn detect_mode(_py: Python<'_>) -> RuntimeMode {
    #[cfg(Py_GIL_DISABLED)]
    {
        RuntimeMode::GilFree
    }
    #[cfg(not(Py_GIL_DISABLED))]
    {
        let py = _py;
        let gil_enabled = py
            .import("sys")
            .ok()
            .and_then(|sys| sys.getattr("_is_gil_enabled").ok())
            .and_then(|v| v.extract::<bool>().ok())
            .unwrap_or(true);
        RuntimeMode::from_gil_enabled(gil_enabled)
    }
}

/// Initialize the dedicated GIL pool. Safe to call multiple times; only the
/// first call takes effect. Detects and logs the runtime mode once.
pub fn init(py: Python<'_>, num_threads: Option<usize>) {
    let mode = detect_mode(py);
    let n = num_threads.unwrap_or_else(|| default_pool_size(mode));
    init_pool(num_threads, py);
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
        let cap = num_threads.max(1) * CHANNEL_CAP_PER_WORKER;
        let (tx, mut rx) = tokio_mpsc::channel::<Job>(cap);
        std::thread::Builder::new()
            .name(format!("justapi-gil-{}", i))
            .spawn(move || {
                // Per-job GIL acquire (a no-op on free-threaded builds). The
                // pool is dedicated and bounded, so this never deadlocks with
                // the tokio runtime the way `spawn_blocking` + `Python::attach`
                // did (ADR-049).
                while let Some(job) = rx.blocking_recv() {
                    Python::attach(job);
                }
            })
            .expect("failed to spawn justapi GIL pool worker");
        senders.push(tx);
    }
    GilPool { senders, mode, next: std::sync::Arc::new(AtomicUsize::new(0)) }
}

/// Initialize the GIL pool with a warmup (must hold the GIL). Safe to call
/// multiple times; does nothing if the pool is already initialized for this
/// process. If called after `fork(2)` (PID changed), rebuilds the pool.
fn init_pool(num_threads: Option<usize>, py: Python<'_>) {
    let mode = detect_mode(py);
    let n = num_threads.unwrap_or_else(|| default_pool_size(mode));
    let pid = std::process::id();
    {
        let mut guard = POOL.lock().unwrap_or_else(|e| e.into_inner());
        let needs_rebuild = guard.is_none() || POOL_PID.load(Ordering::Acquire) != pid;
        if needs_rebuild {
            *guard = Some(build_pool(n, mode));
            POOL_PID.store(pid, Ordering::Release);
        }
    }
    // Warm up OnceLock-backed helpers on the calling thread while we hold the
    // GIL, so GIL-pool workers never race on them. See `init()` docstring.
    let _ = get_helper(py);
    let _ = fast_dumps(py);
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
    let mut s =
        STATS.get_or_init(|| Mutex::new((0u64, 0u128))).lock().unwrap_or_else(|e| e.into_inner());
    s.0 += 1;
    s.1 += total_ns as u128;
    if s.0.is_multiple_of(100_000) {
        eprintln!("[profile-pool] n={} avg_run_python={}ns", s.0, (s.1 / s.0 as u128) as u64,);
    }
}

/// Dispatch a Python-executing closure to the dedicated GIL pool and await the
/// result. Replaces `tokio::task::spawn_blocking(|| Python::attach(...))`.
///
/// The pool is lazily initialized on first call if `init()` hasn't been called
/// yet. The lazy path performs the same warmup as `init()`.
pub async fn run_python<F, T>(f: F) -> anyhow::Result<T>
where
    F: for<'py> FnOnce(Python<'py>) -> T + Send + 'static,
    T: Send + 'static,
{
    let _t0 = std::time::Instant::now();
    let pid = std::process::id();
    let pool = {
        let mut guard = POOL.lock().unwrap_or_else(|e| e.into_inner());
        let same_process = guard.is_some() && POOL_PID.load(Ordering::Acquire) == pid;
        if !same_process {
            // Forked child (or first call without init): rebuild the pool.
            // Mode detection uses the compile-time Py_GIL_DISABLED cfg — no
            // `Python` token needed (the runtime sys._is_gil_enabled read is
            // unreliable on free-threaded interpreters). get_helper/fast_dumps
            // are themselves lazy and safe to initialize on the first worker
            // that reaches them (their OnceLock provides mutual exclusion).
            #[cfg(Py_GIL_DISABLED)]
            let mode = RuntimeMode::GilFree;
            #[cfg(not(Py_GIL_DISABLED))]
            let mode = RuntimeMode::GilBased;
            let n = default_pool_size(mode);
            *guard = Some(build_pool(n, mode));
            POOL_PID.store(pid, Ordering::Release);
        }
        let p = guard.as_ref().expect("pool present");
        (p.senders.clone(), p.next.clone())
    };
    let (senders, next) = pool;
    let n = senders.len();
    let idx = if n == 1 { 0usize } else { next.fetch_add(1, Ordering::Relaxed) % n };
    let (resp_tx, resp_rx) = oneshot::channel::<T>();
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
    // Backpressure, not drop: `try_send` would reject a job whenever a worker's
    // bounded queue is momentarily full (single GIL worker => cap = 16), and
    // `handle_request` masks ANY handler `Err` as an RFC 9457 404. Under a burst
    // of concurrent/pipelined requests that produced spurious 404s with no log.
    // `send` awaits until the worker drains a slot, so load is throttled to the
    // pool's real capacity instead of surfacing as an error (ADR-049 dispatch).
    senders[idx]
        .send(job)
        .await
        .map_err(|_| anyhow::anyhow!("gil pool worker dropped (channel closed)"))?;
    let resp = resp_rx.await.map_err(|_| anyhow::anyhow!("gil pool worker dropped"))?;
    profile_gil_pool(_t0.elapsed().as_nanos() as u64);
    Ok(resp)
}

/// The detected runtime mode (GIL-based or free-threaded), if the pool has been
/// initialized.
pub fn mode() -> Option<RuntimeMode> {
    let guard = POOL.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().map(|p| p.mode)
}

/// Run a Python closure on the tokio blocking pool (PARALLEL).
///
/// Correct ONLY on free-threaded CPython (Py_GIL_DISABLED): there is no GIL
/// to thrash, so `@native_async` handlers dispatch in true parallel
/// (available_parallelism workers). On GIL-locked builds this thrashes and
/// must not be used (measured regression, ADR-087).
pub async fn run_python_parallel<F, T>(f: F) -> anyhow::Result<T>
where
    F: FnOnce(Python<'_>) -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || Python::attach(f))
        .await
        .map_err(|e| anyhow::anyhow!("python blocking task error: {}", e))
}
