// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Pure types for host-wide slot capacity (spec 0036). No I/O.

use crate::DomainError;

/// The host-wide ceiling on worker slots, shared by every process on the host (spec 0036 D1).
///
/// ```
/// use uas_domain::HostCapacity;
/// assert_eq!(HostCapacity::new(2).unwrap().get(), 2);
/// assert!(HostCapacity::new(0).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostCapacity(usize);

impl HostCapacity {
    /// The default ceiling on ceres (spec 0036 D1).
    pub const DEFAULT: Self = Self(2);

    /// Build a ceiling; zero is refused.
    pub fn new(slots: usize) -> Result<Self, DomainError> {
        if slots == 0 {
            return Err(DomainError::Invalid {
                kind: "host capacity",
                value: slots.to_string(),
                reason: "must be at least 1",
            });
        }
        Ok(Self(slots))
    }

    /// The number of host slots (`H`).
    pub fn get(self) -> usize {
        self.0
    }

    /// The indices `0..H`, in the order they are tried (REQ-0036-03).
    pub fn indices(self) -> impl Iterator<Item = HostSlotIndex> {
        (0..self.0).map(HostSlotIndex)
    }
}

impl Default for HostCapacity {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The index `i` of a host slot, naming `slot-<i>.lock`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HostSlotIndex(usize);

impl HostSlotIndex {
    /// The index as a number.
    pub fn get(self) -> usize {
        self.0
    }

    /// The lock file name, `slot-<i>.lock`.
    pub fn file_name(self) -> String {
        format!("slot-{}.lock", self.0)
    }
}

/// What to do when every host slot is held (spec 0036 D4). Neither policy blocks without a bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotAcquirePolicy {
    /// Fail at once with a host-capacity error.
    #[default]
    Try,
    /// Poll with backoff until a slot frees or the timeout passes.
    WaitBounded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_capacity_is_refused() {
        assert!(HostCapacity::new(0).is_err());
    }

    #[test]
    fn indices_run_from_zero_in_order() {
        let got: Vec<_> = HostCapacity::new(3)
            .unwrap()
            .indices()
            .map(|i| i.get())
            .collect();
        assert_eq!(got, vec![0, 1, 2]);
        assert_eq!(HostSlotIndex(1).file_name(), "slot-1.lock");
    }

    /// REQ-0036-09: this module names no I/O path. The needles are built at run time so this
    /// test does not match itself.
    #[test]
    fn module_source_has_no_io_imports() {
        let src = include_str!("host_capacity.rs");
        let body = src.split("#[cfg(test)]").next().unwrap();
        for needle in [["std::", "net"], ["std::", "fs"], ["to", "kio"]] {
            let needle = needle.concat();
            assert!(!body.contains(&needle), "domain module names {needle}");
        }
    }
}
