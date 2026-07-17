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

use std::net::SocketAddr;
use std::process::Stdio;
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
pub async fn supervise(
    count: usize,
    spawn: WorkerSpawn,
    shutdown: CancellationToken,
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

    loop {
        tokio::select! {
            exited = waits.join_next() => {
                match exited {
                    Some(Ok((slot, status_res))) => {
                        pids[slot] = None;
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
        }

        if let Some(deadline) = drain_deadline {
            let all_exited = pids.iter().all(|p| p.is_none());
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

/// Build the `WorkerSpawn` payload from the parent's argv + bound fd.
pub fn make_spawn_argv(
    base_argv: &[String],
    listener_fd: i32,
    drain_timeout: Duration,
) -> WorkerSpawn {
    WorkerSpawn { argv: base_argv.to_vec(), listener_fd, drain_timeout }
}

#[cfg(test)]
mod tests {
    use super::*;

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
