// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Host-wide slot arbitration across processes (spec 0036).
//!
//! The host holds `H` lock files `slot-<i>.lock` in one directory. A holder takes an
//! exclusive `flock` on one file and keeps the descriptor open for the life of the lease
//! (D3). The kernel drops the lock when the descriptor closes, including when the process
//! dies, so no reaper exists. Files are never unlinked and never passed to children (D7).

use std::fs::{File, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, FileExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rustix::fs::OFlags;
use serde::{Deserialize, Serialize};
use uas_domain::{HostCapacity, HostSlotIndex, SlotAcquirePolicy};

use crate::error::PoolError;

/// Default longest wait under [`SlotAcquirePolicy::WaitBounded`] (spec 0036 D4).
pub const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);
/// First poll interval under `WaitBounded` (spec 0036 D4).
pub const POLL_INITIAL: Duration = Duration::from_millis(50);
/// Largest poll interval under `WaitBounded` (spec 0036 D4).
pub const POLL_MAX: Duration = Duration::from_millis(500);
/// Fixed length of the holder record at offset 0 (spec 0036 D7).
pub const HOLDER_RECORD_LEN: usize = 128;

/// Advisory holder metadata (spec 0036 D7). Diagnostic only: lock state is the source of truth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolderRecord {
    /// Process id of the holder.
    pub pid: u32,
    /// Index of the host slot.
    pub slot_id: usize,
    /// Epoch seconds at acquisition.
    pub acquired_at: u64,
}

/// An exclusive host slot lease. Dropping it closes the descriptor and frees the lock.
#[derive(Debug)]
pub struct HostSlotLease {
    index: HostSlotIndex,
    file: File,
}

impl HostSlotLease {
    /// The host slot this lease holds.
    pub fn index(&self) -> HostSlotIndex {
        self.index
    }

    /// Whether the held descriptor is close-on-exec (REQ-0036-04).
    pub fn is_close_on_exec(&self) -> std::io::Result<bool> {
        let flags = rustix::io::fcntl_getfd(&self.file)?;
        Ok(flags.contains(rustix::io::FdFlags::CLOEXEC))
    }
}

/// Takes and tracks host slot locks under one directory.
#[derive(Debug, Clone)]
pub struct HostSlotArbiter {
    dir: PathBuf,
    capacity: HostCapacity,
}

impl HostSlotArbiter {
    /// An arbiter over `dir` with ceiling `capacity`. The directory is created and checked
    /// on each acquire, so a swap to a symlink is caught (REQ-0036-11).
    pub fn new(dir: impl Into<PathBuf>, capacity: HostCapacity) -> Self {
        Self {
            dir: dir.into(),
            capacity,
        }
    }

    /// The slot directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The host ceiling `H`.
    pub fn capacity(&self) -> HostCapacity {
        self.capacity
    }

    /// Path of `slot-<i>.lock`.
    pub fn lock_path(&self, index: HostSlotIndex) -> PathBuf {
        self.dir.join(index.file_name())
    }

    /// Create the slot directory with mode 0700, or refuse it when it is a symlink, not a directory,
    /// owned by another uid or looser than 0700 (spec 0036 D2).
    pub fn ensure_dir(&self) -> Result<(), PoolError> {
        let dir = &self.dir;
        let insecure = |why: String| PoolError::DirectoryInsecure(dir.clone(), why);
        let io_err = |source| PoolError::HostSlotIo {
            path: dir.clone(),
            source,
        };
        if std::fs::symlink_metadata(dir).is_err() {
            match std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
            {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(io_err(e)),
            }
        }
        let meta = std::fs::symlink_metadata(dir).map_err(io_err)?;
        if meta.file_type().is_symlink() {
            return Err(insecure("must not be a symlink".to_string()));
        }
        if !meta.is_dir() {
            return Err(insecure("is not a directory".to_string()));
        }
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(insecure(format!("mode {mode:04o} is looser than 0700")));
        }
        let uid = rustix::process::geteuid().as_raw();
        owner_check(meta.uid(), uid).map_err(insecure)?;
        Ok(())
    }

    /// Take the first free slot, trying `slot-0.lock` upward (REQ-0036-03). Never waits.
    pub fn try_acquire(&self) -> Result<HostSlotLease, PoolError> {
        self.ensure_dir()?;
        for index in self.capacity.indices() {
            let path = self.lock_path(index);
            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .mode(0o600)
                .custom_flags(OFlags::NOFOLLOW.bits() as i32)
                .open(&path)
                .map_err(|source| {
                    if source.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error()) {
                        PoolError::DirectoryInsecure(path.clone(), "must not be a symlink".into())
                    } else {
                        PoolError::HostSlotIo {
                            path: path.clone(),
                            source,
                        }
                    }
                })?;
            match file.try_lock() {
                Ok(()) => {
                    write_holder_record(&file, index, &path);
                    return Ok(HostSlotLease { index, file });
                }
                Err(std::fs::TryLockError::WouldBlock) => continue,
                Err(std::fs::TryLockError::Error(source)) => {
                    return Err(PoolError::HostSlotIo { path, source });
                }
            }
        }
        Err(PoolError::HostCapacityExceeded(self.capacity.get()))
    }

    /// Take a slot under `policy`. `WaitBounded` polls from 50 ms to 500 ms until `timeout`
    /// and then returns [`PoolError::HostCapacityExceeded`] (REQ-0036-05, REQ-0036-06).
    pub fn acquire(
        &self,
        policy: SlotAcquirePolicy,
        timeout: Duration,
    ) -> Result<HostSlotLease, PoolError> {
        match policy {
            SlotAcquirePolicy::Try => self.try_acquire(),
            SlotAcquirePolicy::WaitBounded => {
                let start = Instant::now();
                let mut interval = POLL_INITIAL;
                loop {
                    match self.try_acquire() {
                        Err(PoolError::HostCapacityExceeded(h)) => {
                            let left = timeout.saturating_sub(start.elapsed());
                            if left.is_zero() {
                                return Err(PoolError::HostCapacityExceeded(h));
                            }
                            std::thread::sleep(interval.min(left));
                            interval = (interval * 2).min(POLL_MAX);
                        }
                        other => return other,
                    }
                }
            }
        }
    }

    /// Read the advisory holder record of a slot. Diagnostic only (REQ-0036-10): `None` when
    /// the file is missing, unwritten or unparsable.
    pub fn holder_record(&self, index: HostSlotIndex) -> Option<HolderRecord> {
        let bytes = std::fs::read(self.lock_path(index)).ok()?;
        let text = std::str::from_utf8(bytes.get(..HOLDER_RECORD_LEN)?).ok()?;
        serde_json::from_str(text.trim()).ok()
    }
}

/// Refuse a directory owned by another uid (spec 0036 D2). Pure, so the mismatch is testable
/// without root.
fn owner_check(dir_uid: u32, process_uid: u32) -> Result<(), String> {
    if dir_uid != process_uid {
        return Err(format!(
            "owner uid {dir_uid} is not process uid {process_uid}"
        ));
    }
    Ok(())
}

/// Write the fixed-length holder record at offset 0 without truncating. A failed write is
/// logged and ignored: the record is advisory (spec 0036 D7).
fn write_holder_record(file: &File, index: HostSlotIndex, path: &Path) {
    let record = HolderRecord {
        pid: std::process::id(),
        slot_id: index.get(),
        acquired_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    };
    let mut buf = serde_json::to_vec(&record).unwrap_or_default();
    buf.resize(HOLDER_RECORD_LEN - 1, b' ');
    buf.push(b'\n');
    if let Err(e) = file.write_all_at(&buf, 0) {
        tracing::warn!(path = %path.display(), error = %e, "host slot holder record not written");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(label: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "uas-arb-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&p).expect("scratch dir");
            Self(p)
        }
        fn slots(&self) -> PathBuf {
            self.0.join("slots")
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn arbiter(s: &Scratch, h: usize) -> HostSlotArbiter {
        HostSlotArbiter::new(s.slots(), HostCapacity::new(h).unwrap())
    }

    /// REQ-0036-03: slot 0 is claimed first, then slot 1, then the host is full.
    #[test]
    fn claims_index_zero_then_one() {
        let s = Scratch::new("seq");
        let a = arbiter(&s, 2);
        let l0 = a.try_acquire().expect("first");
        let l1 = a.try_acquire().expect("second");
        assert_eq!((l0.index().get(), l1.index().get()), (0, 1));
        assert!(matches!(
            a.try_acquire(),
            Err(PoolError::HostCapacityExceeded(2))
        ));
    }

    /// REQ-0036-06: WaitBounded gives up after the timeout, not before.
    #[test]
    fn wait_bounded_times_out_after_the_configured_duration() {
        let s = Scratch::new("timeout");
        let a = arbiter(&s, 1);
        let _held = a.try_acquire().expect("held");
        let timeout = Duration::from_millis(300);
        let start = Instant::now();
        let err = a
            .acquire(SlotAcquirePolicy::WaitBounded, timeout)
            .expect_err("host is full");
        let took = start.elapsed();
        assert!(matches!(err, PoolError::HostCapacityExceeded(1)));
        assert!(took >= timeout, "returned early after {took:?}");
        assert!(took < timeout + Duration::from_secs(2), "hung: {took:?}");
    }

    /// REQ-0036-11: a symlinked slot directory is refused.
    #[test]
    fn symlinked_directory_is_refused() {
        let s = Scratch::new("symlink");
        let real = s.0.join("real");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o700)).unwrap();
        symlink(&real, s.slots()).unwrap();
        let err = arbiter(&s, 2).try_acquire().expect_err("symlink");
        assert!(matches!(err, PoolError::DirectoryInsecure(_, _)), "{err:?}");
    }

    /// REQ-0036-11: a directory looser than 0700 is refused.
    #[test]
    fn loose_mode_directory_is_refused() {
        let s = Scratch::new("mode");
        std::fs::create_dir_all(s.slots()).unwrap();
        std::fs::set_permissions(s.slots(), std::fs::Permissions::from_mode(0o750)).unwrap();
        let err = arbiter(&s, 2).try_acquire().expect_err("0750");
        assert!(matches!(err, PoolError::DirectoryInsecure(_, _)), "{err:?}");
    }

    /// REQ-0036-02: the arbiter creates the directory with mode 0700.
    #[test]
    fn creates_directory_with_mode_0700() {
        let s = Scratch::new("create");
        let _l = arbiter(&s, 2).try_acquire().expect("acquire");
        let mode = std::fs::metadata(s.slots()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    /// REQ-0036-11: a symlinked lock file is refused, not followed.
    #[test]
    fn symlinked_lock_file_is_refused() {
        let s = Scratch::new("lockln");
        let a = arbiter(&s, 1);
        drop(a.try_acquire().expect("create dir"));
        let target = s.0.join("elsewhere");
        std::fs::write(&target, b"").unwrap();
        std::fs::remove_file(a.lock_path(HostCapacity::new(1).unwrap().indices().next().unwrap()))
            .unwrap();
        symlink(&target, s.slots().join("slot-0.lock")).unwrap();
        let err = a.try_acquire().expect_err("symlinked lock file");
        assert!(matches!(err, PoolError::DirectoryInsecure(_, _)), "{err:?}");
    }

    /// REQ-0036-02, REQ-0036-10: lock files stay after the lease drops.
    #[test]
    fn lock_files_are_retained_on_drop() {
        let s = Scratch::new("retain");
        let a = arbiter(&s, 2);
        let l0 = a.try_acquire().expect("l0");
        let l1 = a.try_acquire().expect("l1");
        drop((l0, l1));
        assert!(s.slots().join("slot-0.lock").is_file());
        assert!(s.slots().join("slot-1.lock").is_file());
        // And the freed slot can be taken again.
        assert_eq!(a.try_acquire().expect("again").index().get(), 0);
    }

    /// REQ-0036-04: the held descriptor is close-on-exec.
    #[test]
    fn held_descriptor_is_close_on_exec() {
        let s = Scratch::new("cloexec");
        let lease = arbiter(&s, 1).try_acquire().expect("lease");
        assert!(lease.is_close_on_exec().expect("fcntl"));
    }

    /// REQ-0036-10: the holder record is fixed-length, at offset 0, and advisory.
    #[test]
    fn holder_record_is_written_at_offset_zero() {
        let s = Scratch::new("record");
        let a = arbiter(&s, 2);
        let lease = a.try_acquire().expect("lease");
        let rec = a.holder_record(lease.index()).expect("record");
        assert_eq!(rec.pid, std::process::id());
        assert_eq!(rec.slot_id, 0);
        let len = std::fs::metadata(s.slots().join("slot-0.lock"))
            .unwrap()
            .len();
        assert_eq!(len as usize, HOLDER_RECORD_LEN);
    }

    /// REQ-0036-10: the lock decides, not the record. Garbage on a held slot leaves it held;
    /// garbage on a free slot is still acquirable.
    #[test]
    fn lock_state_decides_not_the_record() {
        use std::os::unix::fs::FileExt;
        let s = Scratch::new("garbage");
        let a = arbiter(&s, 1);
        let lease = a.try_acquire().expect("lease");
        let path = s.slots().join("slot-0.lock");
        let scribble = || {
            OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .write_all_at(&[b'x'; HOLDER_RECORD_LEN], 0)
                .unwrap();
        };
        scribble();
        assert!(
            a.holder_record(lease.index()).is_none(),
            "record is garbage"
        );
        assert!(matches!(
            a.try_acquire(),
            Err(PoolError::HostCapacityExceeded(1))
        ));
        drop(lease);
        scribble();
        assert_eq!(a.try_acquire().expect("free slot").index().get(), 0);
    }

    /// REQ-0036-11: a directory owned by another uid is refused.
    #[test]
    fn foreign_owner_is_refused() {
        assert!(owner_check(1000, 1000).is_ok());
        assert!(owner_check(0, 1000).is_err());
    }

    /// REQ-0036-08: a wave lock (spec 0025 D8) in the SAME directory is never taken. The arbiter
    /// claims exactly `slot-0..H-1.lock`, leaves `wave.lock` unlocked and unwritten, and still
    /// acquires its slots while another descriptor holds `wave.lock`.
    #[test]
    fn host_slots_do_not_touch_a_wave_lock_in_the_same_directory() {
        let s = Scratch::new("wave");
        let a = arbiter(&s, 2);
        drop(a.try_acquire().expect("create dir"));
        let wave = s.slots().join("wave.lock");
        let open_wave = || {
            OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&wave)
                .unwrap()
        };
        drop(open_wave());

        let l0 = a.try_acquire().expect("l0");
        let l1 = a.try_acquire().expect("l1");
        assert_eq!((l0.index().get(), l1.index().get()), (0, 1));
        assert_eq!(
            std::fs::metadata(&wave).unwrap().len(),
            0,
            "wave.lock written"
        );
        let held_by_other = open_wave();
        held_by_other
            .try_lock()
            .expect("arbiter took wave.lock while holding its slots");

        // With wave.lock held elsewhere, the slots still come free and are retaken.
        drop((l0, l1));
        let _again = (
            a.try_acquire().expect("l0 again"),
            a.try_acquire().expect("l1 again"),
        );
        assert!(matches!(
            a.try_acquire(),
            Err(PoolError::HostCapacityExceeded(2))
        ));
        let mut names: Vec<_> = std::fs::read_dir(s.slots())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, vec!["slot-0.lock", "slot-1.lock", "wave.lock"]);
    }
}
