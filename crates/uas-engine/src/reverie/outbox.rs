// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Write-behind outbox for Reverie attestations (spec 0044 D6, D7, D8, D9).
//!
//! When the Reverie memory daemon is unreachable, slow, or restarting, attestations
//! are appended to an ordered, durable JSONL outbox on disk. A later successful
//! call drains pending records in FIFO order.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{error, warn};

use super::ReverieMemoryRecord;
use crate::EngineError;

/// Sequence counter for unique event IDs (spec 0044 D9).
static EVENT_SEQ: AtomicU64 = AtomicU64::new(1);

/// Maximum pending lines before dropping oldest (spec 0044 D7).
pub const DEFAULT_MAX_PENDING_LINES: usize = 10_000;
/// Maximum outbox byte size before dropping oldest (spec 0044 D7).
pub const DEFAULT_MAX_BYTES: u64 = 50 * 1024 * 1024; // 50 MiB

/// Build a unique event ID for a record call (spec 0044 D9).
///
/// Format: `uas:<session_id>:<unix_nanos>:<pid>:<seq>`.
#[must_use]
pub fn generate_event_id(session_id: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    let seq = EVENT_SEQ.fetch_add(1, Ordering::SeqCst);
    format!("uas:{session_id}:{nanos}:{pid}:{seq}")
}

/// A persisted entry in the write-behind outbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEntry {
    /// Idempotency key assigned before the first send attempt.
    pub event_id: String,
    /// The memory record and attestation.
    pub record: ReverieMemoryRecord,
}

/// Persisted state sidecar tracking the drain offset and drop metrics (spec 0044 D6, D7).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxState {
    /// Byte offset in `outbox.jsonl` of the first unsent line.
    pub offset: u64,
    /// Cumulative count of oldest records dropped due to capacity limits.
    pub dropped: u64,
}

/// Retrieve current process effective/real UID from `/proc/self/status`.
fn current_process_uid() -> u32 {
    if let Ok(content) = fs::read_to_string("/proc/self/status") {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("Uid:") {
                let mut parts = rest.split_whitespace();
                if let Some(uid_str) = parts.next() {
                    if let Ok(uid) = uid_str.parse::<u32>() {
                        return uid;
                    }
                }
            }
        }
    }
    1000
}

/// Validate directory ownership, permissions, and non-symlink status (spec 0044 D8).
fn validate_directory(dir: &Path) -> Result<(), &'static str> {
    let sym_meta = fs::symlink_metadata(dir).map_err(|_| "cannot stat directory")?;
    if sym_meta.file_type().is_symlink() {
        return Err("outbox directory is a symlink");
    }
    if !sym_meta.is_dir() {
        return Err("outbox path is not a directory");
    }
    let mode = sym_meta.permissions().mode() & 0o777;
    if mode != 0o700 {
        return Err("outbox directory mode is looser than 0700");
    }
    let uid = current_process_uid();
    if sym_meta.uid() != uid {
        return Err("outbox directory is not owned by process uid");
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let file_sym_meta =
                fs::symlink_metadata(entry.path()).map_err(|_| "cannot stat file")?;
            if file_sym_meta.file_type().is_symlink() {
                return Err("entry in outbox is a symlink");
            }
            let file_mode = file_sym_meta.permissions().mode() & 0o777;
            if file_mode != 0o600 {
                return Err("entry mode in outbox is looser than 0600");
            }
            if file_sym_meta.uid() != uid {
                return Err("entry owner is not process uid");
            }
        }
    }
    Ok(())
}

/// Write-behind outbox handle managing file operations and locks (spec 0044 D6, D8).
pub struct ReverieOutbox {
    dir: PathBuf,
    refused: bool,
    max_pending_lines: usize,
    max_bytes: u64,
    holding_drain_lock: AtomicBool,
}

impl ReverieOutbox {
    /// Open or create an outbox at `dir` (spec 0044 D8).
    pub fn open(dir: PathBuf) -> Self {
        Self::with_limits(dir, DEFAULT_MAX_PENDING_LINES, DEFAULT_MAX_BYTES)
    }

    /// Open or create an outbox with custom capacity limits (for testing and configuration).
    pub fn with_limits(dir: PathBuf, max_pending_lines: usize, max_bytes: u64) -> Self {
        let mut refused = false;

        // If absent, create directory 0700
        if !dir.exists() {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            builder.mode(0o700);
            if builder.create(&dir).is_err() {
                warn!("failed to create outbox directory {:?}", dir);
                refused = true;
            }
        }

        if !refused {
            if let Err(reason) = validate_directory(&dir) {
                warn!("reverie outbox directory {:?} refused: {}", dir, reason);
                refused = true;
            }
        }

        Self {
            dir,
            refused,
            max_pending_lines,
            max_bytes,
            holding_drain_lock: AtomicBool::new(false),
        }
    }

    /// Whether the outbox directory was refused due to permissions or ownership.
    #[must_use]
    pub fn is_refused(&self) -> bool {
        self.refused
    }

    /// Path to the outbox directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join("state.json")
    }

    /// Path to `outbox.jsonl`.
    pub fn outbox_path(&self) -> PathBuf {
        self.dir.join("outbox.jsonl")
    }

    fn append_lock_path(&self) -> PathBuf {
        self.dir.join("append.lock")
    }

    fn drain_lock_path(&self) -> PathBuf {
        self.dir.join("drain.lock")
    }

    /// Acquire the exclusive `append.lock` (spec 0044 D8).
    ///
    /// # Panics
    /// Panics if the handle currently holds `drain.lock` to enforce strict lock hierarchy.
    pub fn acquire_append_lock(&self) -> io::Result<File> {
        assert!(
            !self.holding_drain_lock.load(Ordering::SeqCst),
            "lock order violation: cannot acquire append.lock while holding drain.lock"
        );
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.append_lock_path())?;
        file.lock()?;
        Ok(file)
    }

    /// Try to acquire `drain.lock` without blocking (spec 0044 D8).
    pub fn try_acquire_drain_lock(&self) -> io::Result<Option<File>> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.drain_lock_path())?;
        match file.try_lock() {
            Ok(()) => {
                self.holding_drain_lock.store(true, Ordering::SeqCst);
                Ok(Some(file))
            }
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e),
        }
    }

    /// Release `drain.lock` and clear holding flag.
    pub fn release_drain_lock(&self, _file: File) {
        self.holding_drain_lock.store(false, Ordering::SeqCst);
        // Dropping `_file` unlocks the descriptor in kernel
    }

    /// Read the persisted state sidecar, returning default if absent.
    pub fn read_state(&self) -> OutboxState {
        if self.refused {
            return OutboxState::default();
        }
        if let Ok(bytes) = fs::read(self.state_path()) {
            if let Ok(state) = serde_json::from_slice(&bytes) {
                return state;
            }
        }
        OutboxState::default()
    }

    /// Write updated state sidecar via temp file, fsync, and atomic rename (spec 0044 D6).
    pub fn write_state(&self, state: OutboxState) -> io::Result<()> {
        if self.refused {
            return Ok(());
        }
        let tmp = self.dir.join("state.json.tmp");
        let dest = self.state_path();
        let json = serde_json::to_vec(&state).map_err(|e| io::Error::other(e.to_string()))?;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(&json)?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.sync_all()?;
        drop(file);

        fs::rename(&tmp, &dest)?;
        self.fsync_dir()?;
        Ok(())
    }

    /// Fsync the outbox directory to guarantee durability of file renames/creations (spec 0044 D6).
    fn fsync_dir(&self) -> io::Result<()> {
        if let Ok(dir_file) = File::open(&self.dir) {
            let _ = dir_file.sync_all();
        }
        Ok(())
    }

    /// Calculate the number of pending lines from `offset` to EOF.
    pub fn pending_count(&self) -> usize {
        if self.refused {
            return 0;
        }
        let state = self.read_state();
        let path = self.outbox_path();
        if !path.exists() {
            return 0;
        }
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return 0,
        };
        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(state.offset)).is_err() {
            return 0;
        }
        let mut count = 0;
        let mut line = String::new();
        while let Ok(bytes) = reader.read_line(&mut line) {
            if bytes == 0 {
                break;
            }
            if !line.trim().is_empty() {
                count += 1;
            }
            line.clear();
        }
        count
    }

    /// Append an entry to `outbox.jsonl` under `append.lock` (spec 0044 D6, D7, D8).
    ///
    /// Returns `true` on successful durable append, or `false` if refused or append failed.
    pub fn append(&self, entry: &OutboxEntry) -> bool {
        if self.refused {
            return false;
        }

        let _append_guard = match self.acquire_append_lock() {
            Ok(guard) => guard,
            Err(e) => {
                error!("failed to acquire append.lock: {e}");
                return false;
            }
        };

        let outbox_path = self.outbox_path();
        let is_new_file = !outbox_path.exists();

        let mut file = match OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(&outbox_path)
        {
            Ok(f) => f,
            Err(e) => {
                error!("failed to open outbox.jsonl for append: {e}");
                return false;
            }
        };

        if is_new_file {
            let _ = self.fsync_dir();
        }

        let mut line_bytes = match serde_json::to_vec(entry) {
            Ok(b) => b,
            Err(e) => {
                error!("failed to serialize outbox entry: {e}");
                return false;
            }
        };
        line_bytes.push(b'\n');

        if let Err(e) = file.write_all(&line_bytes) {
            error!("failed to write line to outbox.jsonl: {e}");
            return false;
        }
        if let Err(e) = file.flush() {
            error!("failed to flush outbox.jsonl: {e}");
            return false;
        }
        if let Err(e) = file.sync_all() {
            error!("failed to fsync outbox.jsonl: {e}");
            return false;
        }

        // Enforce capacity caps (spec 0044 D7)
        self.enforce_caps_under_append_lock();

        true
    }

    /// Enforce soft capacity caps by advancing offset if `drain.lock` is acquired (spec 0044 D7).
    fn enforce_caps_under_append_lock(&self) {
        let outbox_path = self.outbox_path();
        if !outbox_path.exists() {
            return;
        }

        let file_len = fs::metadata(&outbox_path).map(|m| m.len()).unwrap_or(0);
        let mut state = self.read_state();
        let pending_bytes = file_len.saturating_sub(state.offset);

        // Check if either line or byte limit is exceeded
        let file = match File::open(&outbox_path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(state.offset)).is_err() {
            return;
        }

        let mut line_offsets = Vec::new();
        let mut curr_offset = state.offset;
        let mut line = String::new();
        while let Ok(n) = reader.read_line(&mut line) {
            if n == 0 {
                break;
            }
            line_offsets.push(curr_offset);
            curr_offset += n as u64;
            line.clear();
        }

        let pending_lines = line_offsets.len();
        if pending_lines <= self.max_pending_lines && pending_bytes <= self.max_bytes {
            return;
        }

        // Try drain.lock to drop oldest records
        if let Ok(Some(drain_lock)) = self.try_acquire_drain_lock() {
            let mut target_drop = 0;
            if pending_lines > self.max_pending_lines {
                target_drop = target_drop.max(pending_lines - self.max_pending_lines);
            }
            if pending_bytes > self.max_bytes {
                let bytes_to_free = pending_bytes - self.max_bytes;
                let mut freed = 0;
                for (idx, &off) in line_offsets.iter().enumerate() {
                    let next_off = line_offsets.get(idx + 1).copied().unwrap_or(curr_offset);
                    freed += next_off - off;
                    if freed >= bytes_to_free {
                        target_drop = target_drop.max(idx + 1);
                        break;
                    }
                }
            }

            if target_drop > 0 && target_drop <= line_offsets.len() {
                let new_offset = if target_drop < line_offsets.len() {
                    line_offsets[target_drop]
                } else {
                    curr_offset
                };
                state.offset = new_offset;
                state.dropped += target_drop as u64;
                warn!(
                    "outbox cap exceeded: dropped {} oldest records; total dropped: {}",
                    target_drop, state.dropped
                );
                let _ = self.write_state(state);
            }
            self.release_drain_lock(drain_lock);
        }
    }

    /// Drain pending records, calling `send_fn` for each in FIFO order (spec 0044 D6).
    ///
    /// Stops at the first send failure. Returns count of successfully delivered records.
    pub fn drain<F>(&self, mut send_fn: F) -> Result<usize, EngineError>
    where
        F: FnMut(&OutboxEntry) -> Result<(), EngineError>,
    {
        if self.refused {
            return Ok(0);
        }

        let drain_lock = match self.try_acquire_drain_lock() {
            Ok(Some(lock)) => lock,
            Ok(None) => return Ok(0), // Another process/thread is draining
            Err(e) => {
                error!("failed to acquire drain.lock: {e}");
                return Ok(0);
            }
        };

        let mut state = self.read_state();
        let outbox_path = self.outbox_path();
        if !outbox_path.exists() {
            self.release_drain_lock(drain_lock);
            return Ok(0);
        }

        let file = match File::open(&outbox_path) {
            Ok(f) => f,
            Err(e) => {
                self.release_drain_lock(drain_lock);
                return Err(EngineError::Io(e));
            }
        };

        let mut reader = BufReader::new(file);
        if let Err(e) = reader.seek(SeekFrom::Start(state.offset)) {
            self.release_drain_lock(drain_lock);
            return Err(EngineError::Io(e));
        }

        let mut curr_offset = state.offset;
        let mut delivered = 0;
        let mut line = String::new();

        while let Ok(bytes_read) = reader.read_line(&mut line) {
            if bytes_read == 0 {
                break;
            }
            let next_offset = curr_offset + bytes_read as u64;
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if let Ok(entry) = serde_json::from_str::<OutboxEntry>(trimmed) {
                    match send_fn(&entry) {
                        Ok(()) => {
                            state.offset = next_offset;
                            let _ = self.write_state(state);
                            delivered += 1;
                        }
                        Err(e) => {
                            // First failure stops the drain (spec 0044 D6, REQ-0044-05)
                            self.release_drain_lock(drain_lock);
                            return Err(e);
                        }
                    }
                }
            }
            curr_offset = next_offset;
            line.clear();
        }

        self.release_drain_lock(drain_lock);

        // Compact if offset is significant
        if state.offset > 0 {
            self.compact();
        }

        Ok(delivered)
    }

    /// Compact outbox.jsonl by rewriting pending records to temp file under both locks (spec 0044 D6, D8).
    pub fn compact(&self) {
        if self.refused {
            return;
        }

        // Lock order: append.lock then drain.lock (spec 0044 D8)
        let append_lock = match self.acquire_append_lock() {
            Ok(l) => l,
            Err(_) => return,
        };
        let drain_lock = match self.try_acquire_drain_lock() {
            Ok(Some(l)) => l,
            _ => return, // Could not acquire drain.lock; skip compaction
        };

        let mut state = self.read_state();
        let outbox_path = self.outbox_path();
        if outbox_path.exists() && state.offset > 0 {
            let tmp_path = self.dir.join("outbox.jsonl.tmp");
            if let Ok(mut src) = File::open(&outbox_path) {
                if src.seek(SeekFrom::Start(state.offset)).is_ok() {
                    if let Ok(mut dest) = OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .mode(0o600)
                        .open(&tmp_path)
                    {
                        if io::copy(&mut src, &mut dest).is_ok()
                            && dest.flush().is_ok()
                            && dest.sync_all().is_ok()
                        {
                            drop(dest);
                            drop(src);
                            if fs::rename(&tmp_path, &outbox_path).is_ok() {
                                state.offset = 0;
                                let _ = self.write_state(state);
                                let _ = self.fsync_dir();
                            }
                        }
                    }
                }
            }
            let _ = fs::remove_file(tmp_path);
        }

        self.release_drain_lock(drain_lock);
        drop(append_lock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use uas_domain::{Attestation, EvidenceTier, PrincipalRef};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "uas-outbox-{}-{name}-{}",
                std::process::id(),
                EVENT_SEQ.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = fs::remove_dir_all(&path);
            let mut b = fs::DirBuilder::new();
            b.mode(0o700);
            b.recursive(true);
            b.create(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn dummy_record(id: &str) -> ReverieMemoryRecord {
        ReverieMemoryRecord {
            project: "uas".into(),
            session_id: "sess-test".into(),
            topic_key: "uas/test".into(),
            summary: format!("record {id}"),
            attestation: Attestation {
                signer: PrincipalRef::new("uas/test-signer").unwrap(),
                evidence: EvidenceTier::Verified,
                receipt: "test-receipt".into(),
                at: 1000,
            },
        }
    }

    #[test]
    fn test_outbox_directory_validation_rejects_symlink_and_bad_mode() {
        let temp = TestDir::new("val");
        let base = temp.path();

        // 1. Directory mode 0755 is refused
        let bad_mode_dir = base.join("bad_mode");
        let mut b = fs::DirBuilder::new();
        b.mode(0o755);
        b.create(&bad_mode_dir).unwrap();
        let outbox1 = ReverieOutbox::open(bad_mode_dir);
        assert!(outbox1.is_refused());

        // 2. Symlink to valid directory is refused
        let valid_target = base.join("valid_target");
        let mut b2 = fs::DirBuilder::new();
        b2.mode(0o700);
        b2.create(&valid_target).unwrap();
        let sym_link = base.join("sym_link");
        std::os::unix::fs::symlink(&valid_target, &sym_link).unwrap();
        let outbox2 = ReverieOutbox::open(sym_link);
        assert!(outbox2.is_refused());

        // 3. Normal directory 0700 is accepted
        let good_dir = base.join("good");
        let outbox3 = ReverieOutbox::open(good_dir);
        assert!(!outbox3.is_refused());
    }

    #[test]
    #[should_panic(
        expected = "lock order violation: cannot acquire append.lock while holding drain.lock"
    )]
    fn test_outbox_lock_order_panic_when_holding_drain_and_asking_append() {
        let temp = TestDir::new("t");
        let outbox = ReverieOutbox::open(temp.path().join("outbox"));
        let _drain = outbox
            .try_acquire_drain_lock()
            .unwrap()
            .expect("acquire drain");
        // This MUST panic per spec 0044 D8
        let _append = outbox.acquire_append_lock();
    }

    #[test]
    fn test_outbox_soft_cap_and_drop_oldest() {
        let temp = TestDir::new("t");
        let outbox = ReverieOutbox::with_limits(temp.path().join("outbox"), 3, 10 * 1024 * 1024);

        for i in 1..=5 {
            let entry = OutboxEntry {
                event_id: format!("ev-{i}"),
                record: dummy_record(&format!("{i}")),
            };
            assert!(outbox.append(&entry));
        }

        assert_eq!(outbox.pending_count(), 3);
        let state = outbox.read_state();
        assert_eq!(state.dropped, 2);

        // When drain.lock is held by an external process, appends still succeed (soft cap)
        let drain_lock_path = temp.path().join("outbox/drain.lock");
        let external_drain_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&drain_lock_path)
            .unwrap();
        external_drain_lock.lock().unwrap();

        let entry6 = OutboxEntry {
            event_id: "ev-6".into(),
            record: dummy_record("6"),
        };
        assert!(outbox.append(&entry6));
        drop(external_drain_lock);
    }

    #[test]
    fn test_outbox_drain_fifo_order_and_stop_at_first_failure() {
        let temp = TestDir::new("t");
        let outbox = ReverieOutbox::open(temp.path().join("outbox"));

        outbox.append(&OutboxEntry {
            event_id: "ev-1".into(),
            record: dummy_record("1"),
        });
        outbox.append(&OutboxEntry {
            event_id: "ev-2".into(),
            record: dummy_record("2"),
        });
        outbox.append(&OutboxEntry {
            event_id: "ev-3".into(),
            record: dummy_record("3"),
        });

        assert_eq!(outbox.pending_count(), 3);

        // Fail on ev-2
        let res = outbox.drain(|entry| {
            if entry.event_id == "ev-2" {
                Err(EngineError::Reverie("connection refused".into()))
            } else {
                Ok(())
            }
        });
        assert!(res.is_err());
        assert_eq!(outbox.pending_count(), 2);

        // Succeeded on remaining
        let res2 = outbox.drain(|_| Ok(())).unwrap();
        assert_eq!(res2, 2);
        assert_eq!(outbox.pending_count(), 0);
    }

    #[test]
    fn test_outbox_compaction_rewrites_file() {
        let temp = TestDir::new("t");
        let outbox = ReverieOutbox::open(temp.path().join("outbox"));

        for i in 1..=4 {
            outbox.append(&OutboxEntry {
                event_id: format!("ev-{i}"),
                record: dummy_record(&format!("{i}")),
            });
        }

        // Drain 2
        let mut count = 0;
        let _ = outbox.drain(|_| {
            count += 1;
            if count <= 2 {
                Ok(())
            } else {
                Err(EngineError::Reverie("stop".into()))
            }
        });

        assert_eq!(outbox.pending_count(), 2);
        outbox.compact();

        let state = outbox.read_state();
        assert_eq!(state.offset, 0);
        assert_eq!(outbox.pending_count(), 2);
    }

    #[test]
    fn test_outbox_concurrent_appends() {
        let temp = TestDir::new("t");
        let outbox = Arc::new(ReverieOutbox::with_limits(
            temp.path().join("outbox"),
            10_000,
            50 * 1024 * 1024,
        ));

        let outbox_a = Arc::clone(&outbox);
        let t1 = thread::spawn(move || {
            for i in 0..1000 {
                let entry = OutboxEntry {
                    event_id: format!("a-{i}"),
                    record: dummy_record(&format!("a-{i}")),
                };
                assert!(outbox_a.append(&entry));
            }
        });

        let outbox_b = Arc::clone(&outbox);
        let t2 = thread::spawn(move || {
            for i in 0..1000 {
                let entry = OutboxEntry {
                    event_id: format!("b-{i}"),
                    record: dummy_record(&format!("b-{i}")),
                };
                assert!(outbox_b.append(&entry));
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();

        assert_eq!(outbox.pending_count(), 2000);
    }
}
