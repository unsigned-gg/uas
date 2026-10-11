//! Worker slot manager: warm worktree pool, cgroup v2 suspend and resume.
//!
//! Spec 0015 (CER-2722, CER-2723, CER-2724):
//! - [`freezer`]: cgroup v2 freezer and process suspension controller (CER-2722).
//! - [`slot`]: worker slot lifecycle manager and bounded capacity pool (CER-2723).
//! - [`error`]: error types for pool and freezer operations.
//! - [`launch`]: clamped worker spawn and audit track (spec 0015 D6).
#![deny(unsafe_code)]

pub mod error;
pub mod freezer;
pub mod launch;
mod marker;
pub mod slot;
pub mod worktree;

pub use error::{FreezerError, PoolError};
pub use freezer::{
    current_process_cgroup_path, CgroupFreezer, CpuCap, Freezer, FreezerState, ResourceLimits,
    SignalFreezer,
};
pub use launch::{
    spawn_worker, AuditDegradation, LaunchConfig, Worker, WorkerAudit, DEFAULT_DAEMON_SOCKET,
};
pub use slot::{ManagedSlot, PoolConfig, SlotPool, SweepReport};
pub use worktree::WarmWorktree;
