//! Multi-worker prefork supervisor for `justapi serve --workers N`.
//!
//! The parent binds the listening socket exactly once, then spawns `N` child
//! worker processes. Each child is a re-exec of the same binary in a hidden
//! `--worker-fd <fd>` mode that inherits the bound socket (fd inheritance, no
//! `SO_REUSEPORT` port-race), reconstructs the server from the original CLI
//! args, and serves on that socket. This gives true OS-process isolation and
//! saturates all cores — the single-process path only uses one accept loop.
//!
//! The supervisor owns process management:
//! - **Restart-on-death:** a worker that exits while not shutting down is
//!   respawned so the fleet stays at `N` workers.
//! - **Graceful drain:** SIGTERM/SIGINT cancels a `CancellationToken`; the
//!   supervisor forwards SIGTERM to every child and waits up to `drain_timeout`
//!   for in-flight requests to finish before forcibly reaping.
//! - **Hot reload is incompatible with prefork** (a code change must restart the
//!   whole tree); the caller rejects that combination before entering here.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tokio::signal::unix::{signal, SignalKind};
use tokio_util::sync::CancellationToken;

/// Raw CLI args (excluding `justapi serve` itself) needed to reconstruct an
/// identical worker. We re-pass them verbatim, plus `--worker-fd`.
pub struct WorkerSpawn {
    /// The exact argv (without program name) the parent was invoked with,
    /// e.g. `["serve", "--addr", "0.0.0.0:8080", "--compress"]`.
    pub argv: Vec<String>,
    /// Bound listening socket fd inherited by the child.
    pub listener_fd: i32,
    /// Max seconds to drain in-flight requests before forcibly reaping.
    pub drain_timeout: Duration,
}

/// Bind the address once and return the std listener (non-CLOEXEC fd so it
/// survives the child re-exec on Unix).
pub fn bind_listener(addr: SocketAddr) -> anyhow::Result<std::net::TcpListener> {
    let listener =
        std::net::TcpListener::bind(addr).with_context(|| format!("failed to bind {addr}"))?;
    listener.set_nonblocking(true).context("failed to set listener non-blocking")?;

    // Modern std sets FD_CLOEXEC on the socket fd, which would close it across
    // the child's exec. Clear it so the worker inherits the bound socket.
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let fd = listener.as_raw_fd();
        // SAFETY: fd is a valid, owned listener socket; we only clear CLOEXEC.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags >= 0 {
            unsafe {
                libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
            }
        }
    }

    Ok(listener)
}

/// Recover a tokio `TcpListener` from an inherited raw fd (Unix only).
#[cfg(unix)]
pub fn listener_from_fd(fd: i32) -> anyhow::Result<tokio::net::TcpListener> {
    use std::os::fd::FromRawFd;
    // SAFETY: the fd was created by the parent via `std::net::TcpListener::bind`
    // and is not closed on exec, so it is valid and uniquely owned by this
    // process after fork/exec. We do not `from_raw_fd` it anywhere else.
    let std_listener = unsafe { <std::net::TcpListener as FromRawFd>::from_raw_fd(fd) };
    std_listener
        .set_nonblocking(true)
        .context("worker: failed to set inherited listener non-blocking")?;
    tokio::net::TcpListener::from_std(std_listener)
        .context("worker: failed to wrap inherited fd as tokio TcpListener")
}

/// Recover a tokio `UnixListener` from an inherited raw fd (Unix only).
#[cfg(unix)]
pub fn listener_from_unix_fd(fd: i32) -> anyhow::Result<tokio::net::UnixListener> {
    use std::os::fd::FromRawFd;
    // SAFETY: the fd was created by the parent via `bind_unix_listener` and is
    // not closed on exec, so it is valid and uniquely owned by this process
    // after fork/exec. We do not `from_raw_fd` it anywhere else.
    let std_listener = unsafe { <std::os::fd::OwnedFd as FromRawFd>::from_raw_fd(fd) };
    let std_listener = std::os::unix::net::UnixListener::from(std_listener);
    std_listener
        .set_nonblocking(true)
        .context("worker: failed to set inherited unix listener non-blocking")?;
    tokio::net::UnixListener::from_std(std_listener)
        .context("worker: failed to wrap inherited fd as tokio UnixListener")
}

/// Bind a Unix domain socket once and return the std listener (non-CLOEXEC fd
/// so it survives the child re-exec on Unix). An existing socket file is
/// removed first to avoid EADDRINUSE on restart.
#[cfg(unix)]
pub fn bind_unix_listener(path: &str) -> anyhow::Result<std::os::unix::net::UnixListener> {
    use std::os::fd::AsRawFd;
    let p = std::path::Path::new(path);
    if p.exists() {
        let _ = std::fs::remove_file(p);
    }
    let listener = std::os::unix::net::UnixListener::bind(p)
        .with_context(|| format!("failed to bind unix socket {path}"))?;
    // Clear FD_CLOEXEC so the fd survives the child's exec (mirrors bind_listener).
    let fd = listener.as_raw_fd();
    // SAFETY: fd is a valid, owned listener socket; we only clear CLOEXEC.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags >= 0 {
        unsafe {
            libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
        }
    }
    Ok(listener)
}

/// Run the supervisor: spawn `count` workers and manage their lifecycle until
/// the shutdown token is cancelled (or a fatal spawn error occurs).
///
/// When `policy` is `Some((policy, probe))`, the fleet auto-scales between
/// `policy.min_workers` and `policy.max_workers` based on `probe.sample()`
/// (normalized system load), with a cooldown to avoid flapping.
pub async fn supervise(
    count: usize,
    spawn: WorkerSpawn,
    shutdown: CancellationToken,
    policy: Option<(ScalingPolicy, Arc<dyn LoadProbe>)>,
) -> anyhow::Result<()> {
    anyhow::ensure!(count >= 1, "worker count must be >= 1");
    let exe = std::env::current_exe().context("failed to resolve current executable")?;

    // Persistent JoinSet of in-flight worker waits. `pids[slot]` mirrors it so we
    // can forward SIGTERM without borrowing the (moved) `Child`.
    let mut waits: tokio::task::JoinSet<(usize, std::io::Result<std::process::ExitStatus>)> =
        tokio::task::JoinSet::new();
    let mut pids: Vec<Option<u32>> = Vec::with_capacity(count);
    for slot in 0..count {
        let (pid, mut handle) = spawn_worker(&exe, &spawn)?;
        waits.spawn(async move { (slot, handle.wait().await) });
        pids.push(Some(pid));
    }
    tracing::info!(workers = count, fd = spawn.listener_fd, "prefork supervisor started");

    let mut sigterm =
        signal(SignalKind::terminate()).context("failed to install SIGTERM handler")?;
    let mut sigint = signal(SignalKind::interrupt()).context("failed to install SIGINT handler")?;

    let mut drain_deadline: Option<tokio::time::Instant> = None;

    // Auto-scaling state. A `NoProbe` (always 0.0) is used when no policy is
    // supplied so the rest of the loop can stay uniform; `scaling_enabled` gates
    // the actual scaling branch.
    let (policy, probe) = policy
        .unwrap_or_else(|| (ScalingPolicy::default(), Arc::new(NoProbe) as Arc<dyn LoadProbe>));
    // Slots being scaled down on purpose: do not auto-restart them.
    let mut stopping: HashSet<usize> = HashSet::new();
    let mut last_scale = tokio::time::Instant::now()
        .checked_sub(policy.cooldown)
        .unwrap_or_else(tokio::time::Instant::now);
    let mut scale_tick =
        tokio::time::interval(std::cmp::max(Duration::from_secs(2), policy.cooldown / 2));
    scale_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let scaling_enabled = policy.max_workers > policy.min_workers;

    loop {
        tokio::select! {
            exited = waits.join_next() => {
                match exited {
                    Some(Ok((slot, status_res))) => {
                        pids[slot] = None;
                        // A slot we intentionally scaled down is not restarted.
                        if stopping.remove(&slot) {
                            tracing::info!(worker = slot, "scaled-down worker exited");
                            continue;
                        }
                        match status_res {
                            Ok(status) => {
                                if shutdown.is_cancelled() {
                                    tracing::info!(worker = slot, %status, "worker exited during shutdown");
                                } else {
                                    tracing::warn!(worker = slot, %status, "worker died; restarting");
                                    let (pid, mut handle) = spawn_worker(&exe, &spawn)?;
                                    waits.spawn(async move { (slot, handle.wait().await) });
                                    pids[slot] = Some(pid);
                                }
                            }
                            Err(e) => {
                                tracing::error!(worker = slot, error = %e, "worker wait error");
                            }
                        }
                    }
                    Some(Err(e)) => {
                        tracing::error!(error = %e, "worker join error");
                    }
                    None => {
                        // No live children to watch.
                        if shutdown.is_cancelled() {
                            break;
                        }
                    }
                }
            }
            _ = shutdown.cancelled() => {
                if drain_deadline.is_none() {
                    drain_deadline = Some(tokio::time::Instant::now() + spawn.drain_timeout);
                    tracing::info!(drain_secs = spawn.drain_timeout.as_secs(), "shutdown signal; draining workers");
                    signal_children(&pids);
                }
            }
            _ = sigterm.recv() => {
                if !shutdown.is_cancelled() { shutdown.cancel(); }
            }
            _ = sigint.recv() => {
                if !shutdown.is_cancelled() { shutdown.cancel(); }
            }
            _ = scale_tick.tick(), if scaling_enabled && !shutdown.is_cancelled() => {
                let load = probe.sample();
                let active = pids
                    .iter()
                    .enumerate()
                    .filter(|(s, p)| p.is_some() && !stopping.contains(s))
                    .count();
                let desired = decide_scale(active, load, &policy);
                if desired != active
                    && tokio::time::Instant::now().duration_since(last_scale) >= policy.cooldown
                {
                    last_scale = tokio::time::Instant::now();
                    if desired > active {
                        // Scale up: spawn up to `step` new workers into free slots.
                        let mut to_add = (desired - active).min(policy.step);
                        while to_add > 0 {
                            let slot = next_free_slot(&mut pids);
                            let (pid, mut handle) = match spawn_worker(&exe, &spawn) {
                                Ok(w) => w,
                                Err(e) => {
                                    tracing::error!(error = %e, "failed to scale up");
                                    break;
                                }
                            };
                            waits.spawn(async move { (slot, handle.wait().await) });
                            pids[slot] = Some(pid);
                            to_add -= 1;
                        }
                        tracing::info!(load = format!("{load:.2}"), workers = desired, "scaled up");
                    } else {
                        // Scale down: gracefully SIGTERM up to `step` live, non-
                        // stopping workers (picking the highest slots first).
                        let mut to_remove = (active - desired).min(policy.step);
                        for slot in (0..pids.len()).rev() {
                            if to_remove == 0 {
                                break;
                            }
                            if pids[slot].is_some() && !stopping.contains(&slot) {
                                stopping.insert(slot);
                                signal_one(pids[slot]);
                                to_remove -= 1;
                            }
                        }
                        tracing::info!(load = format!("{load:.2}"), workers = desired, "scaled down");
                    }
                }
            }
        }

        if let Some(deadline) = drain_deadline {
            // A slot being scaled down counts as "done" once it is marked
            // stopping, even if its process hasn't fully exited yet (shutdown
            // SIGTERM has already been sent to the whole fleet).
            let all_exited =
                pids.iter().enumerate().all(|(s, p)| p.is_none() || stopping.contains(&s));
            if all_exited {
                tracing::info!("all workers exited cleanly");
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                tracing::warn!("drain timeout exceeded; forcibly reaping workers");
                signal_kill(&pids);
                // Drain remaining wait results so JoinSet empties.
                while waits.join_next().await.is_some() {}
                break;
            }
        }
    }

    Ok(())
}

/// Spawn a single worker child that inherits the listening socket fd.
/// Returns `(pid, wait_handle)` so the supervisor can signal without holding the
/// `Child` (which is consumed by the JoinSet await).
fn spawn_worker(
    exe: &std::path::Path,
    spawn: &WorkerSpawn,
) -> anyhow::Result<(u32, tokio::process::Child)> {
    let mut args: Vec<String> = spawn.argv.clone();
    args.push("--worker-fd".to_string());
    args.push(spawn.listener_fd.to_string());

    let mut cmd = tokio::process::Command::new(exe);
    cmd.args(&args).stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());

    let child = cmd.spawn().context("failed to spawn worker process")?;
    let pid = child.id().ok_or_else(|| anyhow::anyhow!("spawned worker had no pid"))?;
    Ok((pid, child))
}

/// Forward SIGTERM to all live workers so they begin graceful drain.
fn signal_children(pids: &[Option<u32>]) {
    for pid in pids.iter().flatten() {
        // SAFETY: pid is a live child we spawned; SIGTERM is the defined
        // graceful-shutdown signal.
        let r = unsafe { libc::kill(*pid as libc::pid_t, libc::SIGTERM) };
        if r != 0 {
            tracing::warn!("failed to signal worker pid {pid} (errno {r})");
        }
    }
}

/// Forcefully kill all live workers (drain timeout exceeded).
fn signal_kill(pids: &[Option<u32>]) {
    for pid in pids.iter().flatten() {
        // SAFETY: pid is a live child we spawned; SIGKILL forcibly reaps it.
        let r = unsafe { libc::kill(*pid as libc::pid_t, libc::SIGKILL) };
        if r != 0 {
            tracing::warn!("failed to kill worker pid {pid} (errno {r})");
        }
    }
}

/// Find a free slot index in the `pids` table (a `None` entry) or grow it by
/// one. Used when scaling up so workers keep stable slot identities.
fn next_free_slot(pids: &mut Vec<Option<u32>>) -> usize {
    if let Some(slot) = pids.iter().position(|p| p.is_none()) {
        return slot;
    }
    pids.push(None);
    pids.len() - 1
}

/// Gracefully terminate a single worker by pid (used for scale-down).
fn signal_one(pid: Option<u32>) {
    if let Some(pid) = pid {
        // SAFETY: pid is a live child we spawned; SIGTERM begins graceful drain.
        let r = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        if r != 0 {
            tracing::warn!("failed to signal worker pid {pid} (errno {r})");
        }
    }
}

/// Build the `WorkerSpawn` payload from the parent's argv + bound fd.
pub fn make_spawn_argv(
    base_argv: &[String],
    listener_fd: i32,
    drain_timeout: Duration,
) -> WorkerSpawn {
    WorkerSpawn { argv: base_argv.to_vec(), listener_fd, drain_timeout }
}

// ---------------------------------------------------------------------------
// Auto-scaling
// ---------------------------------------------------------------------------
//
// When `--scale` is enabled the supervisor adjusts the live worker count between
// `min_workers` and `max_workers` in response to system load (default probe: the
// 1-minute load average, normalized by available parallelism). This keeps a
// small idle fleet when quiet and sheds load to more processes when busy,
// without a human editing `--workers` at runtime. The decision logic is a pure
// function (`decide_scale`) so it is unit-testable without spawning processes.

/// A sample of system load in the `[0, 1+]` range, where `1.0` means "fully
/// saturated relative to available CPU" and values above `1.0` mean overload.
pub trait LoadProbe: Send + Sync {
    /// Returns the current normalized load (load-average / parallelism).
    fn sample(&self) -> f64;
}

/// Default probe: reads the 1-minute load average via `getloadavg(3)` and
/// normalizes by the number of available CPUs. On non-Linux/non-unix or when
/// `getloadavg` is unavailable, it falls back to `0.0` (no scaling pressure).
#[cfg(unix)]
pub struct SystemLoadProbe {
    parallelism: f64,
}

#[cfg(unix)]
impl SystemLoadProbe {
    pub fn new() -> Self {
        let parallelism =
            std::thread::available_parallelism().map(|n| n.get().max(1) as f64).unwrap_or(1.0);
        Self { parallelism }
    }
}

#[cfg(unix)]
impl LoadProbe for SystemLoadProbe {
    fn sample(&self) -> f64 {
        // SAFETY: `getloadavg` writes at most 3 `f64`s into the 3-element array
        // we provide; it returns the count written (0 on failure).
        let mut load: [f64; 3] = [0.0; 3];
        let n = unsafe { libc::getloadavg(load.as_mut_ptr(), 3) };
        if n <= 0 {
            return 0.0;
        }
        // Index 0 is the 1-minute average.
        (load[0] / self.parallelism).max(0.0)
    }
}

/// Fallback probe used when no scaling policy is supplied. Reports zero load so
/// the scaling branch is a no-op (the fleet stays at its fixed size).
struct NoProbe;

impl LoadProbe for NoProbe {
    fn sample(&self) -> f64 {
        0.0
    }
}

/// Auto-scaling policy. `low`/`high` are normalized-load thresholds (`0..=1+`).
/// Below `low` the fleet shrinks toward `min_workers`; above `high` it grows
/// toward `max_workers`. `cooldown` prevents flapping; `step` caps how many
/// workers are added/removed per decision.
#[derive(Debug, Clone)]
pub struct ScalingPolicy {
    pub min_workers: usize,
    pub max_workers: usize,
    /// Scale down when normalized load is at or below this.
    pub low: f64,
    /// Scale up when normalized load is at or above this.
    pub high: f64,
    /// Minimum time between scaling actions.
    pub cooldown: Duration,
    /// Max workers to add/remove in a single decision.
    pub step: usize,
}

impl Default for ScalingPolicy {
    fn default() -> Self {
        Self {
            min_workers: 1,
            max_workers: 4,
            low: 0.3,
            high: 0.7,
            cooldown: Duration::from_secs(15),
            step: 1,
        }
    }
}

/// Pure scaling decision: given the current live worker count and the latest
/// normalized load sample, return the desired worker count (clamped to
/// `[min_workers, max_workers]`).
///
/// - load >= high  -> scale up by `step` (toward max)
/// - load <= low   -> scale down by `step` (toward min)
/// - otherwise     -> hold
pub fn decide_scale(current: usize, load: f64, policy: &ScalingPolicy) -> usize {
    let desired = if load >= policy.high {
        current.saturating_add(policy.step)
    } else if load <= policy.low {
        current.saturating_sub(policy.step)
    } else {
        current
    };
    desired.clamp(policy.min_workers, policy.max_workers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(min: usize, max: usize, low: f64, high: f64) -> ScalingPolicy {
        ScalingPolicy {
            min_workers: min,
            max_workers: max,
            low,
            high,
            cooldown: Duration::from_secs(15),
            step: 1,
        }
    }

    #[test]
    fn decide_scale_holds_in_band() {
        let p = policy(2, 8, 0.3, 0.7);
        assert_eq!(decide_scale(4, 0.5, &p), 4, "mid-band holds");
    }

    #[test]
    fn decide_scale_up_at_high() {
        let p = policy(2, 8, 0.3, 0.7);
        assert_eq!(decide_scale(4, 0.7, &p), 5, "at/above high scales up");
        assert_eq!(decide_scale(4, 0.9, &p), 5, "above high scales up");
    }

    #[test]
    fn decide_scale_down_at_low() {
        let p = policy(2, 8, 0.3, 0.7);
        assert_eq!(decide_scale(4, 0.3, &p), 3, "at/below low scales down");
        assert_eq!(decide_scale(4, 0.1, &p), 3, "below low scales down");
    }

    #[test]
    fn decide_scale_clamps_to_bounds() {
        let p = policy(2, 8, 0.3, 0.7);
        assert_eq!(decide_scale(8, 1.0, &p), 8, "cannot exceed max");
        assert_eq!(decide_scale(2, 0.0, &p), 2, "cannot drop below min");
    }

    #[test]
    fn decide_scale_respects_step() {
        let mut p = policy(1, 10, 0.3, 0.7);
        p.step = 2;
        assert_eq!(decide_scale(3, 0.9, &p), 5, "step of 2 adds 2");
        assert_eq!(decide_scale(3, 0.1, &p), 1, "step of 2 removes 2 (clamped to min)");
    }

    #[test]
    fn next_free_slot_reuses_and_grows() {
        let mut pids: Vec<Option<u32>> = vec![Some(1), None, Some(3)];
        assert_eq!(next_free_slot(&mut pids), 1);
        pids[1] = Some(9);
        // No free slot now -> grows.
        assert_eq!(next_free_slot(&mut pids), 3);
        assert_eq!(pids.len(), 4);
    }

    #[test]
    fn spawn_argv_carries_fd_and_args() {
        let base = vec!["serve".to_string(), "--addr".to_string(), "0:8080".to_string()];
        let spawn = make_spawn_argv(&base, 9, Duration::from_secs(5));
        assert_eq!(spawn.listener_fd, 9);
        assert_eq!(spawn.drain_timeout, Duration::from_secs(5));
        // The child re-exec must receive the original args verbatim.
        assert_eq!(spawn.argv, base);
    }

    #[test]
    fn bind_listener_binds_and_is_nonblocking() {
        // Use a guaranteed-free ephemeral port.
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = bind_listener(addr).expect("bind should succeed");
        // Re-binding the same (now-allocated) ephemeral port must fail, proving
        // the listener is actually bound and unique.
        let again = std::net::TcpListener::bind(listener.local_addr().unwrap());
        assert!(again.is_err(), "ephemeral port must be occupied by our listener");
    }
}
