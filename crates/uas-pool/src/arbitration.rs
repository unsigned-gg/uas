// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Host-wide slot arbitration across processes (spec 0036).
//!
//! Enforces a host-wide capacity ceiling across all UAS processes on the machine.
//! Each slot is represented as an advisory lock file (`slot-<i>.lock`) in a canonical
//! directory (default `/tmp/uas-slots`, override via `UAS_SLOT_DIR`).
//! Leases are retained by open file descriptors with `O_CLOEXEC`.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustix::process::getuid;
use serde::{Deserialize, Serialize};
use uas_domain::{HostSlotIndex, SlotAcquirePolicy, SlotId};

use crate::error::PoolError;

/// The canonical directory for host slot lock files.
pub const DEFAULT_SLOT_DIR: &str = "/tmp/uas-slots";

/// Environment variable overriding the slot lock directory.
pub const UAS_SLOT_DIR_ENV: &str = "UAS_SLOT_DIR";

/// A leased host slot holding an open advisory lock file descriptor.
#[derive(Debug)]
pub struct HostSlotLease {
    index: HostSlotIndex,
    file: File,
    path: PathBuf,
}

impl HostSlotLease {
    /// Returns the host slot index.
    #[must_use]
    pub const fn index(&self) -> HostSlotIndex {
        self.index
    }

    /// Returns the path to the slot lock file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns a reference to the locked file descriptor.
    #[must_use]
    pub fn file(&self) -> &File {
        &self.file
    }
}

/// Metadata written at offset 0 of a held host slot lock file (spec 0036 D7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostSlotMetadata {
    /// Process ID holding the lock.
    pub pid: u32,
    /// Slot ID allocated to this lease.
    pub slot_id: String,
    /// Epoch timestamp when the lease was acquired.
    pub acquired_at: u64,
}

/// Host slot arbiter managing lock files and acquisition policies (spec 0036).
#[derive(Debug, Clone)]
pub struct HostSlotArbiter {
    slot_dir: PathBuf,
    host_max_slots: usize,
    acquire_policy: SlotAcquirePolicy,
}

impl HostSlotArbiter {
    /// Creates a new arbiter and verifies or initializes the slot directory.
    pub fn new(
        slot_dir: Option<PathBuf>,
        host_max_slots: usize,
        acquire_policy: SlotAcquirePolicy,
    ) -> Result<Self, PoolError> {
        let dir = slot_dir
            .or_else(|| std::env::var_os(UAS_SLOT_DIR_ENV).map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_SLOT_DIR));

        Self::verify_or_create_dir(&dir)?;

        Ok(Self {
            slot_dir: dir,
            host_max_slots: host_max_slots.max(1),
            acquire_policy,
        })
    }

    /// Returns the configured slot directory path.
    #[must_use]
    pub fn slot_dir(&self) -> &Path {
        &self.slot_dir
    }

    /// Returns the configured host maximum slots ceiling.
    #[must_use]
    pub const fn host_max_slots(&self) -> usize {
        self.host_max_slots
    }

    /// Verifies the slot directory permissions or creates it with mode 0700.
    pub fn verify_or_create_dir(dir: &Path) -> Result<(), PoolError> {
        if dir.exists() {
            let meta = fs::symlink_metadata(dir).map_err(|e| {
                PoolError::DirectoryInsecure(dir.to_path_buf(), format!("metadata failed: {e}"))
            })?;

            if meta.file_type().is_symlink() {
                return Err(PoolError::DirectoryInsecure(
                    dir.to_path_buf(),
                    "directory must not be a symlink".into(),
                ));
            }

            let my_uid = getuid().as_raw();
            if meta.uid() != my_uid {
                return Err(PoolError::DirectoryInsecure(
                    dir.to_path_buf(),
                    format!(
                        "directory owner UID {} differs from process UID {}",
                        meta.uid(),
                        my_uid
                    ),
                ));
            }

            // Mode looser than 0700: any group or other bits set
            let mode = meta.mode() & 0o777;
            if (mode & 0o077) != 0 {
                return Err(PoolError::DirectoryInsecure(
                    dir.to_path_buf(),
                    format!("directory permissions {mode:o} are looser than 0700"),
                ));
            }
        } else {
            fs::DirBuilder::new()
                .mode(0o700)
                .recursive(true)
                .create(dir)
                .map_err(|e| {
                    PoolError::DirectoryInsecure(dir.to_path_buf(), format!("mkdir failed: {e}"))
                })?;
        }

        Ok(())
    }

    /// Slot lock file path for a given index.
    #[must_use]
    pub fn slot_lock_path(&self, index: usize) -> PathBuf {
        self.slot_dir.join(format!("slot-{index}.lock"))
    }

    /// Attempts to acquire an available host slot according to the configured policy.
    pub fn acquire(&self, slot_id: &SlotId) -> Result<HostSlotLease, PoolError> {
        match self.acquire_policy {
            SlotAcquirePolicy::Try => self.try_acquire_once(slot_id),
            SlotAcquirePolicy::WaitBounded { timeout } => {
                let deadline = Instant::now() + timeout;
                let mut backoff = Duration::from_millis(50);
                let max_backoff = Duration::from_millis(500);

                loop {
                    match self.try_acquire_once(slot_id) {
                        Ok(lease) => return Ok(lease),
                        Err(PoolError::HostCapacityExceeded(_)) => {
                            if Instant::now() >= deadline {
                                return Err(PoolError::HostCapacityExceeded(self.host_max_slots));
                            }
                            thread::sleep(backoff);
                            backoff = (backoff * 2).min(max_backoff);
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }

    /// Attempts to acquire any free host slot lock file in sequential index order.
    fn try_acquire_once(&self, slot_id: &SlotId) -> Result<HostSlotLease, PoolError> {
        for i in 0..self.host_max_slots {
            let path = self.slot_lock_path(i);
            // Open with read+write, create if missing, O_CLOEXEC (std default)
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .map_err(PoolError::Spawn)?;

            if file.try_lock().is_ok() {
                // Write fixed-size advisory metadata at offset 0
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let meta = HostSlotMetadata {
                    pid: std::process::id(),
                    slot_id: slot_id.as_str().to_string(),
                    acquired_at: now,
                };
                if let Ok(json) = serde_json::to_vec(&meta) {
                    let _ = file.write_all(&json);
                    let _ = file.flush();
                }

                return Ok(HostSlotLease {
                    index: HostSlotIndex(i),
                    file,
                    path,
                });
            }
        }

        Err(PoolError::HostCapacityExceeded(self.host_max_slots))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let p =
                std::env::temp_dir().join(format!("uas-arb-test-{}-{}", std::process::id(), id));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::DirBuilder::new()
                .mode(0o700)
                .recursive(true)
                .create(&p)
                .unwrap();
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sequential_slot_acquisition_claims_index_zero_then_one() {
        let dir = TestDir::new();
        let arbiter =
            HostSlotArbiter::new(Some(dir.path().to_path_buf()), 2, SlotAcquirePolicy::Try)
                .unwrap();

        let s0 = SlotId::new("slot-s0").unwrap();
        let s1 = SlotId::new("slot-s1").unwrap();

        let lease0 = arbiter.acquire(&s0).unwrap();
        assert_eq!(lease0.index().as_usize(), 0);

        let lease1 = arbiter.acquire(&s1).unwrap();
        assert_eq!(lease1.index().as_usize(), 1);

        let s2 = SlotId::new("slot-s2").unwrap();
        let err = arbiter.acquire(&s2).unwrap_err();
        assert!(matches!(err, PoolError::HostCapacityExceeded(2)));

        // Release slot 0 and verify it can be reacquired
        drop(lease0);
        let lease_reacquired = arbiter.acquire(&s2).unwrap();
        assert_eq!(lease_reacquired.index().as_usize(), 0);
    }

    #[test]
    fn wait_bounded_timeout_expires_on_exhausted_capacity() {
        let dir = TestDir::new();
        let arbiter = HostSlotArbiter::new(
            Some(dir.path().to_path_buf()),
            1,
            SlotAcquirePolicy::WaitBounded {
                timeout: Duration::from_millis(150),
            },
        )
        .unwrap();

        let s0 = SlotId::new("slot-0").unwrap();
        let _lease0 = arbiter.acquire(&s0).unwrap();

        let s1 = SlotId::new("slot-1").unwrap();
        let start = Instant::now();
        let err = arbiter.acquire(&s1).unwrap_err();
        assert!(matches!(err, PoolError::HostCapacityExceeded(1)));
        assert!(start.elapsed() >= Duration::from_millis(150));
    }

    #[test]
    fn slot_lock_files_are_retained_on_drop_and_never_unlinked() {
        let dir = TestDir::new();
        let arbiter =
            HostSlotArbiter::new(Some(dir.path().to_path_buf()), 1, SlotAcquirePolicy::Try)
                .unwrap();

        let s0 = SlotId::new("slot-0").unwrap();
        let lease = arbiter.acquire(&s0).unwrap();
        let path = lease.path().to_path_buf();
        assert!(path.exists());

        drop(lease);
        // File must still exist after drop
        assert!(path.exists());
    }

    #[test]
    fn directory_insecure_rejects_symlink() {
        let dir = TestDir::new();
        let real_target = dir.path().join("real");
        fs::create_dir(&real_target).unwrap();
        let symlink_path = dir.path().join("symlink");
        std::os::unix::fs::symlink(&real_target, &symlink_path).unwrap();

        let err = HostSlotArbiter::new(Some(symlink_path), 2, SlotAcquirePolicy::Try).unwrap_err();
        assert!(matches!(err, PoolError::DirectoryInsecure(_, _)));
    }
}
