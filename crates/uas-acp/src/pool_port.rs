// SPDX-License-Identifier: Apache-2.0 OR MIT
//! The slot operations `uas-acp` needs from the worker pool.

use uas_domain::{Lease, LeaseToken, SlotId};
use uas_pool::{PoolError, SlotPool};

/// The three pool calls the ACP server makes. A mock implements it in tests.
pub trait PoolPort {
    /// Acquire the slot named by the lease. Returns the slot id.
    fn acquire(&self, lease: Lease) -> Result<SlotId, PoolError>;
    /// Freeze the slot's processes.
    fn suspend(&self, slot: &SlotId) -> Result<(), PoolError>;
    /// Release the slot's lease. Thaws the slot first if it is frozen.
    fn release(&self, slot: &SlotId, token: &LeaseToken) -> Result<(), PoolError>;
}

impl PoolPort for SlotPool {
    fn acquire(&self, lease: Lease) -> Result<SlotId, PoolError> {
        self.acquire_lease(lease)
    }

    fn suspend(&self, slot: &SlotId) -> Result<(), PoolError> {
        self.suspend_slot(slot)
    }

    fn release(&self, slot: &SlotId, token: &LeaseToken) -> Result<(), PoolError> {
        self.release_lease(slot, token)
    }
}
