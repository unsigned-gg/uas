//! Error types for the worker slot pool and cgroup freezer.

use std::path::PathBuf;

use uas_domain::{DomainError, SlotId};

/// Errors arising from worker slot pool and cgroup operations.
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    /// Slot pool capacity reached.
    #[error("slot pool capacity exhausted (max {0})")]
    CapacityExhausted(usize),

    /// Requested slot does not exist.
    #[error("slot {0} not found")]
    SlotNotFound(SlotId),

    /// Slot id is already registered.
    #[error("slot {0} already exists")]
    SlotExists(SlotId),

    /// Slot is not in an acceptable state for the requested operation.
    #[error("slot {0} is in invalid state {1:?} for operation {2}")]
    InvalidSlotState(SlotId, uas_domain::SlotState, &'static str),

    /// Lease expired on the requested slot.
    #[error("slot {0} lease expired")]
    LeaseExpired(SlotId),

    /// Domain logic error.
    #[error("domain error: {0}")]
    Domain(#[from] DomainError),

    /// Cgroup or signal freezer error.
    #[error("freezer error: {0}")]
    Freezer(#[from] FreezerError),

    /// The supplied lease token did not match.
    #[error("lease token mismatch for slot {0}")]
    TokenMismatch(SlotId),

    /// No active lease held on the slot.
    #[error("no active lease on slot {0}")]
    NoActiveLease(SlotId),

    /// Worker process execution failed.
    #[error("worker process spawn failed: {0}")]
    Spawn(#[from] std::io::Error),

    /// In-process gitoxide operation failed.
    #[error("gitoxide operation failed: {0}")]
    Gitoxide(String),

    /// Reconciliation refused to touch a slot remain (spec 0040 D6).
    #[error("reconcile refused slot {slot}: {reason}")]
    ReconcileRefused {
        /// Slot id, or the control directory name when the marker is unreadable.
        slot: String,
        /// Why the entry was refused.
        reason: String,
    },

    /// eBPF audit client error.
    #[error("ebpf audit client error: {0}")]
    Ebpf(String),
}

/// Errors arising from cgroup v2 freezer operations.
#[derive(Debug, thiserror::Error)]
pub enum FreezerError {
    /// Unsafe or prohibited cgroup path.
    #[error("unsafe cgroup path: {0}")]
    UnsafePath(String),

    /// File I/O error on a cgroup control file.
    #[error("cgroup io error on {path}: {source}")]
    Io {
        /// Path to the control file.
        path: PathBuf,
        /// Underlying IO error.
        #[source]
        source: std::io::Error,
    },

    /// Timed out waiting for state change.
    #[error("freeze timeout waiting for {path} to reach frozen={expected}")]
    Timeout {
        /// Path to cgroup.events.
        path: PathBuf,
        /// Expected frozen state (0 or 1).
        expected: u32,
    },

    /// A slot cgroup was still populated after the reclaim timeout.
    #[error("reclaim timeout: {path} still populated after cgroup.kill")]
    ReclaimTimeout {
        /// Path to cgroup.events.
        path: PathBuf,
    },

    /// Failed to parse a cgroup control or event file.
    #[error("parse error reading cgroup file {path}: {message}")]
    Parse {
        /// File path.
        path: PathBuf,
        /// Error description.
        message: String,
    },

    /// Signal-based freezer error.
    #[error("signal freezer error: {0}")]
    Signal(String),
}
