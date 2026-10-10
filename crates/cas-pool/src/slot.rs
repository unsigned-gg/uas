//! Worker slot pool management and lifecycle controller.
//!
//! CER-2723: Enforces host concurrency ceilings (`max_slots = 2`), coordinates
//! domain leases (`cas-domain`), and manages automatic suspension of idle slots.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cas_domain::{DomainError, Lease, LeaseToken, Slot, SlotId, SlotState, WorktreeRef};

use crate::error::PoolError;
use crate::freezer::{CgroupFreezer, CpuCap, Freezer, ResourceLimits, SignalFreezer};

/// Configuration options for the worker slot pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum concurrent slots allowed in the pool (default 2 on ceres).
    pub max_slots: usize,
    /// Inactivity duration before an idle leased slot is suspended.
    pub idle_suspend_duration: Option<Duration>,
    /// Base directory for cgroup v2 hierarchy (None for signal fallback / mock).
    pub cgroup_base: Option<PathBuf>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_slots: 2,
            idle_suspend_duration: Some(Duration::from_secs(300)),
            cgroup_base: None,
        }
    }
}

/// A managed worker slot in the pool.
pub struct ManagedSlot {
    slot: Slot,
    lease: Option<Lease>,
    last_activity: Instant,
    freezer: Option<Box<dyn Freezer>>,
}

impl ManagedSlot {
    /// Create a new managed slot wrapping a domain `Slot`.
    pub fn new(id: SlotId, freezer: Option<Box<dyn Freezer>>) -> Self {
        Self {
            slot: Slot::new(id),
            lease: None,
            last_activity: Instant::now(),
            freezer,
        }
    }

    /// The slot id.
    pub fn id(&self) -> &SlotId {
        self.slot.id()
    }

    /// Current domain state of the slot.
    pub fn state(&self) -> SlotState {
        self.slot.state()
    }

    /// Associated worktree reference if warmed.
    pub fn worktree(&self) -> Option<&WorktreeRef> {
        self.slot.worktree()
    }

    /// Associated active lease, if held.
    pub fn lease(&self) -> Option<&Lease> {
        self.lease.as_ref()
    }

    /// Instant of most recent activity.
    pub fn last_activity(&self) -> Instant {
        self.last_activity
    }

    /// Mutable reference to the assigned freezer, if present.
    pub fn freezer_mut(&mut self) -> Option<&mut Box<dyn Freezer>> {
        self.freezer.as_mut()
    }

    /// Returns the CPU cap recorded by the assigned freezer, or [`CpuCap::Uncapped`] if none.
    pub fn cpu_cap(&self) -> CpuCap {
        self.freezer
            .as_ref()
            .map(|f| f.cpu_cap())
            .unwrap_or(CpuCap::Uncapped)
    }
}

/// Results of sweeping the pool for idle or expired slots.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SweepReport {
    /// Slots automatically suspended due to inactivity.
    pub suspended: Vec<SlotId>,
    /// Slots whose leases have reached expiration.
    pub expired: Vec<SlotId>,
}

/// Thread-safe worker slot pool manager.
pub struct SlotPool {
    config: PoolConfig,
    slots: Mutex<BTreeMap<SlotId, ManagedSlot>>,
}

impl SlotPool {
    /// Initialize a slot pool with the given configuration.
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            slots: Mutex::new(BTreeMap::new()),
        }
    }

    /// Total number of registered slots currently managed by the pool.
    pub fn slot_count(&self) -> usize {
        let guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        guard.len()
    }

    /// Registers a new cold slot in the pool.
    ///
    /// Returns [`PoolError::CapacityExhausted`] if the pool is already at or
    /// above `config.max_slots`.
    pub fn create_slot(&self, id: SlotId) -> Result<(), PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());

        if guard.contains_key(&id) {
            return Err(PoolError::SlotExists(id));
        }

        if guard.len() >= self.config.max_slots {
            return Err(PoolError::CapacityExhausted(self.config.max_slots));
        }

        let freezer: Option<Box<dyn Freezer>> = if let Some(ref base) = self.config.cgroup_base {
            match CgroupFreezer::new(base, id.clone()) {
                Ok(cg) => Some(Box::new(cg)),
                Err(e) => {
                    tracing::warn!("cgroup freezer creation failed for slot {id}: {e}; falling back to signal freezer");
                    Some(Box::new(SignalFreezer::new(id.clone())))
                }
            }
        } else {
            Some(Box::new(SignalFreezer::new(id.clone())))
        };

        let managed = ManagedSlot::new(id.clone(), freezer);
        guard.insert(id, managed);
        Ok(())
    }

    /// Attaches a worktree reference and transitions a slot from `Cold` to `Warm`.
    pub fn warm_slot(&self, id: &SlotId, worktree: WorktreeRef) -> Result<(), PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get_mut(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;

        let current = managed.slot.clone();
        let next = current.warm(worktree)?;
        managed.slot = next;
        managed.last_activity = Instant::now();
        Ok(())
    }

    /// Acquires a lease on a warm slot according to spec 0007 rules.
    pub fn acquire_lease(&self, lease: Lease) -> Result<SlotId, PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let slot_id = lease.slot.clone();

        let managed = guard
            .get_mut(&slot_id)
            .ok_or_else(|| PoolError::SlotNotFound(slot_id.clone()))?;

        // Rule R1: check if slot currently holds an unexpired lease
        if let Some(existing) = &managed.lease {
            if existing.expires_at > lease.issued_at {
                return Err(PoolError::Domain(DomainError::SlotLeased {
                    slot: slot_id.as_str().to_string(),
                    lease: existing.id.as_str().to_string(),
                    until: existing.expires_at,
                }));
            }
        }

        if managed.slot.state() != SlotState::Warm {
            return Err(PoolError::InvalidSlotState(
                slot_id.clone(),
                managed.slot.state(),
                "lease",
            ));
        }

        let current = managed.slot.clone();
        let next = current.lease()?;
        managed.slot = next;
        managed.lease = Some(lease);
        managed.last_activity = Instant::now();
        Ok(slot_id)
    }

    /// Renews an active lease per spec 0007 rules R2 and R3.
    pub fn renew_lease(
        &self,
        id: &SlotId,
        token: &LeaseToken,
        now: u64,
        extend_by: u64,
    ) -> Result<(), PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get_mut(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;

        let lease = managed
            .lease
            .as_mut()
            .ok_or_else(|| PoolError::NoActiveLease(id.clone()))?;

        // R2: token comparison
        if !token.matches(&lease.token) {
            return Err(PoolError::TokenMismatch(id.clone()));
        }

        // Expiry check
        if lease.expires_at <= now {
            return Err(PoolError::LeaseExpired(id.clone()));
        }

        // R3: extend_by >= 1
        if extend_by == 0 {
            return Err(PoolError::Domain(DomainError::Invalid {
                kind: "lease renewal",
                value: "0".to_string(),
                reason: "renewal must extend by at least one second",
            }));
        }

        let new_expires =
            lease
                .expires_at
                .checked_add(extend_by)
                .ok_or_else(|| DomainError::Invalid {
                    kind: "lease renewal",
                    value: format!("{}+{}", lease.expires_at, extend_by),
                    reason: "expiry overflow",
                })?;

        lease.expires_at = new_expires;
        managed.last_activity = Instant::now();
        Ok(())
    }

    /// Suspends an active leased slot and invokes the freezer.
    pub fn suspend_slot(&self, id: &SlotId) -> Result<(), PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get_mut(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;

        if managed.slot.state() != SlotState::Leased {
            return Err(PoolError::InvalidSlotState(
                id.clone(),
                managed.slot.state(),
                "suspend",
            ));
        }

        if let Some(freezer) = &managed.freezer {
            freezer.freeze()?;
        }

        let current = managed.slot.clone();
        let next = current.suspend()?;
        managed.slot = next;
        Ok(())
    }

    /// Resumes a suspended slot, unfreezing worker processes.
    pub fn resume_slot(&self, id: &SlotId) -> Result<(), PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get_mut(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;

        if managed.slot.state() != SlotState::Suspended {
            return Err(PoolError::InvalidSlotState(
                id.clone(),
                managed.slot.state(),
                "resume",
            ));
        }

        if let Some(freezer) = &managed.freezer {
            freezer.unfreeze()?;
        }

        let current = managed.slot.clone();
        let next = current.resume()?;
        managed.slot = next;
        managed.last_activity = Instant::now();
        Ok(())
    }

    /// Records activity on a slot, resuming it if suspended.
    pub fn record_activity(&self, id: &SlotId) -> Result<(), PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get_mut(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;

        managed.last_activity = Instant::now();

        if managed.slot.state() == SlotState::Suspended {
            if let Some(freezer) = &managed.freezer {
                freezer.unfreeze()?;
            }
            let current = managed.slot.clone();
            let next = current.resume()?;
            managed.slot = next;
        }

        Ok(())
    }

    /// Releases a lease, returning the slot from `Leased` to `Warm`.
    pub fn release_lease(&self, id: &SlotId, token: &LeaseToken) -> Result<(), PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get_mut(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;

        let lease = managed
            .lease
            .as_ref()
            .ok_or_else(|| PoolError::NoActiveLease(id.clone()))?;

        if !token.matches(&lease.token) {
            return Err(PoolError::TokenMismatch(id.clone()));
        }

        // If suspended, thaw first
        if managed.slot.state() == SlotState::Suspended {
            if let Some(freezer) = &managed.freezer {
                freezer.unfreeze()?;
            }
            let current = managed.slot.clone();
            managed.slot = current.resume()?;
        }

        let current = managed.slot.clone();
        let next = current.release()?;
        managed.slot = next;
        managed.lease = None;
        managed.last_activity = Instant::now();
        Ok(())
    }

    /// Drains a slot, terminating workers and moving state to `Draining`.
    pub fn drain_slot(&self, id: &SlotId) -> Result<(), PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get_mut(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;

        if let Some(freezer) = &managed.freezer {
            let _ = freezer.unfreeze();
        }

        let current = managed.slot.clone();
        let next = current.drain()?;
        managed.slot = next;
        managed.lease = None;
        Ok(())
    }

    /// Returns the current state of a slot.
    pub fn slot_state(&self, id: &SlotId) -> Result<SlotState, PoolError> {
        let guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;
        Ok(managed.slot.state())
    }

    /// Returns a clone of the inner domain [`Slot`].
    pub fn slot(&self, id: &SlotId) -> Result<Slot, PoolError> {
        let guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;
        Ok(managed.slot.clone())
    }

    /// Returns the worktree reference of a slot if attached.
    pub fn slot_worktree(&self, id: &SlotId) -> Result<Option<WorktreeRef>, PoolError> {
        let guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;
        Ok(managed.slot.worktree().cloned())
    }

    /// Sweeps the pool for idle slots (to suspend) and expired leases.
    pub fn sweep(
        &self,
        now_epoch_secs: u64,
        now_instant: Instant,
    ) -> Result<SweepReport, PoolError> {
        let mut report = SweepReport::default();
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());

        for (id, managed) in guard.iter_mut() {
            // Check expiry
            if let Some(lease) = &managed.lease {
                if lease.expires_at <= now_epoch_secs {
                    report.expired.push(id.clone());
                }
            }

            // Check idle suspension on Leased slots
            if managed.slot.state() == SlotState::Leased {
                if let Some(idle_dur) = self.config.idle_suspend_duration {
                    if now_instant.duration_since(managed.last_activity) >= idle_dur {
                        if let Some(freezer) = &managed.freezer {
                            let _ = freezer.freeze();
                        }
                        if let Ok(next) = managed.slot.clone().suspend() {
                            managed.slot = next;
                            report.suspended.push(id.clone());
                        }
                    }
                }
            }
        }

        Ok(report)
    }

    /// Returns the CPU cap status recorded for a slot.
    pub fn slot_cpu_cap(&self, id: &SlotId) -> Result<CpuCap, PoolError> {
        let guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;
        Ok(managed.cpu_cap())
    }

    /// Configures resource constraints (CPU/memory) for a slot.
    pub fn configure_slot_limits(
        &self,
        id: &SlotId,
        limits: &ResourceLimits,
    ) -> Result<CpuCap, PoolError> {
        let guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;
        if let Some(ref freezer) = managed.freezer {
            let cap = freezer.configure_limits(limits)?;
            Ok(cap)
        } else {
            Ok(CpuCap::Uncapped)
        }
    }
}
