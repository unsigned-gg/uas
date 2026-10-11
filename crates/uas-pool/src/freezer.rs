// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Linux cgroup v2 freezer and process suspension controller.
//!
//! CER-2722: Manages cgroup v2 hierarchy under a delegated subtree (e.g.
//! `/sys/fs/cgroup/uas` or `<delegated-base>`) with sub-5ms freeze/unfreeze
//! lifecycle.
//!
//! # Safety Invariant
//!
//! A pool process MUST NEVER freeze its own cgroup (on `ceres`, that cgroup is
//! `herdr-estate.service`, and freezing it halts the entire session host).
//! All operations are strictly constrained to ephemeral child cgroups `slot-<id>`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use uas_domain::{LimitEvents, SlotId};

use crate::error::FreezerError;
use crate::limits::ResolvedLimits;

/// Operational state of a freezer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreezerState {
    /// Worker processes are actively executing.
    Thawed,
    /// Worker processes are paused / frozen in execution.
    Frozen,
}

/// Recorded CPU cap status for a worker slot (Spec 0020).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuCap {
    /// Enforced quota and period line, e.g. "200000 100000".
    Capped(String),
    /// Max quota without throttling.
    Max,
    /// Uncapped because controller or quota is absent.
    #[default]
    Uncapped,
}

impl CpuCap {
    /// Returns the string representation suitable for receipt recording.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Capped(s) => s.as_str(),
            Self::Max => "max",
            Self::Uncapped => "uncapped",
        }
    }
}

impl std::fmt::Display for CpuCap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<CpuCap> for String {
    fn from(cap: CpuCap) -> Self {
        cap.as_str().to_string()
    }
}

impl From<&CpuCap> for String {
    fn from(cap: &CpuCap) -> Self {
        cap.as_str().to_string()
    }
}

/// Resource constraints configured on child cgroups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// CPU quota in microseconds per period (None means "max").
    pub cpu_quota_us: Option<u64>,
    /// CPU period in microseconds (defaults to 100,000us i.e. 100ms).
    pub cpu_period_us: Option<u64>,
    /// Memory high watermark in bytes (throttles/reclaims beyond this).
    pub memory_high_bytes: Option<u64>,
    /// Memory hard ceiling in bytes (triggers OOM kill beyond this).
    pub memory_max_bytes: Option<u64>,
    /// Swap ceiling in bytes; `Some(0)` forbids swap (spec 0041 D1).
    pub memory_swap_max_bytes: Option<u64>,
    /// Maximum number of tasks in the slot cgroup (spec 0041 D2).
    pub pids_max: Option<u32>,
}

impl From<&ResolvedLimits> for ResourceLimits {
    /// Carry the resolved memory, swap and pids values; CPU keeps its default quota.
    fn from(r: &ResolvedLimits) -> Self {
        Self {
            memory_high_bytes: r.memory_high_bytes,
            memory_max_bytes: r.memory_max_bytes,
            memory_swap_max_bytes: Some(r.memory_swap_max_bytes),
            pids_max: Some(r.pids_max),
            ..Self::default()
        }
    }
}

impl Default for ResourceLimits {
    /// Default limits: 2 CPUs of quota (200,000us quota per 100,000us period, Spec 0020 D3).
    fn default() -> Self {
        Self {
            cpu_quota_us: Some(200_000),
            cpu_period_us: Some(100_000),
            memory_high_bytes: None,
            memory_max_bytes: None,
            memory_swap_max_bytes: Some(0),
            pids_max: Some(crate::limits::DEFAULT_PIDS_MAX),
        }
    }
}

/// Common interface for process suspension backends.
pub trait Freezer: Send + Sync {
    /// Freeze all worker processes in this slot.
    fn freeze(&self) -> Result<(), FreezerError>;

    /// Thaw and resume all worker processes in this slot.
    fn unfreeze(&self) -> Result<(), FreezerError>;

    /// Read the current operational state of the freezer.
    fn state(&self) -> Result<FreezerState, FreezerError>;

    /// Enroll a worker process PID into the freezer.
    fn add_pid(&self, pid: u32) -> Result<(), FreezerError>;

    /// Enumerate all process IDs assigned to this freezer.
    fn pids(&self) -> Result<Vec<u32>, FreezerError>;

    /// Read the currently recorded CPU cap status.
    fn cpu_cap(&self) -> CpuCap;

    /// Configure resource constraints on this freezer if supported.
    fn configure_limits(&self, limits: &ResourceLimits) -> Result<CpuCap, FreezerError> {
        let _ = limits;
        Ok(CpuCap::Uncapped)
    }

    /// The slot cgroup directory, when this backend has one.
    fn cgroup_path(&self) -> Option<PathBuf> {
        None
    }

    /// The slot cgroup limit counters, when this backend has a cgroup.
    fn limit_events(&self) -> Option<LimitEvents> {
        None
    }
}

/// Resolves the current process's cgroup path relative to `/sys/fs/cgroup`.
pub fn current_process_cgroup_path() -> Result<PathBuf, FreezerError> {
    let content = fs::read_to_string("/proc/self/cgroup").map_err(|e| FreezerError::Io {
        path: PathBuf::from("/proc/self/cgroup"),
        source: e,
    })?;

    for line in content.lines() {
        if let Some(rel) = line.strip_prefix("0::") {
            let trimmed = rel.trim_start_matches('/');
            return Ok(PathBuf::from("/sys/fs/cgroup").join(trimmed));
        }
    }

    Ok(PathBuf::from("/sys/fs/cgroup"))
}

/// Default bound on waiting for an orphan cgroup to empty after `cgroup.kill` (spec 0040 REQ-06).
pub const DEFAULT_RECLAIM_TIMEOUT: Duration = Duration::from_secs(5);

/// Kills and removes an orphan slot cgroup in the order of spec 0040 D3.
///
/// Writes `1` to `cgroup.kill`, polls `cgroup.events` until `populated 0` (bounded by
/// `timeout`), then `rmdir`s every descendant cgroup deepest first and the slot cgroup last.
/// A missing cgroup, `cgroup.kill` or `cgroup.events` is not an error. A cgroup still
/// populated at `timeout` returns [`FreezerError::ReclaimTimeout`] and removes nothing.
///
/// Refuses a symlink and any cgroup that equals or contains the caller's own cgroup.
pub fn reclaim_cgroup(slot_cgroup: &Path, timeout: Duration) -> Result<(), FreezerError> {
    let own = current_process_cgroup_path().ok();
    reclaim_cgroup_guarded(slot_cgroup, timeout, own.as_deref())
}

fn reclaim_cgroup_guarded(
    slot_cgroup: &Path,
    timeout: Duration,
    own_cgroup: Option<&Path>,
) -> Result<(), FreezerError> {
    let io = |path: &Path, source| FreezerError::Io {
        path: path.to_path_buf(),
        source,
    };

    if let Some(own) = own_cgroup {
        if own.starts_with(slot_cgroup) {
            return Err(FreezerError::UnsafePath(format!(
                "slot cgroup {slot_cgroup:?} equals or contains own process cgroup {own:?}"
            )));
        }
    }
    match fs::symlink_metadata(slot_cgroup) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            return Err(FreezerError::UnsafePath(format!(
                "slot cgroup {slot_cgroup:?} is not a plain directory"
            )));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io(slot_cgroup, e)),
    }

    // 1. Kill the whole subtree. A kernel without cgroup.kill has no such file.
    let kill_path = slot_cgroup.join("cgroup.kill");
    match OpenOptions::new()
        .write(true)
        .open(&kill_path)
        .and_then(|mut f| f.write_all(b"1\n"))
    {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(&kill_path, e)),
    }

    // 2. Wait for the subtree to empty. A missing cgroup.events counts as unpopulated.
    let events_path = slot_cgroup.join("cgroup.events");
    let start = Instant::now();
    loop {
        match fs::read_to_string(&events_path) {
            Ok(content) => {
                if !content.lines().any(|l| l.trim() == "populated 1") {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => return Err(io(&events_path, e)),
        }
        if start.elapsed() >= timeout {
            return Err(FreezerError::ReclaimTimeout { path: events_path });
        }
        thread::sleep(Duration::from_millis(5));
    }

    // 3. Remove descendants deepest first, then the slot cgroup.
    let mut dirs = Vec::new();
    collect_cgroup_dirs(slot_cgroup, &mut dirs).map_err(|(p, e)| io(&p, e))?;
    for dir in dirs.iter().rev() {
        match fs::remove_dir(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(dir, e)),
        }
    }
    Ok(())
}

/// Pre-order list of `root` and every plain subdirectory; symlinks are not followed.
fn collect_cgroup_dirs(
    root: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), (PathBuf, std::io::Error)> {
    out.push(root.to_path_buf());
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err((root.to_path_buf(), e)),
    };
    for entry in entries {
        let entry = entry.map_err(|e| (root.to_path_buf(), e))?;
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            collect_cgroup_dirs(&entry.path(), out)?;
        }
    }
    Ok(())
}

/// Linux cgroup v2 freezer controller for a single worker slot.
#[derive(Debug)]
pub struct CgroupFreezer {
    base_cgroup: PathBuf,
    slot_cgroup: PathBuf,
    slot_id: SlotId,
    cpu_cap: Mutex<CpuCap>,
    events_warned: AtomicBool,
}

/// Value of the line `key <n>` in a cgroup events file (`memory.events`, `pids.events`).
pub fn parse_event_counter(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        (parts.next()? == key).then(|| parts.next()?.parse().ok())?
    })
}

impl CgroupFreezer {
    /// Initialize a child cgroup freezer under `base_cgroup`.
    ///
    /// The ephemeral child cgroup directory `slot-<id>` is created if not already present.
    pub fn new(base_cgroup: impl Into<PathBuf>, slot_id: SlotId) -> Result<Self, FreezerError> {
        Self::with_limits(base_cgroup, slot_id, &ResourceLimits::default())
    }

    /// Like [`Self::new`], writing `limits` in the same call, before any worker exists
    /// (spec 0041 D4).
    pub fn with_limits(
        base_cgroup: impl Into<PathBuf>,
        slot_id: SlotId,
        limits: &ResourceLimits,
    ) -> Result<Self, FreezerError> {
        let base_cgroup = base_cgroup.into();
        let slot_cgroup = base_cgroup.join(format!("slot-{}", slot_id));

        Self::verify_safety(&base_cgroup, &slot_cgroup)?;

        if !slot_cgroup.exists() {
            fs::create_dir_all(&slot_cgroup).map_err(|e| FreezerError::Io {
                path: slot_cgroup.clone(),
                source: e,
            })?;
        }

        let freezer = Self {
            base_cgroup,
            slot_cgroup,
            slot_id,
            cpu_cap: Mutex::new(CpuCap::Uncapped),
            events_warned: AtomicBool::new(false),
        };

        // Default limits: 2 CPUs of quota per Spec 0020 D3
        if let Err(e) = freezer.configure_limits(limits) {
            tracing::error!(slot_id = %freezer.slot_id, error = %e, "cgroup limit write failed; slot not created");
            return Err(e);
        }

        Ok(freezer)
    }

    /// Verifies that `slot_cgroup` is safe to create and freeze.
    pub fn verify_safety(base_cgroup: &Path, slot_cgroup: &Path) -> Result<(), FreezerError> {
        let root = Path::new("/");
        let sys_cgroup = Path::new("/sys/fs/cgroup");

        if base_cgroup == root || base_cgroup == sys_cgroup {
            return Err(FreezerError::UnsafePath(format!(
                "base cgroup cannot be root or top-level /sys/fs/cgroup: {:?}",
                base_cgroup
            )));
        }

        if slot_cgroup == root || slot_cgroup == sys_cgroup {
            return Err(FreezerError::UnsafePath(format!(
                "slot cgroup cannot be root or top-level /sys/fs/cgroup: {:?}",
                slot_cgroup
            )));
        }

        if !slot_cgroup.starts_with(base_cgroup) || slot_cgroup == base_cgroup {
            return Err(FreezerError::UnsafePath(format!(
                "slot cgroup {:?} must be a strict child of base cgroup {:?}",
                slot_cgroup, base_cgroup
            )));
        }

        if let Ok(current_cg) = current_process_cgroup_path() {
            if slot_cgroup == current_cg {
                return Err(FreezerError::UnsafePath(format!(
                    "target slot cgroup matches own process cgroup {:?}",
                    slot_cgroup
                )));
            }
        }

        Ok(())
    }

    /// The slot id managed by this freezer.
    pub fn slot_id(&self) -> &SlotId {
        &self.slot_id
    }

    /// The absolute path to the child cgroup.
    pub fn path(&self) -> &Path {
        &self.slot_cgroup
    }

    /// The parent base cgroup path.
    pub fn base_path(&self) -> &Path {
        &self.base_cgroup
    }

    /// Read the currently recorded CPU cap status.
    pub fn cpu_cap(&self) -> CpuCap {
        self.cpu_cap
            .lock()
            .map(|c| c.clone())
            .unwrap_or(CpuCap::Uncapped)
    }

    /// Configures CPU and memory constraints in the child cgroup.
    pub fn configure_limits(&self, limits: &ResourceLimits) -> Result<CpuCap, FreezerError> {
        let mut recorded_cap = CpuCap::Uncapped;

        if limits.cpu_quota_us.is_some() || limits.cpu_period_us.is_some() {
            let period = limits.cpu_period_us.unwrap_or(100_000);
            let quota_str = limits
                .cpu_quota_us
                .map(|q| q.to_string())
                .unwrap_or_else(|| "max".to_string());
            let line = format!("{quota_str} {period}\n");

            let cpu_max_path = self.slot_cgroup.join("cpu.max");
            if cpu_max_path.exists() {
                fs::write(&cpu_max_path, line.as_bytes()).map_err(|e| FreezerError::Io {
                    path: cpu_max_path,
                    source: e,
                })?;
                recorded_cap = if quota_str == "max" {
                    CpuCap::Max
                } else {
                    CpuCap::Capped(format!("{quota_str} {period}"))
                };
            } else {
                tracing::warn!(
                    slot_id = %self.slot_id,
                    path = ?cpu_max_path,
                    "cgroup cpu.max controller not available; slot remains uncapped"
                );
                recorded_cap = CpuCap::Uncapped;
            }
        }

        self.write_limit("memory.max", limits.memory_max_bytes)?;
        self.write_limit("memory.high", limits.memory_high_bytes)?;
        self.write_limit("memory.swap.max", limits.memory_swap_max_bytes)?;
        self.write_limit("pids.max", limits.pids_max)?;

        if let Ok(mut lock) = self.cpu_cap.lock() {
            *lock = recorded_cap.clone();
        }

        Ok(recorded_cap)
    }

    /// Write one control file when `value` is set. A missing file logs at `warn` and leaves
    /// that limit unset, as `cpu.max` does (spec 0041 D4).
    fn write_limit(&self, name: &str, value: Option<impl ToString>) -> Result<(), FreezerError> {
        let Some(value) = value else { return Ok(()) };
        let path = self.slot_cgroup.join(name);
        if !path.exists() {
            tracing::warn!(
                slot_id = %self.slot_id,
                path = ?path,
                "cgroup {name} controller not available; limit left unset"
            );
            return Ok(());
        }
        fs::write(&path, format!("{}\n", value.to_string()).as_bytes())
            .map_err(|e| FreezerError::Io { path, source: e })
    }

    /// Read `oom_kill` from `memory.events` and `max` from `pids.events` (spec 0041 D5).
    /// A missing or unparsable file reads as zero and is logged once per freezer.
    pub fn read_limit_events(&self) -> LimitEvents {
        let read = |file: &str, key: &str| {
            let path = self.slot_cgroup.join(file);
            if let Some(n) = fs::read_to_string(&path)
                .ok()
                .and_then(|t| parse_event_counter(&t, key))
            {
                return n;
            }
            if !self.events_warned.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    slot_id = %self.slot_id,
                    path = ?path,
                    "cgroup events file unreadable; counter read as zero"
                );
            }
            0
        };
        LimitEvents {
            oom_kill: read("memory.events", "oom_kill"),
            pids_max_hits: read("pids.events", "max"),
        }
    }

    /// Poll `cgroup.events` until `frozen` matches `expected_val`, or timeout expires.
    fn poll_frozen_state(&self, expected_val: u32, timeout: Duration) -> Result<(), FreezerError> {
        let events_path = self.slot_cgroup.join("cgroup.events");
        let start = Instant::now();

        loop {
            let content = fs::read_to_string(&events_path).map_err(|e| FreezerError::Io {
                path: events_path.clone(),
                source: e,
            })?;

            for line in content.lines() {
                if let Some(val_str) = line.strip_prefix("frozen ") {
                    if let Ok(val) = val_str.trim().parse::<u32>() {
                        if val == expected_val {
                            return Ok(());
                        }
                    }
                }
            }

            if start.elapsed() >= timeout {
                return Err(FreezerError::Timeout {
                    path: events_path,
                    expected: expected_val,
                });
            }

            thread::sleep(Duration::from_micros(500));
        }
    }

    /// Tears down and cleans up the child cgroup.
    pub fn destroy(&self) -> Result<(), FreezerError> {
        let _ = self.unfreeze();

        if self.slot_cgroup.exists() {
            if let Err(e) = fs::remove_dir(&self.slot_cgroup) {
                if e.kind() == std::io::ErrorKind::DirectoryNotEmpty {
                    fs::remove_dir_all(&self.slot_cgroup).map_err(|e| FreezerError::Io {
                        path: self.slot_cgroup.clone(),
                        source: e,
                    })?;
                } else {
                    return Err(FreezerError::Io {
                        path: self.slot_cgroup.clone(),
                        source: e,
                    });
                }
            }
        }

        Ok(())
    }
}

impl Freezer for CgroupFreezer {
    fn freeze(&self) -> Result<(), FreezerError> {
        let freeze_path = self.slot_cgroup.join("cgroup.freeze");
        let mut file = OpenOptions::new()
            .write(true)
            .open(&freeze_path)
            .map_err(|e| FreezerError::Io {
                path: freeze_path.clone(),
                source: e,
            })?;

        file.write_all(b"1\n").map_err(|e| FreezerError::Io {
            path: freeze_path,
            source: e,
        })?;

        self.poll_frozen_state(1, Duration::from_millis(100))
    }

    fn unfreeze(&self) -> Result<(), FreezerError> {
        let freeze_path = self.slot_cgroup.join("cgroup.freeze");
        let mut file = OpenOptions::new()
            .write(true)
            .open(&freeze_path)
            .map_err(|e| FreezerError::Io {
                path: freeze_path.clone(),
                source: e,
            })?;

        file.write_all(b"0\n").map_err(|e| FreezerError::Io {
            path: freeze_path,
            source: e,
        })?;

        self.poll_frozen_state(0, Duration::from_millis(100))
    }

    fn state(&self) -> Result<FreezerState, FreezerError> {
        let events_path = self.slot_cgroup.join("cgroup.events");
        let content = fs::read_to_string(&events_path).map_err(|e| FreezerError::Io {
            path: events_path.clone(),
            source: e,
        })?;

        for line in content.lines() {
            if let Some(val_str) = line.strip_prefix("frozen ") {
                match val_str.trim() {
                    "1" => return Ok(FreezerState::Frozen),
                    "0" => return Ok(FreezerState::Thawed),
                    other => {
                        return Err(FreezerError::Parse {
                            path: events_path,
                            message: format!("unexpected frozen value '{other}'"),
                        })
                    }
                }
            }
        }

        Err(FreezerError::Parse {
            path: events_path,
            message: "no 'frozen' entry found in cgroup.events".to_string(),
        })
    }

    fn add_pid(&self, pid: u32) -> Result<(), FreezerError> {
        let procs_path = self.slot_cgroup.join("cgroup.procs");
        let mut file = OpenOptions::new()
            .write(true)
            .open(&procs_path)
            .map_err(|e| FreezerError::Io {
                path: procs_path.clone(),
                source: e,
            })?;

        let line = format!("{pid}\n");
        file.write_all(line.as_bytes())
            .map_err(|e| FreezerError::Io {
                path: procs_path,
                source: e,
            })
    }

    fn pids(&self) -> Result<Vec<u32>, FreezerError> {
        let procs_path = self.slot_cgroup.join("cgroup.procs");
        let content = fs::read_to_string(&procs_path).map_err(|e| FreezerError::Io {
            path: procs_path.clone(),
            source: e,
        })?;

        let mut res = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if let Ok(pid) = trimmed.parse::<u32>() {
                    res.push(pid);
                }
            }
        }

        Ok(res)
    }

    fn cpu_cap(&self) -> CpuCap {
        self.cpu_cap()
    }

    fn configure_limits(&self, limits: &ResourceLimits) -> Result<CpuCap, FreezerError> {
        self.configure_limits(limits)
    }

    fn cgroup_path(&self) -> Option<PathBuf> {
        Some(self.slot_cgroup.clone())
    }

    fn limit_events(&self) -> Option<LimitEvents> {
        Some(self.read_limit_events())
    }
}

/// Fallback freezer implementing process suspension via POSIX signals (SIGSTOP / SIGCONT).
///
/// Used per Spec 0015 D4 when cgroup v2 is not writable or available.
#[derive(Debug)]
pub struct SignalFreezer {
    slot_id: SlotId,
    pids: Mutex<Vec<u32>>,
    state: AtomicU8,
}

const STATE_THAWED: u8 = 0;
const STATE_FROZEN: u8 = 1;

impl SignalFreezer {
    /// Creates a signal-based freezer for the given slot.
    pub fn new(slot_id: SlotId) -> Self {
        Self {
            slot_id,
            pids: Mutex::new(Vec::new()),
            state: AtomicU8::new(STATE_THAWED),
        }
    }

    /// The slot id managed by this freezer.
    pub fn slot_id(&self) -> &SlotId {
        &self.slot_id
    }

    /// Read the currently recorded CPU cap status (always uncapped for signal freezer).
    pub fn cpu_cap(&self) -> CpuCap {
        CpuCap::Uncapped
    }
}

impl Freezer for SignalFreezer {
    fn freeze(&self) -> Result<(), FreezerError> {
        let pids = self
            .pids
            .lock()
            .map_err(|_| FreezerError::Signal("lock poisoned".to_string()))?;

        for &pid in pids.iter() {
            let raw_pid = pid as i32;
            if raw_pid > 1 {
                let proc_pid = rustix::process::Pid::from_raw(raw_pid);
                if let Some(pid_ref) = proc_pid {
                    let _ = rustix::process::kill_process(pid_ref, rustix::process::Signal::STOP);
                }
            }
        }

        self.state.store(STATE_FROZEN, Ordering::SeqCst);
        Ok(())
    }

    fn unfreeze(&self) -> Result<(), FreezerError> {
        let pids = self
            .pids
            .lock()
            .map_err(|_| FreezerError::Signal("lock poisoned".to_string()))?;

        for &pid in pids.iter() {
            let raw_pid = pid as i32;
            if raw_pid > 1 {
                let proc_pid = rustix::process::Pid::from_raw(raw_pid);
                if let Some(pid_ref) = proc_pid {
                    let _ = rustix::process::kill_process(pid_ref, rustix::process::Signal::CONT);
                }
            }
        }

        self.state.store(STATE_THAWED, Ordering::SeqCst);
        Ok(())
    }

    fn state(&self) -> Result<FreezerState, FreezerError> {
        match self.state.load(Ordering::SeqCst) {
            STATE_FROZEN => Ok(FreezerState::Frozen),
            _ => Ok(FreezerState::Thawed),
        }
    }

    fn add_pid(&self, pid: u32) -> Result<(), FreezerError> {
        let mut pids = self
            .pids
            .lock()
            .map_err(|_| FreezerError::Signal("lock poisoned".to_string()))?;
        if !pids.contains(&pid) {
            pids.push(pid);
        }
        Ok(())
    }

    fn pids(&self) -> Result<Vec<u32>, FreezerError> {
        let pids = self
            .pids
            .lock()
            .map_err(|_| FreezerError::Signal("lock poisoned".to_string()))?;
        Ok(pids.clone())
    }

    fn cpu_cap(&self) -> CpuCap {
        CpuCap::Uncapped
    }

    fn configure_limits(&self, _limits: &ResourceLimits) -> Result<CpuCap, FreezerError> {
        Ok(CpuCap::Uncapped)
    }
}

#[cfg(test)]
mod reclaim_tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("uas-reclaim-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn reclaim_removes_nested_cgroups_deepest_first() {
        let base = tree("nested");
        let slot = base.join("slot-a");
        fs::create_dir_all(slot.join("w1/w2")).unwrap();
        fs::create_dir_all(slot.join("w3")).unwrap();
        reclaim_cgroup_guarded(&slot, Duration::from_millis(100), None).unwrap();
        assert!(!slot.exists());
        assert!(base.exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn reclaim_missing_cgroup_is_ok() {
        let base = tree("missing");
        reclaim_cgroup_guarded(&base.join("slot-none"), Duration::from_millis(10), None).unwrap();
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn reclaim_times_out_on_populated_cgroup() {
        let base = tree("populated");
        let slot = base.join("slot-p");
        fs::create_dir_all(&slot).unwrap();
        fs::write(slot.join("cgroup.events"), "populated 1\nfrozen 0\n").unwrap();
        let t = Instant::now();
        let err = reclaim_cgroup_guarded(&slot, Duration::from_millis(100), None).unwrap_err();
        assert!(matches!(err, FreezerError::ReclaimTimeout { .. }));
        assert!(t.elapsed() < Duration::from_secs(2));
        assert!(slot.exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn reclaim_refuses_own_cgroup_and_ancestors() {
        let base = tree("own");
        let slot = base.join("slot-o");
        fs::create_dir_all(&slot).unwrap();
        let own = slot.join("child");
        let err = reclaim_cgroup_guarded(&slot, Duration::from_millis(10), Some(&own)).unwrap_err();
        assert!(matches!(err, FreezerError::UnsafePath(_)));
        assert!(slot.exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn reclaim_refuses_symlink() {
        let base = tree("symlink");
        let real = base.join("real");
        fs::create_dir_all(&real).unwrap();
        let link = base.join("slot-l");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = reclaim_cgroup_guarded(&link, Duration::from_millis(10), None).unwrap_err();
        assert!(matches!(err, FreezerError::UnsafePath(_)));
        assert!(real.exists());
        fs::remove_dir_all(&base).unwrap();
    }
}
