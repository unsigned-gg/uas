// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Worker slot pool management and lifecycle controller.
//!
//! CER-2723: Enforces host concurrency ceilings (`max_slots = 2`), coordinates
//! domain leases (`uas-domain`), and manages automatic suspension of idle slots.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uas_domain::{
    DomainError, HostCapacity, HostSlotIndex, Lease, LeaseToken, ReconcileReport, Slot,
    SlotAcquirePolicy, SlotId, SlotState, WorktreeRef,
};

use crate::arbitration::{HostSlotArbiter, HostSlotLease, UAS_SLOT_DIR_ENV};
use crate::error::PoolError;
use crate::freezer::{
    CgroupFreezer, CpuCap, Freezer, ResourceLimits, SignalFreezer, SubreaperGuard,
};
use crate::limits::{self, LimitOverrides, LimitSource, ResolvedLimits};
use crate::reconcile::{self, ReconcileRoots};

/// Configuration options for the worker slot pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum concurrent slots allowed in the pool (default 2 on ceres).
    pub max_slots: usize,
    /// Maximum concurrent slots allowed across all processes on the host (spec 0036).
    pub host_max_slots: usize,
    /// Timeout for acquiring a host slot under bounded wait policy (spec 0036).
    pub slot_acquire_timeout: Duration,
    /// Canonical directory for host slot lock files (spec 0036).
    pub host_slot_dir: Option<PathBuf>,
    /// Policy for acquiring host slots when capacity is contested (spec 0036).
    pub slot_acquire_policy: SlotAcquirePolicy,
    /// Inactivity duration before an idle leased slot is suspended.
    pub idle_suspend_duration: Option<Duration>,
    /// Base directory for cgroup v2 hierarchy (None for signal fallback / mock).
    pub cgroup_base: Option<PathBuf>,
    /// Roots to reconcile before the pool admits work (spec 0040 D2). `None`
    /// skips reconciliation.
    pub reconcile: Option<ReconcileRoots>,
    /// Explicit memory, pids and disk overrides; they beat the environment (spec 0041 D7).
    pub limits: LimitOverrides,
    /// Worktree root whose filesystem must keep `min_free_bytes` free (spec 0041 D3).
    /// `None` uses `reconcile.worktree_root` when set, else the current directory.
    pub disk_root: Option<PathBuf>,
    /// Maximum cache directory size per slot in bytes (spec 0047, default 20 GiB).
    pub cache_max_bytes: u64,
    /// Whether sccache compilation wrapper is enabled (spec 0047, default false).
    pub sccache: bool,
    /// Maximum cache size for sccache store in bytes (spec 0047, default 10 GiB).
    pub sccache_max_bytes: u64,
}

impl PoolConfig {
    /// Host slot ceiling used by the memory formula (spec 0041 D1) and host arbitration (spec 0036).
    pub fn host_max_slots(&self) -> usize {
        self.host_max_slots
    }
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_slots: 2,
            host_max_slots: HostCapacity::DEFAULT_HOST_MAX_SLOTS,
            slot_acquire_timeout: Duration::from_secs(30),
            host_slot_dir: None,
            slot_acquire_policy: SlotAcquirePolicy::WaitBounded {
                timeout: Duration::from_secs(30),
            },
            idle_suspend_duration: Some(Duration::from_secs(300)),
            cgroup_base: None,
            reconcile: None,
            limits: LimitOverrides::default(),
            disk_root: None,
            cache_max_bytes: crate::cache::DEFAULT_CACHE_MAX_BYTES,
            sccache: false,
            sccache_max_bytes: crate::cache::DEFAULT_SCCACHE_MAX_BYTES,
        }
    }
}

#[doc(hidden)] // exists for tests: injects the host facts
/// Host facts the pool reads once at construction: `MemTotal`, the environment and `statvfs`.
///
/// [`SlotPool::new`] reads the real host; [`SlotPool::with_host`] takes injected values.
pub struct HostProbe {
    /// `MemTotal` in bytes; `None` when `/proc/meminfo` is unreadable.
    pub mem_total: Option<u64>,
    /// Environment lookup for the `UAS_*` twins.
    pub env: Box<dyn Fn(&str) -> Option<String> + Send + Sync>,
    /// Free bytes on the filesystem holding a path.
    pub free_bytes: Box<dyn Fn(&Path) -> io::Result<u64> + Send + Sync>,
}

impl HostProbe {
    /// The real host: `/proc/meminfo`, `std::env::var` and `statvfs`.
    pub fn real() -> Self {
        Self {
            mem_total: limits::read_mem_total(),
            env: Box::new(|k| std::env::var(k).ok()),
            free_bytes: Box::new(limits::free_bytes_at),
        }
    }
}

/// A managed worker slot in the pool.
pub struct ManagedSlot {
    slot: Slot,
    lease: Option<Lease>,
    host_lease: Option<HostSlotLease>,
    last_activity: Instant,
    freezer: Option<Box<dyn Freezer>>,
    last_eviction: Option<crate::cache::EvictionReport>,
}

impl ManagedSlot {
    /// Create a new managed slot wrapping a domain `Slot`.
    pub fn new(id: SlotId, freezer: Option<Box<dyn Freezer>>) -> Self {
        Self {
            slot: Slot::new(id),
            lease: None,
            host_lease: None,
            last_activity: Instant::now(),
            freezer,
            last_eviction: None,
        }
    }

    /// Associated host slot lease if held (spec 0036).
    pub fn host_lease(&self) -> Option<&HostSlotLease> {
        self.host_lease.as_ref()
    }

    /// Sets or clears the host slot lease (spec 0036).
    pub fn set_host_lease(&mut self, lease: Option<HostSlotLease>) {
        self.host_lease = lease;
    }

    /// Last cache eviction report for this slot (spec 0047).
    pub fn last_eviction(&self) -> Option<crate::cache::EvictionReport> {
        self.last_eviction
    }

    /// Sets the last cache eviction report (spec 0047).
    pub fn set_last_eviction(&mut self, report: Option<crate::cache::EvictionReport>) {
        self.last_eviction = report;
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
    arbiter: Option<HostSlotArbiter>,
    slots: Mutex<BTreeMap<SlotId, ManagedSlot>>,
    reconcile_report: ReconcileReport,
    /// Limits resolved once at construction; the error side is the `MemoryLow` data.
    resolved: Result<ResolvedLimits, (u64, u64)>,
    min_free_bytes: u64,
    free_bytes: Box<dyn Fn(&Path) -> io::Result<u64> + Send + Sync>,
    _subreaper: Option<SubreaperGuard>,
}

impl SlotPool {
    /// Initialize a slot pool with the given configuration.
    ///
    /// When `config.reconcile` is set, the leftovers of dead pools are
    /// reclaimed here, before any slot can be created (spec 0040 D2, D4). A
    /// failed reclaim never stops the pool; it is counted in the report.
    pub fn new(config: PoolConfig) -> Self {
        Self::with_host(config, HostProbe::real())
    }

    /// Like [`Self::new`] with the host facts injected. Exists for tests.
    ///
    /// Resolves the slot limits once (config, then env, then formula), logs the value in
    /// force and its source (spec 0041 D7, REQ-0041-12). A memory limit below the floor does
    /// not stop construction: [`Self::create_slot`] refuses with [`PoolError::MemoryLow`].
    #[doc(hidden)] // exists for tests
    pub fn with_host(config: PoolConfig, host: HostProbe) -> Self {
        let min_free_bytes;
        let resolved = match limits::resolve(
            &config.limits,
            host.mem_total,
            config.host_max_slots(),
            host.env.as_ref(),
        ) {
            Ok(r) => {
                limits::log_start(&r);
                min_free_bytes = r.min_free_bytes;
                Ok(r)
            }
            Err(PoolError::MemoryLow {
                per_slot_bytes,
                floor_bytes,
            }) => {
                // The disk floor is independent of the memory refusal: resolve it with a
                // memory value at the floor so a refused pool still honours its override.
                min_free_bytes = limits::resolve(
                    &LimitOverrides {
                        memory_max_bytes: Some(limits::MEMORY_FLOOR_BYTES),
                        ..config.limits.clone()
                    },
                    None,
                    config.host_max_slots(),
                    host.env.as_ref(),
                )
                .map_or(limits::DEFAULT_MIN_FREE_BYTES, |r| r.min_free_bytes);
                tracing::error!(
                    per_slot_bytes,
                    floor_bytes,
                    "slot memory limit below the floor; create_slot will refuse"
                );
                Err((per_slot_bytes, floor_bytes))
            }
            Err(e) => unreachable!("limits::resolve only returns MemoryLow, got {e}"),
        };
        let reconcile_report = config
            .reconcile
            .as_ref()
            .map(reconcile::reconcile)
            .unwrap_or_default();
        let subreaper = SubreaperGuard::enter().ok();
        let arbiter =
            if config.host_slot_dir.is_some() || std::env::var_os(UAS_SLOT_DIR_ENV).is_some() {
                HostSlotArbiter::new(
                    config.host_slot_dir.clone(),
                    config.host_max_slots(),
                    config.slot_acquire_policy,
                )
                .ok()
            } else {
                None
            };
        Self {
            config,
            arbiter,
            slots: Mutex::new(BTreeMap::new()),
            reconcile_report,
            resolved,
            min_free_bytes,
            free_bytes: host.free_bytes,
            _subreaper: subreaper,
        }
    }

    /// The resolved slot limits, or `None` when the memory limit is below the floor.
    pub fn resolved_limits(&self) -> Option<&ResolvedLimits> {
        self.resolved.as_ref().ok()
    }

    /// Where the memory limit in force came from; `None` when none is set or it was refused.
    pub fn memory_source(&self) -> Option<LimitSource> {
        self.resolved_limits().and_then(|r| r.memory_source)
    }

    /// Reconcile the slot remains under `roots` and return the counts.
    pub fn reconcile(&self, roots: &ReconcileRoots) -> ReconcileReport {
        reconcile::reconcile(roots)
    }

    /// The report of the reconciliation the constructor ran.
    pub fn reconcile_report(&self) -> ReconcileReport {
        self.reconcile_report
    }

    /// Total number of registered slots currently managed by the pool.
    pub fn slot_count(&self) -> usize {
        let guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        guard.len()
    }

    /// Allocates a worker slot in the pool with host-wide slot arbitration (spec 0036).
    ///
    /// Checks capacity and resource limits, acquires an exclusive advisory host slot lease
    /// when an arbiter is active, and registers the managed slot.
    pub fn allocate(&self, id: SlotId) -> Result<(), PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());

        if guard.contains_key(&id) {
            return Err(PoolError::SlotExists(id));
        }

        if guard.len() >= self.config.max_slots {
            return Err(PoolError::CapacityExhausted(self.config.max_slots));
        }

        if let Err((per_slot_bytes, floor_bytes)) = &self.resolved {
            return Err(PoolError::MemoryLow {
                per_slot_bytes: *per_slot_bytes,
                floor_bytes: *floor_bytes,
            });
        }

        let root = self
            .config
            .disk_root
            .clone()
            .or_else(|| {
                self.config
                    .reconcile
                    .as_ref()
                    .map(|r| r.worktree_root.clone())
            })
            .unwrap_or_else(|| PathBuf::from("."));
        let free = (self.free_bytes)(&root).map_err(PoolError::DiskProbe)?;
        limits::check_free(free, self.min_free_bytes)?;

        let host_lease = if let Some(ref arbiter) = self.arbiter {
            Some(arbiter.acquire(&id)?)
        } else {
            None
        };

        // Spec 0047 D1, D4, D11: Initialize slot cache and evict down to cache_max_bytes
        let host_idx = host_lease
            .as_ref()
            .map(|l| l.index())
            .unwrap_or(HostSlotIndex(0));
        let slot_cache = crate::cache::SlotBuildCache::new(host_idx, None);
        let _ = slot_cache.init();
        let eviction_report = slot_cache.evict(self.config.cache_max_bytes).ok();

        // Check slot cache filesystem free space after eviction (REQ-0047-07)
        let cache_root = crate::cache::base_cache_dir();
        let cache_free = (self.free_bytes)(&cache_root).map_err(PoolError::DiskProbe)?;
        limits::check_free(cache_free, self.min_free_bytes)?;

        let freezer: Option<Box<dyn Freezer>> = if let Some(ref base) = self.config.cgroup_base {
            let resource_limits = match &self.resolved {
                Ok(r) => ResourceLimits::from(r),
                Err(_) => ResourceLimits::default(),
            };
            Some(Box::new(CgroupFreezer::with_limits(
                base,
                id.clone(),
                &resource_limits,
            )?))
        } else {
            Some(Box::new(SignalFreezer::new(id.clone())))
        };

        let mut managed = ManagedSlot::new(id.clone(), freezer);
        managed.set_last_eviction(eviction_report);
        managed.set_host_lease(host_lease);
        guard.insert(id, managed);
        Ok(())
    }

    /// Registers a new cold slot in the pool.
    ///
    /// Delegates to [`Self::allocate`] to enforce host-wide capacity ceilings (spec 0036).
    pub fn create_slot(&self, id: SlotId) -> Result<(), PoolError> {
        self.allocate(id)
    }

    /// Removes a slot from the pool, dropping any held host lease and resources.
    pub fn remove_slot(&self, id: &SlotId) -> Result<(), PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        guard
            .remove(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;
        Ok(())
    }

    /// Returns a reference to the host slot arbiter if configured.
    pub fn arbiter(&self) -> Option<&HostSlotArbiter> {
        self.arbiter.as_ref()
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

    /// Reclaims an active or suspended slot, reaping child processes and returning it to `Warm`.
    ///
    /// If a worktree is provided, its working directory is reset to clean HEAD state (spec 0015 D2, REQ-0015-06).
    pub fn reclaim_slot(
        &self,
        id: &SlotId,
        worktree: Option<&crate::worktree::WarmWorktree>,
    ) -> Result<(), PoolError> {
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get_mut(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;

        // If suspended, thaw first
        if managed.slot.state() == SlotState::Suspended {
            if let Some(freezer) = &managed.freezer {
                let _ = freezer.unfreeze();
            }
            let current = managed.slot.clone();
            managed.slot = current.resume()?;
        }

        // Reap worker processes in the freezer
        if let Some(freezer) = &managed.freezer {
            freezer.reap(Duration::from_millis(500))?;
        }

        // Drain any reparented child processes that have exited (Spec 0060 D5)
        SubreaperGuard::reap_zombies();

        // If a worktree is provided, reset it back to clean HEAD
        if let Some(wt) = worktree {
            wt.reset_clean()?;
        }

        // Spec 0047 D4, D7: Evict slot cache down to cache_max_bytes and record report
        let host_idx = managed
            .host_lease()
            .map(|l| l.index())
            .unwrap_or(HostSlotIndex(0));
        let slot_cache = crate::cache::SlotBuildCache::new(host_idx, None);
        if let Ok(eviction) = slot_cache.evict(self.config.cache_max_bytes) {
            managed.set_last_eviction(Some(eviction));
        }

        // Transition from Leased to Warm if leased
        if managed.slot.state() == SlotState::Leased {
            let current = managed.slot.clone();
            managed.slot = current.release()?;
        }

        managed.lease = None;
        managed.last_activity = Instant::now();
        Ok(())
    }

    /// Returns the last eviction report recorded for a slot (spec 0047).
    pub fn slot_last_eviction(
        &self,
        id: &SlotId,
    ) -> Result<Option<crate::cache::EvictionReport>, PoolError> {
        let guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;
        Ok(managed.last_eviction())
    }

    /// Returns the slot build cache manager for a slot (spec 0047).
    pub fn slot_build_cache(&self, id: &SlotId) -> Result<crate::cache::SlotBuildCache, PoolError> {
        let guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;
        let host_idx = managed
            .host_lease()
            .map(|l| l.index())
            .unwrap_or(HostSlotIndex(0));
        Ok(crate::cache::SlotBuildCache::new(host_idx, None))
    }

    /// Attaches the slot's cache metrics to an audit receipt before sealing (spec 0047 D7, REQ-0047-11).
    pub fn seal_audit_receipt(
        &self,
        id: &SlotId,
        mut receipt: uas_ebpf::audit::AuditReceipt,
    ) -> Result<uas_ebpf::audit::AuditReceipt, PoolError> {
        let guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let managed = guard
            .get(id)
            .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;
        if let Some(eviction) = managed.last_eviction() {
            receipt.cache_bytes_before = eviction.bytes_before;
            receipt.cache_bytes_after = eviction.bytes_after;
            receipt.cache_evicted = eviction.evicted_count;
        }
        Ok(receipt)
    }

    /// Reaps any reparented child processes that have exited without blocking (Spec 0060 D5).
    pub fn reap_zombies(&self) -> usize {
        SubreaperGuard::reap_zombies()
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

    /// Spawns a worker in a leased slot, clamped and tracked (spec 0015 D6).
    ///
    /// A slot that is not `Leased` is refused before anything starts.
    pub fn spawn_worker(
        &self,
        id: &SlotId,
        cmd: std::process::Command,
        cfg: &crate::launch::LaunchConfig,
    ) -> Result<crate::launch::Worker, PoolError> {
        let (cpu_cap, cgroup) = {
            let guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            let managed = guard
                .get(id)
                .ok_or_else(|| PoolError::SlotNotFound(id.clone()))?;
            if managed.slot.state() != SlotState::Leased {
                return Err(PoolError::InvalidSlotState(
                    id.clone(),
                    managed.slot.state(),
                    "spawn",
                ));
            }
            let cgroup = managed
                .freezer
                .as_ref()
                .and_then(|f| Some((f.cgroup_path()?, f.limit_events()?)))
                .map(|(path, baseline)| crate::launch::SlotCgroup { path, baseline });
            (managed.cpu_cap().to_string(), cgroup)
        };
        let worker = crate::launch::spawn_worker(cmd, cfg, cpu_cap, cgroup)?;
        let mut guard = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(managed) = guard.get_mut(id) {
            if let Some(ref freezer) = managed.freezer {
                if freezer.cgroup_path().is_none() {
                    freezer.add_pid(worker.id())?;
                }
            }
        }
        Ok(worker)
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
