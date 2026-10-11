// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Types for restart reconciliation (spec 0040). Pure data, no I/O.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::LeaseId;

/// Counts from one pool reconciliation pass over leftover slots (spec 0040 D7).
///
/// `reclaimed` slots were cleaned up, `kept` slots belong to a live owner and
/// were left alone, and `failed` slots hit a reclaim error and are retried at
/// the next start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconcileReport {
    /// Orphaned slots reclaimed.
    pub reclaimed: u32,
    /// Slots left untouched because another process holds their lock.
    pub kept: u32,
    /// Slots whose reclaim failed and will be retried.
    pub failed: u32,
}

/// The compensation reason for a lease the pool no longer knows (spec 0040 D5).
///
/// Leases do not survive a pool restart, so a replayed activity that gets
/// `LeaseNotFound` has lost its slot. The engine runs the compensators with
/// this reason and then re-admits the task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LostSlot {
    lease: LeaseId,
}

impl LostSlot {
    /// Build the reason for the lease that was lost.
    pub fn new(lease: &LeaseId) -> Self {
        Self {
            lease: lease.clone(),
        }
    }

    /// The lease the pool no longer knows.
    pub fn lease(&self) -> &LeaseId {
        &self.lease
    }
}

impl fmt::Display for LostSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "lost slot: lease {} is unknown to the pool after a restart",
            self.lease.as_str()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LeaseId;

    #[test]
    fn reconcile_report_default_is_zero() {
        let report = ReconcileReport::default();
        assert_eq!((report.reclaimed, report.kept, report.failed), (0, 0, 0));
    }

    #[test]
    fn reconcile_lost_slot_reason_names_the_lease() {
        let lease = LeaseId::new("lease-7").unwrap();
        let text = LostSlot::new(&lease).to_string();
        assert!(text.contains("lease-7"), "{text}");
        assert!(!text.contains('\n'), "{text}");
    }

    #[test]
    fn reconcile_report_round_trips_through_json() {
        let report = ReconcileReport {
            reclaimed: 2,
            kept: 1,
            failed: 3,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert_eq!(
            serde_json::from_str::<ReconcileReport>(&json).unwrap(),
            report
        );
    }
}
