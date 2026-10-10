//! Linux cgroup v2 freezer and process suspension controller.
//!
//! CER-2722: Manages cgroup v2 hierarchy under a delegated subtree (e.g.
//! `/sys/fs/cgroup/cas` or `<delegated-base>`) with sub-5ms freeze/unfreeze
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
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use cas_domain::SlotId;

use crate::error::FreezerError;

/// Operational state of a freezer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreezerState {
    /// Worker processes are actively executing.
    Thawed,
    /// Worker processes are paused / frozen in execution.
    Frozen,
}

/// Resource constraints configured on child cgroups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResourceLimits {
    /// CPU quota in microseconds per period (None means "max").
    pub cpu_quota_us: Option<u64>,
    /// CPU period in microseconds (defaults to 100,000us i.e. 100ms).
    pub cpu_period_us: Option<u64>,
    /// Memory high watermark in bytes (throttles/reclaims beyond this).
    pub memory_high_bytes: Option<u64>,
    /// Memory hard ceiling in bytes (triggers OOM kill beyond this).
    pub memory_max_bytes: Option<u64>,
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

/// Linux cgroup v2 freezer controller for a single worker slot.
#[derive(Debug)]
pub struct CgroupFreezer {
    base_cgroup: PathBuf,
    slot_cgroup: PathBuf,
    slot_id: SlotId,
}

impl CgroupFreezer {
    /// Initialize a child cgroup freezer under `base_cgroup`.
    ///
    /// The ephemeral child cgroup directory `slot-<id>` is created if not already present.
    pub fn new(base_cgroup: impl Into<PathBuf>, slot_id: SlotId) -> Result<Self, FreezerError> {
        let base_cgroup = base_cgroup.into();
        let slot_cgroup = base_cgroup.join(format!("slot-{}", slot_id));

        Self::verify_safety(&base_cgroup, &slot_cgroup)?;

        if !slot_cgroup.exists() {
            fs::create_dir_all(&slot_cgroup).map_err(|e| FreezerError::Io {
                path: slot_cgroup.clone(),
                source: e,
            })?;
        }

        Ok(Self {
            base_cgroup,
            slot_cgroup,
            slot_id,
        })
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

    /// Configures CPU and memory constraints in the child cgroup.
    pub fn configure_limits(&self, limits: &ResourceLimits) -> Result<(), FreezerError> {
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
            }
        }

        if let Some(high) = limits.memory_high_bytes {
            let mem_high_path = self.slot_cgroup.join("memory.high");
            if mem_high_path.exists() {
                let line = format!("{high}\n");
                fs::write(&mem_high_path, line.as_bytes()).map_err(|e| FreezerError::Io {
                    path: mem_high_path,
                    source: e,
                })?;
            }
        }

        if let Some(max) = limits.memory_max_bytes {
            let mem_max_path = self.slot_cgroup.join("memory.max");
            if mem_max_path.exists() {
                let line = format!("{max}\n");
                fs::write(&mem_max_path, line.as_bytes()).map_err(|e| FreezerError::Io {
                    path: mem_max_path,
                    source: e,
                })?;
            }
        }

        Ok(())
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
}
