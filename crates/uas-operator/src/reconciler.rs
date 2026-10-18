// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Reconcilers for `WorkerSlot` and `AgentSandbox` custom resources.
//!
//! Spec 0038 D1, D4, D5, D6, D7, D8: bridges Kubernetes custom resources to
//! `uas-domain`, `uas-pool` (including Spec 0036 host slot locks and Spec 0041
//! resource limit overrides), and `uas-sandbox` (Spec 0010 Landlock ruleset
//! plans and Spec 0013 IPC socket directories).

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use thiserror::Error;
use uas_domain::{
    Budget, DomainError, EvidenceTier, HostCapacity, LandingReceipt, Lease, LeaseId, LeaseToken,
    PrincipalRef, Slot, SlotId, SlotState, WorktreeRef,
};
use uas_pool::limits::{LimitOverrides, ResolvedLimits};
use uas_pool::slot::arbitration::{HostSlotArbiter, HostSlotLease};
use uas_pool::{HostProbe, PoolConfig, PoolError, SlotPool};
use uas_sandbox::{
    ipc, LandlockRulesetBuilder, PathAccess, PlannedRule, RulesetPlan, SandboxError,
};

use crate::crd::{
    AgentSandbox, ConditionStatus, SlotLimitsSpec, WorkerSlot, SLOT_CLEANUP_FINALIZER,
};

/// Default requeue delay when all host slots on a node are saturated (spec 0038 REQ-0038-07).
pub const CAPACITY_EXHAUSTED_REQUEUE: Duration = Duration::from_secs(5);

/// Default requeue delay after a normal state transition.
pub const STEADY_REQUEUE: Duration = Duration::from_secs(60);

/// Operating mode of the `uas-operator` binary (spec 0038 D8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorMode {
    /// Cluster-wide controller Deployment: validates specs, custody receipts, and node placement.
    Controller,
    /// Node-local DaemonSet agent: acquires Spec 0036 host slot locks, cgroups, and sandboxes.
    NodeAgent,
}

impl std::str::FromStr for OperatorMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "controller" => Ok(Self::Controller),
            "node-agent" | "node_agent" | "nodeagent" => Ok(Self::NodeAgent),
            other => Err(format!(
                "invalid operator mode '{other}': expected 'controller' or 'node-agent'"
            )),
        }
    }
}

/// Outcome of a single reconciliation pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileDecision {
    /// Resource reached its desired state and should be re-checked after `requeue_after`.
    Reconciled {
        /// Duration before the next periodic check.
        requeue_after: Duration,
    },
    /// Resource was already at the desired state for its generation; no mutation was needed.
    NoOp,
    /// Host slot capacity was exhausted (`WouldBlock`); resource was marked `Ready=False`
    /// with `reason = "CapacityExhausted"` and must be requeued after `requeue_after`.
    RequeueCapacityExhausted {
        /// Backoff duration before retrying slot acquisition.
        requeue_after: Duration,
    },
    /// Resource failed a terminal invariant (such as custody receipt verification) and must not retry.
    TerminalFailure {
        /// Machine-readable reason code.
        reason: String,
    },
    /// Resource had a deletion timestamp, released its host slot lock, and removed its finalizer.
    Finalized,
}

/// Errors produced by the operator reconciler or host lock table.
#[derive(Debug, Error)]
pub enum ReconcileError {
    /// Domain state transition or identifier error.
    #[error("domain error: {0}")]
    Domain(#[from] DomainError),

    /// Slot pool error.
    #[error("pool error: {0}")]
    Pool(#[from] PoolError),

    /// Sandbox ruleset or IPC error.
    #[error("sandbox error: {0}")]
    Sandbox(#[from] SandboxError),

    /// Host slot lock directory or file I/O failure.
    #[error("host slot lock I/O error at {path}: {source}")]
    SlotLockIo {
        /// Path that failed validation or open.
        path: PathBuf,
        /// Underlying OS error.
        #[source]
        source: io::Error,
    },

    /// Host slot lock directory failed Spec 0036 D2 ownership or permission checks.
    #[error("invalid host slot directory {path}: {reason}")]
    InvalidSlotDir {
        /// Directory path.
        path: PathBuf,
        /// Reason the directory was rejected.
        reason: String,
    },

    /// All `0..max_slots` host slot locks on the node are held by other processes.
    #[error("all {max_slots} host slot locks are busy")]
    CapacityExhausted {
        /// Host slot ceiling (`H`).
        max_slots: usize,
    },

    /// Invalid desired state string in `WorkerSlotSpec.desired_state`.
    #[error("unsupported desired_state '{0}'")]
    UnsupportedDesiredState(String),
}

/// A host slot lease held for one `WorkerSlot` (spec 0036 D3). Dropping it frees the lock.
#[derive(Debug)]
struct HeldHostLock {
    index: usize,
    _lease: HostSlotLease,
}

/// Cross-process host slot lock table over the `uas-pool` arbiter (spec 0036 D2, D6).
///
/// The arbiter owns the lock files and descriptors. This table only maps a `WorkerSlot` key to
/// the lease it holds, and drops the lease (without unlinking the file) when the slot
/// transitions to `Cold`/`Terminated` or its finalizer runs.
#[derive(Debug)]
pub struct HostSlotLockTable {
    arbiter: HostSlotArbiter,
    held: Mutex<BTreeMap<String, HeldHostLock>>,
}

impl HostSlotLockTable {
    /// Create a host slot lock table rooted at `slot_dir` with ceiling `host_max_slots`.
    pub fn new(
        slot_dir: impl Into<PathBuf>,
        host_max_slots: usize,
    ) -> Result<Self, ReconcileError> {
        let capacity = HostCapacity::new(host_max_slots.max(1)).map_err(ReconcileError::Domain)?;
        let arbiter = HostSlotArbiter::new(slot_dir, capacity);
        arbiter.ensure_dir().map_err(map_arbiter_error)?;
        Ok(Self {
            arbiter,
            held: Mutex::new(BTreeMap::new()),
        })
    }

    /// Directory holding `slot-0.lock` .. `slot-(H-1).lock`.
    pub fn slot_dir(&self) -> &Path {
        self.arbiter.dir()
    }

    /// Maximum concurrent host slots (`H`).
    pub fn host_max_slots(&self) -> usize {
        self.arbiter.capacity().get()
    }

    /// Acquire one of `slot-0.lock` .. `slot-(H-1).lock` for `slot_key`, or return the
    /// already-held index if `slot_key` already owns a lock.
    pub fn acquire(&self, slot_key: &str) -> Result<usize, ReconcileError> {
        let mut held = self.held.lock().expect("host slot lock mutex poisoned");
        if let Some(existing) = held.get(slot_key) {
            return Ok(existing.index);
        }
        let lease = self.arbiter.try_acquire().map_err(map_arbiter_error)?;
        let index = lease.index().get();
        held.insert(
            slot_key.to_string(),
            HeldHostLock {
                index,
                _lease: lease,
            },
        );
        Ok(index)
    }

    /// Release the open lock descriptor held by `slot_key`, if any, without unlinking the file.
    pub fn release(&self, slot_key: &str) -> bool {
        let mut held = self.held.lock().expect("host slot lock mutex poisoned");
        held.remove(slot_key).is_some()
    }

    /// Check whether `slot_key` currently holds a host slot lock in this process.
    pub fn held_index(&self, slot_key: &str) -> Option<usize> {
        let held = self.held.lock().expect("host slot lock mutex poisoned");
        held.get(slot_key).map(|h| h.index)
    }
}

/// Map an arbiter error onto the reconciler's error type.
fn map_arbiter_error(err: PoolError) -> ReconcileError {
    match err {
        PoolError::HostCapacityExceeded(max_slots) => {
            ReconcileError::CapacityExhausted { max_slots }
        }
        PoolError::DirectoryInsecure(path, reason) => {
            ReconcileError::InvalidSlotDir { path, reason }
        }
        PoolError::HostSlotIo { path, source } => ReconcileError::SlotLockIo { path, source },
        other => ReconcileError::Pool(other),
    }
}

/// Build a [`PoolConfig`] for a [`WorkerSlot`], mapping `spec.limits` into
/// [`LimitOverrides`] so explicit CRD limits beat `UAS_SLOT_MEMORY_MAX` and
/// `UAS_SLOT_PIDS_MAX` environment variables (spec 0038 REQ-0038-13).
pub fn pool_config_for_slot(slot: &WorkerSlot, host_max_slots: usize) -> PoolConfig {
    let limits = slot
        .spec
        .limits
        .as_ref()
        .map(limits_from_spec)
        .unwrap_or_default();
    PoolConfig {
        max_slots: host_max_slots.max(1),
        limits,
        ..PoolConfig::default()
    }
}

fn limits_from_spec(spec: &SlotLimitsSpec) -> LimitOverrides {
    LimitOverrides {
        memory_max_bytes: spec.memory_max_bytes,
        memory_high_bytes: spec.memory_high_bytes,
        pids_max: spec.pids_max,
        min_free_bytes: None,
    }
}

/// Resolve effective [`ResolvedLimits`] for a [`WorkerSlot`] using an injected [`HostProbe`].
pub fn resolve_slot_limits_with_probe(
    slot: &WorkerSlot,
    host_max_slots: usize,
    probe: HostProbe,
) -> Result<ResolvedLimits, PoolError> {
    let config = pool_config_for_slot(slot, host_max_slots);
    let pool = SlotPool::with_host(config, probe);
    pool.resolved_limits().cloned().ok_or(PoolError::MemoryLow {
        per_slot_bytes: 0,
        floor_bytes: uas_pool::limits::MEMORY_FLOOR_BYTES,
    })
}

/// Stateful reconciler for [`WorkerSlot`] and [`AgentSandbox`] resources.
pub struct OperatorReconciler {
    mode: OperatorMode,
    node_name: String,
    lock_table: Arc<HostSlotLockTable>,
}

impl OperatorReconciler {
    /// Construct a new reconciler for the given mode, node name, and host slot lock table.
    pub fn new(
        mode: OperatorMode,
        node_name: impl Into<String>,
        lock_table: Arc<HostSlotLockTable>,
    ) -> Self {
        Self {
            mode,
            node_name: node_name.into(),
            lock_table,
        }
    }

    /// Operating mode (`Controller` or `NodeAgent`).
    pub fn mode(&self) -> OperatorMode {
        self.mode
    }

    /// Host slot lock table used for Spec 0036 lock acquisition.
    pub fn lock_table(&self) -> &Arc<HostSlotLockTable> {
        &self.lock_table
    }

    /// Reconcile a [`WorkerSlot`] resource in-place, updating `metadata.finalizers` and `status`.
    ///
    /// Time is passed explicitly as `now_epoch_secs` so tests remain deterministic.
    pub fn reconcile_worker_slot(
        &self,
        slot: &mut WorkerSlot,
        now_epoch_secs: u64,
    ) -> Result<ReconcileDecision, ReconcileError> {
        self.reconcile_worker_slot_with_probe(slot, now_epoch_secs, HostProbe::real())
    }

    /// Reconcile a [`WorkerSlot`] resource with an injected [`HostProbe`] for deterministic limit tests.
    pub fn reconcile_worker_slot_with_probe(
        &self,
        slot: &mut WorkerSlot,
        now_epoch_secs: u64,
        probe: HostProbe,
    ) -> Result<ReconcileDecision, ReconcileError> {
        let slot_key = slot
            .metadata
            .name
            .clone()
            .unwrap_or_else(|| slot.spec.slot_id.clone());

        // 1. Handle deletion & finalizer cleanup (REQ-0038-08).
        if is_deleting(slot) {
            self.lock_table.release(&slot_key);
            remove_finalizer(slot, SLOT_CLEANUP_FINALIZER);
            let mut status = slot.status.take().unwrap_or_default();
            status.phase = Some("Terminated".to_string());
            status.host_slot_index = None;
            set_condition(
                &mut status.conditions,
                ConditionStatus {
                    type_: "Ready".to_string(),
                    status: "False".to_string(),
                    reason: "Terminated".to_string(),
                    message: "host slot lock released and slot finalized".to_string(),
                    last_transition_time: now_epoch_secs,
                },
            );
            slot.status = Some(status);
            return Ok(ReconcileDecision::Finalized);
        }

        // 2. Ensure cleanup finalizer is registered before acquiring resources (REQ-0038-08).
        ensure_finalizer(slot, SLOT_CLEANUP_FINALIZER);

        // Assign node_name if unset (Spec 0038 D8).
        if slot.spec.node_name.is_none() {
            slot.spec.node_name = Some(self.node_name.clone());
        }

        // 3. Verify custody receipt invariant before any state transition (REQ-0038-10).
        if let Err(custody_msg) = verify_custody_receipt(slot) {
            self.lock_table.release(&slot_key);
            let mut status = slot.status.take().unwrap_or_default();
            status.phase = Some("Failed".to_string());
            status.observed_generation = slot.metadata.generation;
            status.host_slot_index = None;
            status.verified_landing = false;
            set_condition(
                &mut status.conditions,
                ConditionStatus {
                    type_: "CustodyReceiptInvalid".to_string(),
                    status: "False".to_string(),
                    reason: "CustodyBypassRefused".to_string(),
                    message: custody_msg.clone(),
                    last_transition_time: now_epoch_secs,
                },
            );
            set_condition(
                &mut status.conditions,
                ConditionStatus {
                    type_: "Ready".to_string(),
                    status: "False".to_string(),
                    reason: "CustodyBypassRefused".to_string(),
                    message: custody_msg,
                    last_transition_time: now_epoch_secs,
                },
            );
            slot.status = Some(status);
            return Ok(ReconcileDecision::TerminalFailure {
                reason: "CustodyBypassRefused".to_string(),
            });
        }

        let desired_state = parse_desired_state(&slot.spec.desired_state)?;

        // 4. Idempotency check (REQ-0038-06): if generation, phase, and Ready=True already match, no-op.
        if let Some(existing) = &slot.status {
            let phase_matches = existing.phase.as_deref() == Some(slot_state_label(desired_state));
            let gen_matches = existing.observed_generation == slot.metadata.generation;
            let ready_true = existing
                .conditions
                .iter()
                .any(|c| c.type_ == "Ready" && c.status == "True");
            if phase_matches && gen_matches && ready_true {
                return Ok(ReconcileDecision::NoOp);
            }
        }

        // 5. Resolve cgroup v2 limits via uas-pool (REQ-0038-13).
        let resolved_limits =
            resolve_slot_limits_with_probe(slot, self.lock_table.host_max_slots(), probe)?;

        // 6. Acquire or release Spec 0036 host slot lock depending on desired_state (REQ-0038-07).
        let needs_host_lock = matches!(
            desired_state,
            SlotState::Warm | SlotState::Leased | SlotState::Suspended
        );
        let host_slot_index = if needs_host_lock {
            match self.lock_table.acquire(&slot_key) {
                Ok(idx) => Some(idx),
                Err(ReconcileError::CapacityExhausted { max_slots }) => {
                    let mut status = slot.status.take().unwrap_or_default();
                    status.phase = Some("Cold".to_string());
                    status.observed_generation = slot.metadata.generation;
                    status.host_slot_index = None;
                    status.resolved_memory_max_bytes = resolved_limits.memory_max_bytes;
                    status.resolved_pids_max = Some(resolved_limits.pids_max);
                    set_condition(
                        &mut status.conditions,
                        ConditionStatus {
                            type_: "Ready".to_string(),
                            status: "False".to_string(),
                            reason: "CapacityExhausted".to_string(),
                            message: format!(
                                "all {max_slots} host slot locks under {} are held",
                                self.lock_table.slot_dir().display()
                            ),
                            last_transition_time: now_epoch_secs,
                        },
                    );
                    slot.status = Some(status);
                    return Ok(ReconcileDecision::RequeueCapacityExhausted {
                        requeue_after: CAPACITY_EXHAUSTED_REQUEUE,
                    });
                }
                Err(err) => return Err(err),
            }
        } else {
            self.lock_table.release(&slot_key);
            None
        };

        // 7. Drive the pure uas-domain::Slot state machine through the requested transition.
        let domain_slot_id = SlotId::new(slot.spec.slot_id.clone())?;
        let domain_slot = Slot::new(domain_slot_id.clone());
        let (final_slot, lease_expires_at) = match desired_state {
            SlotState::Cold => (domain_slot, None),
            SlotState::Warm => {
                let wt = WorktreeRef::new(
                    slot.spec
                        .worktree_ref
                        .clone()
                        .unwrap_or_else(|| format!("worktrees/{}", slot.spec.slot_id)),
                )?;
                (domain_slot.warm(wt)?, None)
            }
            SlotState::Leased => {
                let wt = WorktreeRef::new(
                    slot.spec
                        .worktree_ref
                        .clone()
                        .unwrap_or_else(|| format!("worktrees/{}", slot.spec.slot_id)),
                )?;
                let warmed = domain_slot.warm(wt)?;
                let leased = warmed.lease()?;
                let lease_id = LeaseId::new(
                    slot.spec
                        .lease_id
                        .clone()
                        .unwrap_or_else(|| format!("lease-{}", slot.spec.slot_id)),
                )?;
                let principal = PrincipalRef::new(
                    slot.spec
                        .principal
                        .clone()
                        .unwrap_or_else(|| "uas-operator".to_string()),
                )?;
                let token = LeaseToken::new(format!("token-{}", slot.spec.slot_id))?;
                let ttl = slot.spec.lease_ttl_seconds.unwrap_or(300).max(1);
                let budget = Budget::new(100_000, ttl)?;
                let lease = Lease::new(
                    lease_id,
                    domain_slot_id,
                    principal,
                    token,
                    now_epoch_secs,
                    now_epoch_secs.saturating_add(ttl),
                    budget,
                )?;
                (leased, Some(lease.expires_at))
            }
            SlotState::Suspended => {
                let wt = WorktreeRef::new(
                    slot.spec
                        .worktree_ref
                        .clone()
                        .unwrap_or_else(|| format!("worktrees/{}", slot.spec.slot_id)),
                )?;
                let suspended = domain_slot.warm(wt)?.lease()?.suspend()?;
                (suspended, None)
            }
            SlotState::Draining => (domain_slot.drain()?, None),
        };

        let verified_landing = slot
            .spec
            .landing_receipt_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<LandingReceipt>(raw).ok())
            .is_some_and(|r| r.evidence() == EvidenceTier::Verified);

        let mut status = slot.status.take().unwrap_or_default();
        status.phase = Some(slot_state_label(final_slot.state()).to_string());
        status.lease_expires_at = lease_expires_at;
        status.observed_generation = slot.metadata.generation;
        status.host_slot_index = host_slot_index;
        status.resolved_memory_max_bytes = resolved_limits.memory_max_bytes;
        status.resolved_pids_max = Some(resolved_limits.pids_max);
        status.verified_landing = verified_landing;
        set_condition(
            &mut status.conditions,
            ConditionStatus {
                type_: "Ready".to_string(),
                status: "True".to_string(),
                reason: "Reconciled".to_string(),
                message: format!(
                    "slot {} reconciled to {}",
                    slot.spec.slot_id,
                    slot_state_label(final_slot.state())
                ),
                last_transition_time: now_epoch_secs,
            },
        );
        slot.status = Some(status);

        Ok(ReconcileDecision::Reconciled {
            requeue_after: STEADY_REQUEUE,
        })
    }

    /// Reconcile an [`AgentSandbox`] resource, translating its spec into a
    /// [`uas_sandbox::RulesetPlan`] and preparing its `.ipc` directory (REQ-0038-09).
    pub fn reconcile_agent_sandbox(
        &self,
        sandbox: &mut AgentSandbox,
        now_epoch_secs: u64,
    ) -> Result<(ReconcileDecision, RulesetPlan), ReconcileError> {
        let workspace = Path::new(&sandbox.spec.workspace_path);
        let mut builder =
            LandlockRulesetBuilder::worker(workspace, sandbox.spec.toolchain_paths.iter());
        for extra in &sandbox.spec.extra_rules {
            let access: PathAccess = extra.access.to_path_access();
            builder = builder.allow(&extra.path, access);
        }
        let plan = builder.plan()?;
        let ipc_dir = ipc::ensure_ipc_dir(workspace)?;
        let socket_path = match &sandbox.spec.ipc_socket_name {
            Some(name) => ipc_dir.join(name),
            None => ipc::socket_path(workspace),
        };

        let mut status = sandbox.status.take().unwrap_or_default();
        status.ready = true;
        status.abi_version = Some("v5".to_string());
        status.ipc_socket_path = Some(socket_path.display().to_string());
        status.planned_rule_count = plan.rules.len();
        status.observed_generation = sandbox.metadata.generation;
        set_condition(
            &mut status.conditions,
            ConditionStatus {
                type_: "Ready".to_string(),
                status: "True".to_string(),
                reason: "SandboxPlanned".to_string(),
                message: format!(
                    "planned {} Landlock rules (isolate_network={})",
                    plan.rules.len(),
                    sandbox.spec.isolate_network
                ),
                last_transition_time: now_epoch_secs,
            },
        );
        sandbox.status = Some(status);

        Ok((
            ReconcileDecision::Reconciled {
                requeue_after: STEADY_REQUEUE,
            },
            plan,
        ))
    }
}

/// Inspect planned rules on a compiled [`RulesetPlan`].
pub fn planned_rules(plan: &RulesetPlan) -> &[PlannedRule] {
    &plan.rules
}

fn verify_custody_receipt(slot: &WorkerSlot) -> Result<(), String> {
    if slot.spec.claim_verified_without_receipt && slot.spec.landing_receipt_json.is_none() {
        return Err(
            "cannot mark landing verified without a valid uas_domain::LandingReceipt".to_string(),
        );
    }
    if let Some(raw) = &slot.spec.landing_receipt_json {
        let receipt = serde_json::from_str::<LandingReceipt>(raw)
            .map_err(|err| format!("invalid LandingReceipt payload: {err}"))?;
        if slot.spec.claim_verified_without_receipt && receipt.evidence() != EvidenceTier::Verified
        {
            return Err(format!(
                "LandingReceipt evidence is {:?}, which does not satisfy Verified custody",
                receipt.evidence()
            ));
        }
    }
    Ok(())
}

fn parse_desired_state(raw: &str) -> Result<SlotState, ReconcileError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "cold" => Ok(SlotState::Cold),
        "warm" => Ok(SlotState::Warm),
        "leased" => Ok(SlotState::Leased),
        "suspended" => Ok(SlotState::Suspended),
        "draining" | "terminated" => Ok(SlotState::Draining),
        _ => Err(ReconcileError::UnsupportedDesiredState(raw.to_string())),
    }
}

fn slot_state_label(state: SlotState) -> &'static str {
    match state {
        SlotState::Cold => "Cold",
        SlotState::Warm => "Warm",
        SlotState::Leased => "Leased",
        SlotState::Suspended => "Suspended",
        SlotState::Draining => "Draining",
    }
}

fn set_condition(conditions: &mut Vec<ConditionStatus>, next: ConditionStatus) {
    if let Some(existing) = conditions.iter_mut().find(|c| c.type_ == next.type_) {
        *existing = next;
    } else {
        conditions.push(next);
    }
}

fn is_deleting(slot: &WorkerSlot) -> bool {
    slot.metadata.deletion_timestamp.is_some()
}

fn ensure_finalizer(slot: &mut WorkerSlot, finalizer: &str) {
    let finalizers = slot.metadata.finalizers.get_or_insert_with(Vec::new);
    if !finalizers.iter().any(|f| f == finalizer) {
        finalizers.push(finalizer.to_string());
    }
}

fn remove_finalizer(slot: &mut WorkerSlot, finalizer: &str) {
    if let Some(finalizers) = &mut slot.metadata.finalizers {
        finalizers.retain(|f| f != finalizer);
    }
}
