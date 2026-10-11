// SPDX-License-Identifier: Apache-2.0 OR MIT
//! The 168-byte `ExecEvent` record shared with the kernel program (spec 0014 D6).
//!
//! The layout is `#[repr(C)]` and little-endian. Offsets are fixed by the spec:
//! `timestamp_ns` 0, `pid` 8, `ppid` 12, `uid` 16, `comm` 20, `filename` 36,
//! `ns_pid` 164. The kernel program in `ebpf/src/main.rs` declares the same
//! struct. Both sides assert the size at compile time.
//!
//! Pids: `pid` is the global tgid the kernel knows; `ns_pid` and `ppid` are
//! pids of the daemon's pid namespace (spec 0014 T13). Without a namespace
//! shift the two are equal. `ns_pid` 0 is "unresolved" (D12), and `pid` is then
//! the only name the task has.
//!
//! Records are parsed field by field, never transmuted, so a short or garbled
//! ring-buffer item is rejected instead of read as memory.

use serde::{Deserialize, Serialize};

/// Size of one record in bytes.
pub const EXEC_EVENT_SIZE: usize = 168;
/// Length of the `comm` field (the kernel's `TASK_COMM_LEN`).
pub const COMM_LEN: usize = 16;
/// Length of the `filename` field. Longer paths are truncated by the kernel program.
pub const FILENAME_LEN: usize = 128;

/// One process execution, as the kernel program emits it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecEvent {
    /// Monotonic kernel timestamp (`bpf_ktime_get_ns`).
    pub timestamp_ns: u64,
    /// Global process id (tgid) that executed, as the kernel knows it.
    pub pid: u32,
    /// Parent process id in the daemon's pid namespace, as recorded at fork or track time.
    pub ppid: u32,
    /// Real user id of the executing task.
    pub uid: u32,
    /// Command name after exec, NUL padded.
    pub comm: [u8; COMM_LEN],
    /// Executed path, NUL padded, truncated at 127 bytes plus NUL.
    pub filename: [u8; FILENAME_LEN],
    /// The executing process id in the daemon's pid namespace. Equals `pid`
    /// when there is no namespace shift. 0 means unresolved: the task is in a
    /// nested pid namespace the kernel helper cannot name (spec 0014 D12).
    pub ns_pid: u32,
}

const _: () = assert!(core::mem::size_of::<ExecEvent>() == EXEC_EVENT_SIZE);
const _: () = assert!(core::mem::align_of::<ExecEvent>() == 8);

impl ExecEvent {
    /// Parses one record from the first [`EXEC_EVENT_SIZE`] bytes of `bytes`.
    ///
    /// Returns `None` when `bytes` is shorter than a record.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < EXEC_EVENT_SIZE {
            return None;
        }
        let u64_at = |at: usize| {
            let mut raw = [0u8; 8];
            raw.copy_from_slice(&bytes[at..at + 8]);
            u64::from_le_bytes(raw)
        };
        let u32_at = |at: usize| {
            let mut raw = [0u8; 4];
            raw.copy_from_slice(&bytes[at..at + 4]);
            u32::from_le_bytes(raw)
        };
        let mut comm = [0u8; COMM_LEN];
        comm.copy_from_slice(&bytes[20..20 + COMM_LEN]);
        let mut filename = [0u8; FILENAME_LEN];
        filename.copy_from_slice(&bytes[36..36 + FILENAME_LEN]);
        Some(Self {
            timestamp_ns: u64_at(0),
            pid: u32_at(8),
            ppid: u32_at(12),
            uid: u32_at(16),
            comm,
            filename,
            ns_pid: u32_at(164),
        })
    }

    /// Serializes the record in its wire form. This is the byte string the
    /// `run_hash` digest absorbs (spec 0014, audit hash rule 3).
    pub fn to_bytes(&self) -> [u8; EXEC_EVENT_SIZE] {
        let mut out = [0u8; EXEC_EVENT_SIZE];
        out[0..8].copy_from_slice(&self.timestamp_ns.to_le_bytes());
        out[8..12].copy_from_slice(&self.pid.to_le_bytes());
        out[12..16].copy_from_slice(&self.ppid.to_le_bytes());
        out[16..20].copy_from_slice(&self.uid.to_le_bytes());
        out[20..20 + COMM_LEN].copy_from_slice(&self.comm);
        out[36..36 + FILENAME_LEN].copy_from_slice(&self.filename);
        out[164..168].copy_from_slice(&self.ns_pid.to_le_bytes());
        out
    }

    /// The command name up to the first NUL, lossily decoded.
    pub fn comm_str(&self) -> String {
        cstr_lossy(&self.comm)
    }

    /// The executed path up to the first NUL, lossily decoded.
    pub fn filename_str(&self) -> String {
        cstr_lossy(&self.filename)
    }

    /// The record as owned strings, for JSON frames and logs.
    pub fn record(&self) -> ExecRecord {
        ExecRecord {
            timestamp_ns: self.timestamp_ns,
            pid: self.pid,
            ns_pid: self.ns_pid,
            ppid: self.ppid,
            uid: self.uid,
            comm: self.comm_str(),
            filename: self.filename_str(),
        }
    }

    /// Builds a record from parts, truncating `comm` and `filename` to fit and
    /// keeping a trailing NUL, with `ns_pid == pid` (no namespace shift). Used
    /// by tests and fakes.
    pub fn new(
        timestamp_ns: u64,
        pid: u32,
        ppid: u32,
        uid: u32,
        comm: &str,
        filename: &str,
    ) -> Self {
        let mut comm_bytes = [0u8; COMM_LEN];
        copy_cstr(&mut comm_bytes, comm.as_bytes());
        let mut filename_bytes = [0u8; FILENAME_LEN];
        copy_cstr(&mut filename_bytes, filename.as_bytes());
        Self {
            timestamp_ns,
            pid,
            ppid,
            uid,
            comm: comm_bytes,
            filename: filename_bytes,
            ns_pid: pid,
        }
    }
}

/// An [`ExecEvent`] with its strings decoded. The JSON form of a streamed event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecRecord {
    /// See [`ExecEvent::timestamp_ns`].
    pub timestamp_ns: u64,
    /// See [`ExecEvent::pid`].
    pub pid: u32,
    /// See [`ExecEvent::ns_pid`]. 0 means unresolved: the daemon could not
    /// name the task in its own pid namespace (spec 0014 D12). A frame from an
    /// older daemon has no such field and reads as 0.
    #[serde(default)]
    pub ns_pid: u32,
    /// See [`ExecEvent::ppid`].
    pub ppid: u32,
    /// See [`ExecEvent::uid`].
    pub uid: u32,
    /// See [`ExecEvent::comm`].
    pub comm: String,
    /// See [`ExecEvent::filename`].
    pub filename: String,
}

fn cstr_lossy(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn copy_cstr(dst: &mut [u8], src: &[u8]) {
    let n = src.len().min(dst.len().saturating_sub(1));
    dst[..n].copy_from_slice(&src[..n]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_and_offsets_match_spec() {
        assert_eq!(EXEC_EVENT_SIZE, 168);
        assert_eq!(core::mem::offset_of!(ExecEvent, timestamp_ns), 0);
        assert_eq!(core::mem::offset_of!(ExecEvent, pid), 8);
        assert_eq!(core::mem::offset_of!(ExecEvent, ppid), 12);
        assert_eq!(core::mem::offset_of!(ExecEvent, uid), 16);
        assert_eq!(core::mem::offset_of!(ExecEvent, comm), 20);
        assert_eq!(core::mem::offset_of!(ExecEvent, filename), 36);
        assert_eq!(core::mem::offset_of!(ExecEvent, ns_pid), 164);
    }

    #[test]
    fn roundtrip_through_bytes() {
        let event = ExecEvent::new(42, 1000, 999, 1, "rustc", "/usr/bin/rustc");
        let bytes = event.to_bytes();
        let back = ExecEvent::from_bytes(&bytes).expect("full record");
        assert_eq!(back, event);
        assert_eq!(back.comm_str(), "rustc");
        assert_eq!(back.filename_str(), "/usr/bin/rustc");
        assert_eq!(back.record().filename, "/usr/bin/rustc");
    }

    #[test]
    fn short_input_is_rejected() {
        let event = ExecEvent::new(1, 2, 3, 4, "a", "b");
        let bytes = event.to_bytes();
        assert!(ExecEvent::from_bytes(&bytes[..EXEC_EVENT_SIZE - 1]).is_none());
        assert!(ExecEvent::from_bytes(&[]).is_none());
    }

    #[test]
    fn longer_input_takes_the_first_record() {
        let event = ExecEvent::new(1, 2, 3, 4, "cargo", "/x");
        let mut bytes = event.to_bytes().to_vec();
        bytes.extend_from_slice(&[0xff; 7]);
        assert_eq!(ExecEvent::from_bytes(&bytes), Some(event));
    }

    #[test]
    fn new_truncates_and_keeps_a_nul() {
        let long = "x".repeat(300);
        let event = ExecEvent::new(0, 0, 0, 0, &long, &long);
        assert_eq!(event.comm[COMM_LEN - 1], 0);
        assert_eq!(event.filename[FILENAME_LEN - 1], 0);
        assert_eq!(event.comm_str().len(), COMM_LEN - 1);
        assert_eq!(event.filename_str().len(), FILENAME_LEN - 1);
    }

    #[test]
    fn from_bytes_reads_pid_at_8_and_ns_pid_at_164() {
        // The two ids differ, so reading either field from the wrong offset
        // fails here. Every `ExecEvent::new` fixture has equal ids.
        let mut bytes = [0u8; EXEC_EVENT_SIZE];
        bytes[8..12].copy_from_slice(&4_000_001u32.to_le_bytes());
        bytes[164..168].copy_from_slice(&77u32.to_le_bytes());
        let event = ExecEvent::from_bytes(&bytes).expect("full record");
        assert_eq!(event.pid, 4_000_001);
        assert_eq!(event.ns_pid, 77);
        assert_eq!(event.record().pid, 4_000_001);
        assert_eq!(event.record().ns_pid, 77);
    }

    #[test]
    fn a_record_without_ns_pid_reads_as_unresolved() {
        let json = r#"{"timestamp_ns":1,"pid":8,"ppid":9,"uid":10,"comm":"make","filename":"/usr/bin/make"}"#;
        let record: ExecRecord = serde_json::from_str(json).expect("deserialize");
        assert_eq!(record.pid, 8);
        assert_eq!(record.ns_pid, 0);
    }

    #[test]
    fn record_serializes_to_json() {
        let record = ExecEvent::new(7, 8, 9, 10, "make", "/usr/bin/make").record();
        let json = serde_json::to_string(&record).expect("serialize");
        let back: ExecRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, record);
    }
}
