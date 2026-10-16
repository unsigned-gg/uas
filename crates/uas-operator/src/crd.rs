// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Custom Resource Definitions (`WorkerSlot` and `AgentSandbox`) in group `uas.unsigned.gg/v1alpha1`.
//!
//! Spec 0038 D2, D3 and D8: declarative Kubernetes API surface for worker slot
//! lifecycles, Spec 0036 host slot locks, Spec 0041 resource overrides, and
//! Spec 0010/0013 Landlock + network namespace sandboxes.

use serde::{Deserialize, Serialize};

/// API group for all UAS custom resources (spec 0038 D2).
pub const API_GROUP: &str = "uas.unsigned.gg";

/// API version for Wave 1 UAS custom resources (spec 0038 D2).
pub const API_VERSION: &str = "v1alpha1";

/// Finalizer registered on `WorkerSlot` resources to guarantee host slot lock
/// release and cgroup cleanup before Kubernetes deletes the object (spec 0038 D4).
pub const SLOT_CLEANUP_FINALIZER: &str = "uas.unsigned.gg/slot-cleanup";

/// Standard Kubernetes-style status condition entry attached to UAS custom resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "k8s", derive(schemars::JsonSchema))]
pub struct ConditionStatus {
    /// Condition type (for example `"Ready"` or `"CustodyReceiptInvalid"`).
    #[serde(rename = "type")]
    pub type_: String,
    /// Condition status (`"True"`, `"False"`, or `"Unknown"`).
    pub status: String,
    /// Machine-readable CamelCase reason code (for example `"CapacityExhausted"`).
    pub reason: String,
    /// Human-readable description of the transition or blocker.
    pub message: String,
    /// Epoch seconds when the condition last transitioned.
    pub last_transition_time: u64,
}

/// Per-slot cgroup v2 resource limit overrides (spec 0038 D2, spec 0041 D7).
///
/// When present on a [`WorkerSlotSpec`], these fields populate
/// [`uas_pool::LimitOverrides`] on [`uas_pool::PoolConfig`] and take precedence
/// over `UAS_SLOT_MEMORY_MAX` and `UAS_SLOT_PIDS_MAX` environment variables.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "k8s", derive(schemars::JsonSchema))]
pub struct SlotLimitsSpec {
    /// Hard memory ceiling (`memory.max`) in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_max_bytes: Option<u64>,
    /// Throttle threshold (`memory.high`) in bytes; defaults to 90% of `memory_max_bytes` when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_high_bytes: Option<u64>,
    /// Maximum number of processes (`pids.max`) inside the slot cgroup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pids_max: Option<u32>,
}

/// Desired state of a [`WorkerSlot`] custom resource (`uas.unsigned.gg/v1alpha1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "k8s",
    derive(kube::CustomResource, schemars::JsonSchema),
    kube(
        group = "uas.unsigned.gg",
        version = "v1alpha1",
        kind = "WorkerSlot",
        namespaced,
        status = "WorkerSlotStatus",
        shortname = "wslot",
        printcolumn = r#"{"name":"State","type":"string","jsonPath":".status.phase"}"#,
        printcolumn = r#"{"name":"Worktree","type":"string","jsonPath":".spec.worktree_ref"}"#,
        printcolumn = r#"{"name":"Lease","type":"string","jsonPath":".spec.lease_id"}"#,
        printcolumn = r#"{"name":"Node","type":"string","jsonPath":".spec.node_name"}"#,
        printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
    )
)]
pub struct WorkerSlotSpec {
    /// Domain slot identifier (`uas_domain::SlotId`).
    pub slot_id: String,
    /// Worktree path or reference required to warm or lease the slot (`uas_domain::WorktreeRef`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_ref: Option<String>,
    /// Desired lifecycle phase (`"Cold"`, `"Warm"`, `"Leased"`, `"Suspended"`, `"Failed"`, `"Terminated"`).
    pub desired_state: String,
    /// Optional lease identifier (`uas_domain::LeaseId`) when `desired_state == "Leased"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_id: Option<String>,
    /// Optional principal holding the lease (`uas_domain::PrincipalRef`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<String>,
    /// Lease time-to-live in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_ttl_seconds: Option<u64>,
    /// Optional reference to an `AgentSandbox` resource in the same namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_ref: Option<String>,
    /// Target Kubernetes node name assigned by the controller for DaemonSet node-agent actuation (spec 0038 D8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    /// Per-slot cgroup v2 resource limit overrides (spec 0038 REQ-0038-13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<SlotLimitsSpec>,
    /// Optional serialized `uas_domain::LandingReceipt` JSON payload for custody verification (spec 0038 D5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landing_receipt_json: Option<String>,
    /// When true without a valid `landing_receipt_json`, simulates an invalid custody bypass attempt
    /// that the reconciler must refuse with `CustodyReceiptInvalid` (spec 0038 REQ-0038-10).
    #[serde(default)]
    pub claim_verified_without_receipt: bool,
}

/// Observed state of a [`WorkerSlot`] custom resource (`uas.unsigned.gg/v1alpha1`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "k8s", derive(schemars::JsonSchema))]
pub struct WorkerSlotStatus {
    /// Observed domain lifecycle phase (`Cold`, `Warm`, `Leased`, `Suspended`, `Failed`, `Terminated`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// Epoch seconds when the active lease expires, if leased.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<u64>,
    /// Last `metadata.generation` reconciled by the operator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
    /// Acquired host lock index (`0..H`) under `UAS_SLOT_DIR` when warm or leased (spec 0036 D2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_slot_index: Option<usize>,
    /// Effective `memory.max` in bytes resolved by `uas-pool` after applying `spec.limits` and env.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_memory_max_bytes: Option<u64>,
    /// Effective `pids.max` resolved by `uas-pool` after applying `spec.limits` and env.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_pids_max: Option<u32>,
    /// True only when a `uas_domain::LandingReceipt` passed custody verification at `EvidenceTier::Verified`.
    #[serde(default)]
    pub verified_landing: bool,
    /// Standard Kubernetes status conditions (`Ready`, `CustodyReceiptInvalid`).
    #[serde(default)]
    pub conditions: Vec<ConditionStatus>,
}

/// Filesystem access mode for an extra [`SandboxRuleSpec`] entry (spec 0010, spec 0038 D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "k8s", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SandboxAccessMode {
    /// Read files and list directories (`PathAccess::Read`).
    Read,
    /// Read, list directories, and execute (`PathAccess::ReadExec`).
    ReadExec,
    /// Write without execute (`PathAccess::Write`).
    Write,
    /// Full read, write, and execute (`PathAccess::ReadWrite`).
    ReadWrite,
}

impl SandboxAccessMode {
    /// Convert the CRD access mode into [`uas_sandbox::PathAccess`].
    pub fn to_path_access(self) -> uas_sandbox::PathAccess {
        match self {
            Self::Read => uas_sandbox::PathAccess::Read,
            Self::ReadExec => uas_sandbox::PathAccess::ReadExec,
            Self::Write => uas_sandbox::PathAccess::Write,
            Self::ReadWrite => uas_sandbox::PathAccess::ReadWrite,
        }
    }
}

/// Additional Landlock path rule in an [`AgentSandboxSpec`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "k8s", derive(schemars::JsonSchema))]
pub struct SandboxRuleSpec {
    /// Absolute filesystem path to grant access beneath.
    pub path: String,
    /// Access level granted beneath `path`.
    pub access: SandboxAccessMode,
}

fn default_isolate_network() -> bool {
    true
}

/// Desired state of an [`AgentSandbox`] custom resource (`uas.unsigned.gg/v1alpha1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "k8s",
    derive(kube::CustomResource, schemars::JsonSchema),
    kube(
        group = "uas.unsigned.gg",
        version = "v1alpha1",
        kind = "AgentSandbox",
        namespaced,
        status = "AgentSandboxStatus",
        shortname = "asbx",
        printcolumn = r#"{"name":"Ready","type":"boolean","jsonPath":".status.ready"}"#,
        printcolumn = r#"{"name":"ABI","type":"string","jsonPath":".status.abi_version"}"#,
        printcolumn = r#"{"name":"Workspace","type":"string","jsonPath":".spec.workspace_path"}"#,
        printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
    )
)]
pub struct AgentSandboxSpec {
    /// Absolute path to the worker's workspace directory (`ReadWrite` under Landlock).
    pub workspace_path: String,
    /// Additional toolchain directories granted `ReadExec` (spec 0010 worker preset).
    #[serde(default)]
    pub toolchain_paths: Vec<String>,
    /// Extra path rules appended after the worker preset.
    #[serde(default)]
    pub extra_rules: Vec<SandboxRuleSpec>,
    /// Whether to unshare the network namespace so only loopback exists (spec 0010 D8).
    #[serde(default = "default_isolate_network")]
    pub isolate_network: bool,
    /// Optional override for the IPC socket file name under `<workspace>/.ipc/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ipc_socket_name: Option<String>,
}

/// Observed state of an [`AgentSandbox`] custom resource (`uas.unsigned.gg/v1alpha1`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "k8s", derive(schemars::JsonSchema))]
pub struct AgentSandboxStatus {
    /// True when the sandbox ruleset plan and IPC directory have been validated.
    pub ready: bool,
    /// Target Landlock ABI version string (for example `"v5"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abi_version: Option<String>,
    /// Canonical path to the worker's LLM gateway Unix domain socket (`<workspace>/.ipc/llm.sock`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ipc_socket_path: Option<String>,
    /// Number of canonicalized Landlock rules in the compiled [`uas_sandbox::RulesetPlan`].
    #[serde(default)]
    pub planned_rule_count: usize,
    /// Last `metadata.generation` reconciled by the operator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
    /// Standard Kubernetes status conditions.
    #[serde(default)]
    pub conditions: Vec<ConditionStatus>,
}

/// Object metadata type used by [`WorkerSlot`] and [`AgentSandbox`].
#[cfg(feature = "k8s")]
pub type ObjectMeta = k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

/// Lightweight fallback metadata when compiled without the `k8s` feature.
#[cfg(not(feature = "k8s"))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectMeta {
    /// Resource name within its namespace.
    pub name: Option<String>,
    /// Resource namespace.
    pub namespace: Option<String>,
    /// Monotonically increasing spec generation counter.
    pub generation: Option<i64>,
    /// RFC 3339 deletion timestamp when the object is being terminated.
    pub deletion_timestamp: Option<String>,
    /// Finalizers registered on the resource.
    pub finalizers: Option<Vec<String>>,
}

/// Fallback `WorkerSlot` resource wrapper when compiled without the `k8s` feature.
#[cfg(not(feature = "k8s"))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerSlot {
    /// Standard object metadata.
    pub metadata: ObjectMeta,
    /// Desired worker slot state.
    pub spec: WorkerSlotSpec,
    /// Observed worker slot state.
    pub status: Option<WorkerSlotStatus>,
}

#[cfg(not(feature = "k8s"))]
impl WorkerSlot {
    /// Construct a new `WorkerSlot` with the given resource name and spec.
    pub fn new(name: &str, spec: WorkerSlotSpec) -> Self {
        Self {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec,
            status: None,
        }
    }
}

/// Fallback `AgentSandbox` resource wrapper when compiled without the `k8s` feature.
#[cfg(not(feature = "k8s"))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSandbox {
    /// Standard object metadata.
    pub metadata: ObjectMeta,
    /// Desired sandbox configuration.
    pub spec: AgentSandboxSpec,
    /// Observed sandbox status.
    pub status: Option<AgentSandboxStatus>,
}

#[cfg(not(feature = "k8s"))]
impl AgentSandbox {
    /// Construct a new `AgentSandbox` with the given resource name and spec.
    pub fn new(name: &str, spec: AgentSandboxSpec) -> Self {
        Self {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec,
            status: None,
        }
    }
}
