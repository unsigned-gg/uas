// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Worker slot manager: warm worktree pool, cgroup v2 suspend and resume.
//!
//! Spec 0015 (CER-2722, CER-2723, CER-2724):
//! - [`freezer`]: cgroup v2 freezer and process suspension controller (CER-2722).
//! - [`slot`]: worker slot lifecycle manager and bounded capacity pool (CER-2723).
//! - [`error`]: error types for pool and freezer operations.
//! - [`launch`]: clamped worker spawn and audit track (spec 0015 D6).
#![deny(unsafe_code)]

pub mod arbitration;
pub mod error;
pub mod freezer;
pub mod launch;
pub mod limits;
mod marker;
pub mod reconcile;
pub mod slot;
pub mod worktree;

pub use arbitration::{
    HostSlotArbiter, HostSlotLease, HostSlotMetadata, DEFAULT_SLOT_DIR, UAS_SLOT_DIR_ENV,
};
pub use error::{FreezerError, PoolError};
pub use freezer::{
    current_process_cgroup_path, is_process_or_group_alive, CgroupFreezer, CpuCap, Freezer,
    FreezerState, ResourceLimits, SignalFreezer, SubreaperGuard,
};
pub use launch::{
    spawn_worker, AuditDegradation, LaunchConfig, SlotCgroup, Worker, WorkerAudit,
    DEFAULT_DAEMON_SOCKET,
};
pub use reconcile::ReconcileRoots;
pub use slot::{HostProbe, ManagedSlot, PoolConfig, SlotPool, SweepReport};
pub use worktree::WarmWorktree;
