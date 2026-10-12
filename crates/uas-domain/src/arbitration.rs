// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Host slot arbitration domain types (spec 0036).
//!
//! Provides pure data types for host-wide slot capacity limits, slot index addressing,
//! and non-hanging acquisition policies with zero I/O imports.

use core::time::Duration;
use serde::{Deserialize, Serialize};

/// Host-wide slot capacity configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCapacity {
    /// Maximum number of active worker slots across all processes on the host.
    pub max_slots: usize,
}

impl HostCapacity {
    /// The default capacity on host ceres (capped to 2 to enforce mandatory -j 2 host throttling).
    pub const DEFAULT_HOST_MAX_SLOTS: usize = 2;

    /// Creates a new host capacity specification.
    ///
    /// Returns `None` if `max_slots` is 0.
    #[must_use]
    pub const fn new(max_slots: usize) -> Option<Self> {
        if max_slots == 0 {
            None
        } else {
            Some(Self { max_slots })
        }
    }

    /// Returns the maximum allowed worker slots.
    #[must_use]
    pub const fn max_slots(&self) -> usize {
        self.max_slots
    }

    /// Validates if an index falls within this capacity.
    #[must_use]
    pub const fn contains(&self, index: HostSlotIndex) -> bool {
        index.0 < self.max_slots
    }
}

impl Default for HostCapacity {
    fn default() -> Self {
        Self {
            max_slots: Self::DEFAULT_HOST_MAX_SLOTS,
        }
    }
}

/// Zero-indexed identifier for a host slot lock file (`slot-<i>.lock`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HostSlotIndex(pub usize);

impl HostSlotIndex {
    /// Creates a new host slot index.
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Returns the numerical index value.
    #[must_use]
    pub const fn as_usize(&self) -> usize {
        self.0
    }
}

/// Policy for acquiring a host slot when all slots are currently occupied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotAcquirePolicy {
    /// Return immediately if no host slot is available.
    Try,
    /// Poll with backoff up to the bounded timeout duration.
    WaitBounded {
        /// Maximum duration to wait before timing out.
        timeout: Duration,
    },
}

impl Default for SlotAcquirePolicy {
    fn default() -> Self {
        Self::WaitBounded {
            timeout: Duration::from_secs(30),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_capacity_default_is_two_for_ceres() {
        let cap = HostCapacity::default();
        assert_eq!(cap.max_slots(), 2);
        assert!(cap.contains(HostSlotIndex::new(0)));
        assert!(cap.contains(HostSlotIndex::new(1)));
        assert!(!cap.contains(HostSlotIndex::new(2)));
    }

    #[test]
    fn host_capacity_rejects_zero() {
        assert!(HostCapacity::new(0).is_none());
        assert!(HostCapacity::new(4).is_some());
    }

    #[test]
    fn slot_acquire_policy_default_is_bounded_30s() {
        let policy = SlotAcquirePolicy::default();
        assert_eq!(
            policy,
            SlotAcquirePolicy::WaitBounded {
                timeout: Duration::from_secs(30)
            }
        );
    }
}
