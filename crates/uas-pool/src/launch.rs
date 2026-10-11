//! Worker launch with clamping and audit tracking (spec 0015 D6, spec 0014 D4 and D14).
//!
//! [`spawn_worker`] starts a worker, clamps it, and registers its pid with
//! `uas-ebpfd`. [`Worker::finish`] waits for the worker and seals its receipt.
//! No daemon socket means the worker still runs, unattested.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::Duration;

use uas_domain::{AuditStatus, LimitEvents, WorkerOutcome};
use uas_ebpf::{clamp::enter_clamped_group, client::DaemonClient, EbpfError};

use crate::error::PoolError;
use crate::freezer::parse_event_counter;

/// Where `uas-ebpfd` listens on a production host (spec 0014 D8).
pub const DEFAULT_DAEMON_SOCKET: &str = "/run/uas/ebpf.sock";

/// How long one daemon request may wait before the worker runs unattested.
pub const DEFAULT_DAEMON_TIMEOUT: Duration = Duration::from_secs(2);

/// How the pool reaches the audit daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchConfig {
    /// The daemon socket. `None` runs every worker unattested.
    pub daemon_socket: Option<PathBuf>,
    /// The read and write timeout of every daemon request.
    pub daemon_timeout: Duration,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            daemon_socket: None,
            daemon_timeout: DEFAULT_DAEMON_TIMEOUT,
        }
    }
}

/// Why a worker runs without an audit track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditDegradation {
    /// No daemon socket is configured.
    NoSocket,
    /// The socket exists but the daemon could not be reached.
    Unavailable(String),
    /// The daemon answered and refused the track request.
    Refused(String),
}

/// The audit facts of one worker run. The landing reads them as its input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerAudit {
    /// Hex SHA-256 of the transcript, when the daemon sealed one.
    pub run_hash: Option<String>,
    /// `Attested` only when the worker was tracked and no record was dropped.
    pub audit: AuditStatus,
    /// The CPU cap of the slot the worker ran in.
    pub cpu_cap: String,
    /// Why the worker ran without a track, when it did.
    pub degradation: Option<AuditDegradation>,
    /// The `oom_kill` and `pids.max` counters that grew while the worker ran
    /// (spec 0041 REQ-06). Zero when the worker had no slot cgroup.
    pub limit_events: LimitEvents,
    /// How the worker ended: an exit code, or killed by the OOM killer (REQ-07).
    pub outcome: WorkerOutcome,
}

/// The slot cgroup a worker is enrolled in (spec 0041 REQ-02 and 08).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotCgroup {
    /// The slot cgroup directory.
    pub path: PathBuf,
    /// The cumulative counters before the worker ran; a slot serves many workers.
    pub baseline: LimitEvents,
}

/// Reads the two cumulative counters of the cgroup at `dir`. A missing or
/// unparsable file reads as zero and is logged.
fn read_events(dir: &Path) -> LimitEvents {
    let read = |file: &str, key: &str| {
        let path = dir.join(file);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| parse_event_counter(&t, key))
            .unwrap_or_else(|| {
                tracing::warn!(path = ?path, "cgroup events file unreadable; counter read as zero");
                0
            })
    };
    LimitEvents {
        oom_kill: read("memory.events", "oom_kill"),
        pids_max_hits: read("pids.events", "max"),
    }
}

/// The exit code of `status`; a death by signal is `128 + signal`.
fn exit_code(status: &ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// The daemon connection that owns one worker's receipt window.
#[derive(Debug)]
struct Tracking {
    client: DaemonClient,
    pid: u32,
    /// The dropped total before the worker ran. `None` when the read failed.
    baseline: Option<u64>,
}

/// A clamped worker process and its audit track.
#[derive(Debug)]
pub struct Worker {
    child: Child,
    tracking: Option<Tracking>,
    cpu_cap: String,
    degradation: Option<AuditDegradation>,
    cgroup: Option<SlotCgroup>,
}

impl Worker {
    /// The pid of the worker process.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Waits for the worker, seals its receipt and forgets the window; the
    /// kernel entry leaves at the worker's exit.
    ///
    /// The receipt is `Attested` only when the worker was tracked, the daemon
    /// sealed it, and no record was dropped. Dropped records come from the
    /// receipt or from the counter growth during the run, whichever is larger.
    pub fn finish(mut self) -> Result<(ExitStatus, WorkerAudit), PoolError> {
        let status = self.child.wait()?;
        let mut audit = WorkerAudit {
            run_hash: None,
            audit: AuditStatus::Unattested { dropped_events: 0 },
            cpu_cap: self.cpu_cap,
            degradation: self.degradation,
            limit_events: LimitEvents::default(),
            outcome: WorkerOutcome::Exited(exit_code(&status)),
        };
        if let Some(cgroup) = &self.cgroup {
            let now = read_events(&cgroup.path);
            audit.limit_events = LimitEvents {
                oom_kill: now.oom_kill.saturating_sub(cgroup.baseline.oom_kill),
                pids_max_hits: now
                    .pids_max_hits
                    .saturating_sub(cgroup.baseline.pids_max_hits),
            };
            audit.outcome = WorkerOutcome::classify(exit_code(&status), &audit.limit_events);
        }
        if let Some(mut tracking) = self.tracking.take() {
            let end = tracking.client.status().ok().map(|(_, dropped)| dropped);
            let grown = match (tracking.baseline, end) {
                (Some(before), Some(after)) => after.saturating_sub(before),
                _ => 0,
            };
            let receipt = tracking.client.receipt(tracking.pid);
            if let Err(e) = tracking.client.forget(tracking.pid) {
                eprintln!("uas-pool: forget of pid {} failed: {e}", tracking.pid);
            }
            match receipt {
                Ok(frame) => {
                    let dropped = grown.max(frame.dropped_events);
                    audit.run_hash = Some(frame.run_hash);
                    audit.audit = if dropped == 0 {
                        AuditStatus::Attested
                    } else {
                        AuditStatus::Unattested {
                            dropped_events: dropped,
                        }
                    };
                }
                Err(e) => {
                    tracing::warn!(error = %e, "receipt unavailable; the run is unattested");
                    audit.audit = AuditStatus::Unattested {
                        dropped_events: grown,
                    };
                    audit.degradation = Some(AuditDegradation::Unavailable(e.to_string()));
                }
            }
        }
        Ok((status, audit))
    }
}

/// Spawns `cmd` clamped to `nice 19` in its own process group, then tracks it.
///
/// Only a failed spawn is an error. Every audit failure becomes a degradation,
/// so the worker still runs.
///
/// With a `cgroup`, the child writes its own pid to `cgroup.procs` before
/// `exec`, so the exec'd process already runs under the slot limits (spec 0041
/// REQ-08, spec 0020 D3). Enrolment fails closed: if `cgroup.procs` cannot be
/// opened or written, nothing is executed and the spawn is an error.
pub fn spawn_worker(
    mut cmd: Command,
    cfg: &LaunchConfig,
    cpu_cap: String,
    cgroup: Option<SlotCgroup>,
) -> Result<Worker, PoolError> {
    if let Some(cgroup) = &cgroup {
        let mut procs = OpenOptions::new()
            .write(true)
            .open(cgroup.path.join("cgroup.procs"))?;
        // SAFETY: the hook runs in the child between fork and exec. It makes one
        // write(2) of a constant to a descriptor opened before the fork and
        // allocates nothing, which is what pre_exec requires.
        #[allow(unsafe_code)]
        unsafe {
            cmd.pre_exec(move || procs.write_all(b"0"));
        }
    }
    // SAFETY: the hook runs in the child between fork and exec. It makes two raw
    // syscalls (setpgid, setpriority) that touch only the child, and it allocates
    // nothing, which is what pre_exec requires.
    #[allow(unsafe_code)]
    unsafe {
        cmd.pre_exec(enter_clamped_group);
    }
    let child = cmd.spawn()?;
    let pid = child.id();
    let (tracking, degradation) = attach(cfg, pid);
    if let Some(reason) = &degradation {
        tracing::warn!(pid, ?reason, "worker runs without an audit track");
    }
    Ok(Worker {
        child,
        tracking,
        cpu_cap,
        degradation,
        cgroup,
    })
}

/// Connects, tracks `pid` and reads the baseline. A failed connect or track is
/// a degradation, never an error: the worker keeps running unattested.
fn attach(cfg: &LaunchConfig, pid: u32) -> (Option<Tracking>, Option<AuditDegradation>) {
    let Some(path) = &cfg.daemon_socket else {
        return (None, Some(AuditDegradation::NoSocket));
    };
    let mut client = match DaemonClient::connect_with_timeout(path, cfg.daemon_timeout) {
        Ok(client) => client,
        Err(e) => return (None, Some(AuditDegradation::Unavailable(e.to_string()))),
    };
    // Track first: the daemon refuses a foreign peer on its first request.
    if let Err(e) = client.track(pid, std::process::id()) {
        let silent = matches!(
            &e,
            EbpfError::Io(io) if matches!(io.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock)
        );
        return (
            None,
            Some(if silent {
                AuditDegradation::Unavailable(e.to_string())
            } else {
                AuditDegradation::Refused(e.to_string())
            }),
        );
    }
    let baseline = client.status().ok().map(|(_, dropped)| dropped);
    (
        Some(Tracking {
            client,
            pid,
            baseline,
        }),
        None,
    )
}
