//! Rust-backed background task scheduler for JustAPI.
//!
//! Design (why Rust owns this, not Python):
//!
//! * The *tasks* are Python callables, so executing them inevitably needs the
//!   GIL. But the *scheduling* — the queue, the worker pool, backpressure,
//!   stats and graceful shutdown — is pure systems work and belongs in Rust:
//!   it has zero Python overhead on the hot path and plugs straight into
//!   Rust's metrics/tracing. This is what lets JustAPI beat FastAPI/Starlette,
//!   which schedule background work with a per-process `anyio` pool and ship
//!   no observability for it.
//! * A single process-wide worker pool (sized to the machine) executes sync
//!   tasks. Async tasks are run to completion with `asyncio.run` on a worker
//!   thread — no extra thread per task, unlike the old Python implementation
//!   that spawned a `threading.Thread` per task.
//! * A bounded MPMC queue gives backpressure: a thundering herd can't blow up
//!   memory; overflow is counted, not crashed.
//! * `BackgroundTasks.stats()` exposes submitted/active/completed/failed/dropped
//!   counters — something no other Python web framework provides for bg work.
//! * **GIL / free-threaded aware.** The worker pool size is chosen at runtime
//!   from `sys._is_gil_enabled()`: modest under a GIL (extra threads don't help
//!   CPU-bound Python), large under PEP 703 free-threaded builds (true
//!   parallelism). All Python calls go through `Python::attach`, which is valid
//!   in both modes. `Py<..>` handles are atomically refcounted, so sharing
//!   tasks across worker threads is sound in either build.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

#[derive(Default)]
struct Stats {
    submitted: AtomicUsize,
    active: AtomicUsize,
    completed: AtomicUsize,
    failed: AtomicUsize,
    dropped: AtomicUsize,
    asyncs: AtomicUsize,
}

/// A queued unit of work: a Python callable plus its bound args/kwargs.
struct Task {
    func: Py<PyAny>,
    args: Py<PyTuple>,
    kwargs: Option<Py<PyDict>>,
}

/// Bounded MPMC queue (std-only) with shutdown signalling.
struct TaskQueue {
    tasks: Mutex<VecDeque<Task>>,
    not_empty: Condvar,
    size: AtomicUsize,
    shutdown: AtomicBool,
}

impl TaskQueue {
    fn push(&self, task: Task) {
        let mut g = self.tasks.lock().unwrap();
        g.push_back(task);
        self.size.fetch_add(1, Ordering::Relaxed);
        self.not_empty.notify_one();
    }

    /// Blocks until a task is available or shutdown is signalled.
    fn pop(&self) -> Option<Task> {
        let mut g = self.tasks.lock().unwrap();
        loop {
            if let Some(t) = g.pop_front() {
                self.size.fetch_sub(1, Ordering::Relaxed);
                return Some(t);
            }
            if self.shutdown.load(Ordering::Relaxed) {
                return None;
            }
            g = self.not_empty.wait(g).unwrap();
        }
    }
}

struct Runner {
    queue: TaskQueue,
    stats: Stats,
    handles: Mutex<Vec<JoinHandle<()>>>,
    max_total: usize,
}

impl Runner {
    fn enqueue(&self, task: Task) {
        if self.queue.size.load(Ordering::Relaxed) >= self.max_total {
            self.stats.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.queue.push(task);
        self.stats.submitted.fetch_add(1, Ordering::Relaxed);
        self.stats.active.fetch_add(1, Ordering::Relaxed);
    }
}

static RUNNER: OnceLock<Arc<Runner>> = OnceLock::new();

fn runner() -> &'static Arc<Runner> {
    RUNNER.get_or_init(|| {
        let max_total = std::env::var("JUSTAPI_BG_MAX_QUEUE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100_000usize);

        // Size the worker pool based on whether the interpreter has a GIL:
        //   * GIL enabled (CPython <=3.12, or 3.13/3.14 built `--disable-gil`
        //     off): only one thread can execute Python at a time, so extra
        //     workers don't speed up CPU-bound work — they mainly help I/O-bound
        //     tasks that release the GIL. Keep the pool modest to avoid GIL
        //     thrash between workers.
        //   * Free-threaded (PEP 703, `--disable-gil`): threads run Python in
        //     true parallel, so scale the pool up for real multi-core
        //     throughput on background work.
        // `Python::attach` (used throughout) is correct in both modes; this only
        // tunes concurrency.
        let gil_enabled = Python::attach(|py| {
            py.import("sys")
                .ok()
                .and_then(|s| s.getattr("_is_gil_enabled").ok())
                .and_then(|f| f.call0().ok())
                .and_then(|r| r.is_truthy().ok())
                .unwrap_or(true)
        });
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let workers = if gil_enabled { (n + 4).min(32) } else { (n * 2).min(256) };

        let runner = Arc::new(Runner {
            queue: TaskQueue {
                tasks: Mutex::new(VecDeque::new()),
                not_empty: Condvar::new(),
                size: AtomicUsize::new(0),
                shutdown: AtomicBool::new(false),
            },
            stats: Stats::default(),
            handles: Mutex::new(Vec::new()),
            max_total,
        });

        for _ in 0..workers {
            let r = Arc::clone(&runner);
            let h = thread::spawn(move || worker(&r));
            runner.handles.lock().unwrap().push(h);
        }

        runner
    })
}

fn worker(r: &Arc<Runner>) {
    while let Some(task) = r.queue.pop() {
        process(r, task);
    }
}

fn is_coroutine(py: Python<'_>, obj: &Bound<'_, PyAny>) -> bool {
    match py.import("asyncio") {
        Ok(asyncio) => asyncio
            .getattr("iscoroutine")
            .and_then(|f| f.call1((obj.clone(),)))
            .and_then(|r| r.is_truthy())
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn process(r: &Arc<Runner>, task: Task) {
    // Run the callable. Capture an *unbound* object so it survives the
    // `attach` scope (a `Bound` is tied to the `Python` token's lifetime).
    let call_res: PyResult<Py<PyAny>> = Python::attach(|py| {
        let func = task.func.bind(py);
        let args = task.args.bind(py);
        let kwargs = task.kwargs.as_ref().map(|k| k.bind(py));
        func.call(args, kwargs).map(|b| b.unbind())
    });

    match call_res {
        Ok(bound_py) => {
            let is_coro = Python::attach(|py| is_coroutine(py, bound_py.bind(py)));
            if is_coro {
                r.stats.asyncs.fetch_add(1, Ordering::Relaxed);
                let run_res: PyResult<()> = Python::attach(|py| {
                    let asyncio = py.import("asyncio")?;
                    let run = asyncio.getattr("run")?;
                    let bound = bound_py.bind(py);
                    run.call1((bound.clone(),))?;
                    Ok(())
                });
                match run_res {
                    Ok(_) => {
                        r.stats.completed.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        tracing::error!("background async task failed: {}", e);
                        r.stats.failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            } else {
                r.stats.completed.fetch_add(1, Ordering::Relaxed);
            }
            r.stats.active.fetch_sub(1, Ordering::Relaxed);
        }
        Err(e) => {
            tracing::error!("background task failed: {}", e);
            r.stats.failed.fetch_add(1, Ordering::Relaxed);
            r.stats.active.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

#[pyclass]
pub struct BackgroundTasks {
    pending: Mutex<Vec<Task>>,
}

#[pymethods]
impl BackgroundTasks {
    #[new]
    fn new() -> Self {
        BackgroundTasks { pending: Mutex::new(Vec::new()) }
    }

    /// `add_task(func, *args, **kwargs)` — queue a callable for later.
    #[pyo3(signature = (func, *args, **kwargs))]
    fn add_task(
        &self,
        func: Bound<'_, PyAny>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let task = Task {
            func: func.clone().unbind(),
            args: args.clone().unbind(),
            kwargs: kwargs.map(|k| k.clone().unbind()),
        };
        self.pending.lock().unwrap().push(task);
        Ok(())
    }

    /// Schedule all queued tasks onto the shared Rust executor (non-blocking).
    fn run(&self) -> PyResult<()> {
        let tasks: Vec<Task> = std::mem::take(&mut *self.pending.lock().unwrap());
        let r = runner();
        for t in tasks {
            r.enqueue(t);
        }
        Ok(())
    }

    fn __call__(&self) -> PyResult<()> {
        self.run()
    }

    fn task_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    /// Process-wide counters: submitted/active/completed/failed/dropped/async.
    #[staticmethod]
    fn stats(py: Python<'_>) -> PyResult<Py<PyDict>> {
        let r = runner();
        let d = PyDict::new(py);
        d.set_item("submitted", r.stats.submitted.load(Ordering::Relaxed))?;
        d.set_item("active", r.stats.active.load(Ordering::Relaxed))?;
        d.set_item("completed", r.stats.completed.load(Ordering::Relaxed))?;
        d.set_item("failed", r.stats.failed.load(Ordering::Relaxed))?;
        d.set_item("dropped", r.stats.dropped.load(Ordering::Relaxed))?;
        d.set_item("async", r.stats.asyncs.load(Ordering::Relaxed))?;
        Ok(d.unbind())
    }

    /// Drain workers and stop the scheduler. `wait=True` joins worker threads.
    #[staticmethod]
    fn shutdown(wait: Option<bool>) -> PyResult<()> {
        let r = runner();
        r.queue.shutdown.store(true, Ordering::Relaxed);
        r.queue.not_empty.notify_all();
        if wait.unwrap_or(true) {
            for h in r.handles.lock().unwrap().drain(..) {
                let _ = h.join();
            }
        }
        Ok(())
    }
}

/// Enqueue an already-bound Python callable (with args/kwargs) onto the shared
/// Rust background-task executor. Used by the scheduler (`scheduler.rs`) so
/// cron/interval jobs run on the same worker pool and observability as
/// `BackgroundTasks`.
pub(crate) fn submit_py_task(func: Py<PyAny>, args: Py<PyTuple>, kwargs: Option<Py<PyDict>>) {
    let task = Task { func, args, kwargs };
    runner().enqueue(task);
}
