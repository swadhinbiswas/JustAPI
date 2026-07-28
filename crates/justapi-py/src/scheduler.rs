//! Native Rust scheduler for JustAPI — cron + interval jobs.
//!
//! Why Rust owns this (AGENTS.md §2): scheduling is systems work (a tick loop,
//! next-fire math, job bookkeeping, graceful start/stop). The *jobs* are Python
//! callables, so executing them needs the GIL — but they are dispatched onto the
//! existing Rust background-task worker pool (`background::submit_py_task`), the
//! same pool and observability used by `BackgroundTasks`. So scheduling is Rust,
//! execution reuses the Rust executor, and only the user callback crosses into
//! Python.
//!
//! v1 scope (see DECISIONS.md ADR-060): UTC-only, in-memory (no persistence).
//! Cron expressions are parsed by the maintained `cron` crate rather than a
//! hand-rolled parser, to avoid DST/leap-year/`L`/`W`/`#` edge-case bugs.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use cron::Schedule;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use crate::background::submit_py_task;

/// A single scheduled unit of work.
struct Job {
    id: u64,
    name: String,
    cron: Option<Schedule>,
    interval: Option<Duration>,
    func: Py<PyAny>,
    args: Py<PyTuple>,
    kwargs: Option<Py<PyDict>>,
    last_fire_ms: Mutex<Option<i64>>,
    next_fire_ms: Mutex<Option<i64>>,
}

impl Job {
    /// Compute the next fire time given "now", advancing past any missed tick.
    fn compute_next(&self, now_ms: i64) -> Option<i64> {
        compute_next(
            &self.cron,
            self.interval,
            *self.last_fire_ms.lock().unwrap_or_else(|e| e.into_inner()),
            now_ms,
        )
    }
}

/// Pure next-fire math (no Python/GIL needed) — unit-testable in isolation.
fn compute_next(
    cron: &Option<Schedule>,
    interval: Option<Duration>,
    last_fire_ms: Option<i64>,
    now_ms: i64,
) -> Option<i64> {
    if let Some(ref s) = cron {
        let now = DateTime::<Utc>::from_timestamp_millis(now_ms)?;
        let next = s.after(&now).next()?;
        return Some(next.timestamp_millis());
    }
    if let Some(iv) = interval {
        // First run fires one interval after the job was registered; later
        // runs advance by the interval from the previous fire.
        let base = last_fire_ms.unwrap_or(now_ms);
        let step = iv.as_millis() as i64;
        let mut next = base + step;
        if next <= now_ms {
            next = now_ms + step;
        }
        return Some(next);
    }
    None
}

#[derive(Default)]
struct SchedStats {
    fired: AtomicUsize,
    failed: AtomicUsize,
}

struct SchedulerInner {
    jobs: Mutex<Vec<Job>>,
    stats: SchedStats,
    next_id: AtomicU64,
    running: AtomicBool,
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    // Dedicated runtime for the tick loop so its `JoinHandle` stays valid for
    // the process lifetime (mirrors `db_runtime` in app.rs).
    rt: Mutex<Option<tokio::runtime::Runtime>>,
}

/// Process-wide scheduler. A `OnceLock` so `app.schedule(...)` and `app.run()`
/// (and the `Scheduler` pyclass) share one instance.
pub struct Scheduler;

impl Scheduler {
    fn get() -> &'static Arc<SchedulerInner> {
        static SCHED: OnceLock<Arc<SchedulerInner>> = OnceLock::new();
        SCHED.get_or_init(|| {
            Arc::new(SchedulerInner {
                jobs: Mutex::new(Vec::new()),
                stats: SchedStats::default(),
                next_id: AtomicU64::new(1),
                running: AtomicBool::new(false),
                handle: Mutex::new(None),
                rt: Mutex::new(None),
            })
        })
    }

    /// Start the tick loop. Idempotent.
    fn start() {
        let inner = Scheduler::get();
        if inner.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("scheduler runtime");
        let inner_clone = Arc::clone(inner);
        let handle = rt.spawn(async move { tick_loop(inner_clone).await });
        *inner.rt.lock().unwrap_or_else(|e| e.into_inner()) = Some(rt);
        *inner.handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
    }

    fn stop() {
        let inner = Scheduler::get();
        if !inner.running.swap(false, Ordering::SeqCst) {
            return;
        }
        if let Some(h) = inner.handle.lock().unwrap_or_else(|e| e.into_inner()).take() {
            h.abort();
        }
        if let Some(rt) = inner.rt.lock().unwrap_or_else(|e| e.into_inner()).take() {
            rt.shutdown_timeout(Duration::from_millis(500));
        }
    }
}

/// The background tick: every 250ms, fire any job whose next-fire is due.
async fn tick_loop(inner: Arc<SchedulerInner>) {
    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    while inner.running.load(Ordering::SeqCst) {
        ticker.tick().await;
        if !inner.running.load(Ordering::SeqCst) {
            break;
        }
        let now_ms = Utc::now().timestamp_millis();
        let jobs = inner.jobs.lock().unwrap_or_else(|e| e.into_inner());
        for job in jobs.iter() {
            // Resolve next-fire if not yet computed.
            let mut next = *job.next_fire_ms.lock().unwrap_or_else(|e| e.into_inner());
            if next.is_none() {
                next = job.compute_next(now_ms);
                *job.next_fire_ms.lock().unwrap_or_else(|e| e.into_inner()) = next;
            }
            let due = match next {
                Some(t) => t <= now_ms,
                None => false,
            };
            if due {
                // Clone the Python handles under the GIL, then enqueue onto the
                // Rust background pool (which releases the GIL during execution).
                let (func, args, kwargs) = Python::attach(|py| {
                    (
                        job.func.clone_ref(py),
                        job.args.clone_ref(py),
                        job.kwargs.as_ref().map(|k| k.clone_ref(py)),
                    )
                });
                submit_py_task(func, args, kwargs);
                inner.stats.fired.fetch_add(1, Ordering::Relaxed);
                *job.last_fire_ms.lock().unwrap_or_else(|e| e.into_inner()) = Some(now_ms);
                // Advance to the next fire time.
                let adv = job.compute_next(now_ms);
                *job.next_fire_ms.lock().unwrap_or_else(|e| e.into_inner()) = adv;
            }
        }
    }
}

#[pyclass(name = "Scheduler")]
pub struct PyScheduler;

#[pymethods]
impl PyScheduler {
    #[new]
    fn new() -> Self {
        PyScheduler
    }

    /// `schedule(cron_expr, func, *args, **kwargs)` — register a cron job.
    /// `cron_expr` is a standard 5-field expression evaluated in UTC.
    #[pyo3(signature = (cron_expr, func, *args, **kwargs))]
    fn schedule(
        &self,
        _py: Python<'_>,
        cron_expr: &str,
        func: Bound<'_, PyAny>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<u64> {
        let schedule: Schedule = cron_expr.parse().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid cron expr: {e}"))
        })?;
        let inner = Scheduler::get();
        let id = inner.next_id.fetch_add(1, Ordering::Relaxed);
        let job = Job {
            id,
            name: format!("cron:{}", cron_expr),
            cron: Some(schedule),
            interval: None,
            func: func.clone().unbind(),
            args: args.clone().unbind(),
            kwargs: kwargs.map(|k| k.clone().unbind()),
            last_fire_ms: Mutex::new(None),
            next_fire_ms: Mutex::new(None),
        };
        inner.jobs.lock().unwrap_or_else(|e| e.into_inner()).push(job);
        Ok(id)
    }

    /// `every(seconds, func, *args, **kwargs)` — register an interval job that
    /// fires every `seconds` (first fire is one interval after registration).
    #[pyo3(signature = (seconds, func, *args, **kwargs))]
    fn every(
        &self,
        _py: Python<'_>,
        seconds: u64,
        func: Bound<'_, PyAny>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<u64> {
        if seconds == 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "every(seconds) requires seconds > 0",
            ));
        }
        let inner = Scheduler::get();
        let id = inner.next_id.fetch_add(1, Ordering::Relaxed);
        let job = Job {
            id,
            name: format!("every:{}s", seconds),
            cron: None,
            interval: Some(Duration::from_secs(seconds)),
            func: func.clone().unbind(),
            args: args.clone().unbind(),
            kwargs: kwargs.map(|k| k.clone().unbind()),
            last_fire_ms: Mutex::new(None),
            next_fire_ms: Mutex::new(None),
        };
        inner.jobs.lock().unwrap_or_else(|e| e.into_inner()).push(job);
        Ok(id)
    }

    /// Start the scheduler tick loop. Idempotent.
    fn start(&self) {
        Scheduler::start();
    }

    /// Stop the scheduler tick loop (jobs already enqueued on the BG pool still
    /// run to completion).
    fn stop(&self) {
        Scheduler::stop();
    }

    /// Aggregate counters: `{jobs, fired, running}`.
    fn stats(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let inner = Scheduler::get();
        let d = PyDict::new(py);
        d.set_item("jobs", inner.jobs.lock().unwrap_or_else(|e| e.into_inner()).len())?;
        d.set_item("fired", inner.stats.fired.load(Ordering::Relaxed))?;
        d.set_item("failed", inner.stats.failed.load(Ordering::Relaxed))?;
        d.set_item("running", inner.running.load(Ordering::SeqCst))?;
        Ok(d.unbind())
    }

    /// List registered jobs with their next/last fire times (epoch millis).
    fn jobs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let inner = Scheduler::get();
        let list = PyList::empty(py);
        for job in inner.jobs.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            let d = PyDict::new(py);
            d.set_item("id", job.id)?;
            d.set_item("name", &job.name)?;
            d.set_item("cron", job.cron.as_ref().map(|s| s.to_string()))?;
            d.set_item("interval_secs", job.interval.map(|i| i.as_secs()))?;
            d.set_item(
                "last_fire_ms",
                *job.last_fire_ms.lock().unwrap_or_else(|e| e.into_inner()),
            )?;
            d.set_item(
                "next_fire_ms",
                *job.next_fire_ms.lock().unwrap_or_else(|e| e.into_inner()),
            )?;
            list.append(d)?;
        }
        Ok(list.into_any().unbind())
    }

    /// Remove a job by id. Returns True if found.
    fn remove(&self, job_id: u64) -> bool {
        let inner = Scheduler::get();
        let mut jobs = inner.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let before = jobs.len();
        jobs.retain(|j| j.id != job_id);
        jobs.len() != before
    }
}

/// Used by `app.run()` to start the scheduler when jobs are registered.
pub fn maybe_start_if_jobs() {
    let inner = Scheduler::get();
    if !inner.jobs.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
        Scheduler::start();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn cron_next_fire_is_in_future() {
        let cron = Some(Schedule::from_str("* * * * * *").unwrap()); // every second (6-field UTC)
        let now = Utc::now().timestamp_millis();
        let next = compute_next(&cron, None, None, now).unwrap();
        assert!(next > now, "cron next-fire must be strictly after now");
        assert!(next - now <= 1000, "every-second cron should fire within 1s");
    }

    #[test]
    fn cron_minute_boundary() {
        let cron = Some(Schedule::from_str("0 * * * * *").unwrap()); // second 0 of every minute
        let now = Utc::now().timestamp_millis();
        let next = compute_next(&cron, None, None, now).unwrap();
        // Next fire lands on a second boundary (epoch is second-aligned).
        assert_eq!(next % 60_000, 0, "minute-boundary cron must fire at :00 seconds");
        assert!(next > now);
    }

    #[test]
    fn interval_first_fire_is_one_period_out() {
        let next =
            compute_next(&None, Some(Duration::from_secs(30)), None, 1_000_000_000_000).unwrap();
        assert_eq!(next, 1_000_000_030_000, "interval first fire = now + period");
    }

    #[test]
    fn interval_advances_from_last_fire() {
        let now = 1_000_000_000_000i64;
        let next =
            compute_next(&None, Some(Duration::from_secs(30)), Some(now), now + 5_000).unwrap();
        assert_eq!(next, now + 30_000);
    }

    #[test]
    fn interval_skips_past_if_behind() {
        // If last_fire is far in the past, next must be at least now + period.
        let now = 2_000_000_000_000i64;
        let next = compute_next(&None, Some(Duration::from_secs(30)), Some(1_000_000_000_000), now)
            .unwrap();
        assert_eq!(next, now + 30_000);
    }

    #[test]
    fn invalid_cron_rejected() {
        assert!(Schedule::from_str("not a cron").is_err());
    }
}
