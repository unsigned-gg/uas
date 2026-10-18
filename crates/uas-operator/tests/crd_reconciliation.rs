// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Integration tests for `uas-operator` CRD schemas, reconciliation loops,
//! Spec 0036 host slot locks, custody receipt enforcement, and Spec 0041 limit overrides.

use std::fs::OpenOptions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use uas_domain::{AuditStatus, CostSource, EvidenceTier, LandingFacts, LandingReceipt, SlotId};
use uas_operator::{
    planned_rules, resolve_slot_limits_with_probe, AgentSandbox, AgentSandboxSpec,
    HostSlotLockTable, OperatorMode, OperatorReconciler, ReconcileDecision, SandboxAccessMode,
    SandboxRuleSpec, SlotLimitsSpec, WorkerSlot, WorkerSlotSpec, CAPACITY_EXHAUSTED_REQUEUE,
    SLOT_CLEANUP_FINALIZER,
};
use uas_pool::limits::LimitSource;
use uas_pool::HostProbe;
use uas_sandbox::PathAccess;

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "uas-operator-test-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .expect("chmod 0700 scratch dir");
    dir
}

fn sample_worker_slot(name: &str, desired_state: &str) -> WorkerSlot {
    let mut slot = WorkerSlot::new(
        name,
        WorkerSlotSpec {
            slot_id: name.to_string(),
            worktree_ref: Some(format!("worktrees/{name}")),
            desired_state: desired_state.to_string(),
            lease_id: Some(format!("lease-{name}")),
            principal: Some("lane-builder".to_string()),
            lease_ttl_seconds: Some(600),
            sandbox_ref: Some(format!("sandbox-{name}")),
            node_name: None,
            limits: None,
            landing_receipt_json: None,
            claim_verified_without_receipt: false,
        },
    );
    slot.metadata.generation = Some(1);
    slot
}

#[test]
fn capacity_exhausted_sets_ready_false_and_requeues_without_failing() {
    // REQ-0038-07: When all H host slot locks are held, reconcile sets Ready=False
    // (reason: CapacityExhausted) and requeues after 5s without failing the resource.
    let slot_dir = scratch_dir("capacity-exhausted");

    // Pre-lock slot-0.lock and slot-1.lock externally via flock.
    let lock0 = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(slot_dir.join("slot-0.lock"))
        .expect("open slot-0.lock");
    lock0.try_lock().expect("flock slot-0.lock");

    let lock1 = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(slot_dir.join("slot-1.lock"))
        .expect("open slot-1.lock");
    lock1.try_lock().expect("flock slot-1.lock");

    let table = Arc::new(HostSlotLockTable::new(&slot_dir, 2).expect("lock table"));
    let reconciler = OperatorReconciler::new(OperatorMode::NodeAgent, "ceres", table);

    let mut slot = sample_worker_slot("slot-overflow", "Warm");
    let decision = reconciler
        .reconcile_worker_slot(&mut slot, 1_700_000_000)
        .expect("reconcile should not return a fatal error");

    assert_eq!(
        decision,
        ReconcileDecision::RequeueCapacityExhausted {
            requeue_after: CAPACITY_EXHAUSTED_REQUEUE,
        }
    );
    assert_eq!(CAPACITY_EXHAUSTED_REQUEUE, Duration::from_secs(5));

    let status = slot.status.as_ref().expect("status populated");
    assert_eq!(status.phase.as_deref(), Some("Cold"));
    assert_eq!(status.host_slot_index, None);

    let ready = status
        .conditions
        .iter()
        .find(|c| c.type_ == "Ready")
        .expect("Ready condition present");
    assert_eq!(ready.status, "False");
    assert_eq!(ready.reason, "CapacityExhausted");

    drop(lock0);
    drop(lock1);
    let _ = std::fs::remove_dir_all(&slot_dir);
}

#[test]
fn deleting_worker_slot_runs_finalizer_and_releases_flock_without_unlinking() {
    // REQ-0038-08 & Spec 0036 D2: Finalizer uas.unsigned.gg/slot-cleanup releases the open
    // flock descriptor on delete so another process can acquire it, and never unlinks slot-0.lock.
    let slot_dir = scratch_dir("finalizer-cleanup");
    let table = Arc::new(HostSlotLockTable::new(&slot_dir, 2).expect("lock table"));
    let reconciler = OperatorReconciler::new(OperatorMode::NodeAgent, "ceres", table);

    let mut slot = sample_worker_slot("slot-1", "Leased");
    let decision = reconciler
        .reconcile_worker_slot(&mut slot, 1_700_000_100)
        .expect("reconcile leased");
    assert!(matches!(decision, ReconcileDecision::Reconciled { .. }));

    // Finalizer must be registered.
    let finalizers = slot.metadata.finalizers.as_ref().expect("finalizers set");
    assert!(finalizers.iter().any(|f| f == SLOT_CLEANUP_FINALIZER));

    // Status reflects Leased phase and held lock index 0.
    let status = slot.status.as_ref().expect("status");
    assert_eq!(status.phase.as_deref(), Some("Leased"));
    assert_eq!(status.host_slot_index, Some(0));
    assert_eq!(status.lease_expires_at, Some(1_700_000_700));

    // Idempotent second pass on same generation returns NoOp (REQ-0038-06).
    let second = reconciler
        .reconcile_worker_slot(&mut slot, 1_700_000_105)
        .expect("idempotent reconcile");
    assert_eq!(second, ReconcileDecision::NoOp);

    // While leased, an external opener cannot lock slot-0.lock.
    let lock0_path = slot_dir.join("slot-0.lock");
    let probe_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock0_path)
        .expect("open lock0");
    assert!(
        matches!(
            probe_file.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ),
        "slot-0.lock must be held while WorkerSlot is Leased"
    );

    // Mark the WorkerSlot for deletion and re-run reconciliation.
    mark_for_deletion(&mut slot);
    let del_decision = reconciler
        .reconcile_worker_slot(&mut slot, 1_700_000_200)
        .expect("reconcile deletion");
    assert_eq!(del_decision, ReconcileDecision::Finalized);

    // Finalizer is removed and lock file still exists on disk (never unlinked).
    assert!(
        slot.metadata
            .finalizers
            .as_ref()
            .is_none_or(|f| !f.iter().any(|item| item == SLOT_CLEANUP_FINALIZER)),
        "finalizer must be removed after cleanup"
    );
    assert!(lock0_path.is_file(), "slot-0.lock must never be unlinked");

    // External opener can now acquire slot-0.lock immediately.
    probe_file
        .try_lock()
        .expect("slot-0.lock must be free after finalizer runs");

    drop(probe_file);
    let _ = std::fs::remove_dir_all(&slot_dir);
}

#[test]
fn agent_sandbox_reconciles_to_landlock_ruleset_plan_and_ipc_socket() {
    // REQ-0038-09: Reconciling an AgentSandbox translates AgentSandboxSpec into a valid
    // uas_sandbox::LandlockRulesetBuilder plan with expected path access flags and IPC socket path.
    let base = scratch_dir("agent-sandbox");
    let workspace = base.join("workspace");
    let extra_ro = base.join("shared-data");
    std::fs::create_dir_all(&workspace).expect("create workspace");
    std::fs::create_dir_all(&extra_ro).expect("create shared-data");

    let slot_dir = base.join("slots");
    let table = Arc::new(HostSlotLockTable::new(&slot_dir, 2).expect("lock table"));
    let reconciler = OperatorReconciler::new(OperatorMode::NodeAgent, "ceres", table);

    let mut sandbox = AgentSandbox::new(
        "sandbox-slot-1",
        AgentSandboxSpec {
            workspace_path: workspace.display().to_string(),
            toolchain_paths: vec![],
            extra_rules: vec![SandboxRuleSpec {
                path: extra_ro.display().to_string(),
                access: SandboxAccessMode::Read,
            }],
            isolate_network: true,
            ipc_socket_name: None,
        },
    );
    sandbox.metadata.generation = Some(3);

    let (decision, plan) = reconciler
        .reconcile_agent_sandbox(&mut sandbox, 1_700_000_300)
        .expect("reconcile sandbox");
    assert!(matches!(decision, ReconcileDecision::Reconciled { .. }));

    let status = sandbox.status.as_ref().expect("sandbox status");
    assert!(status.ready);
    assert_eq!(status.abi_version.as_deref(), Some("v5"));
    assert_eq!(status.observed_generation, Some(3));
    assert_eq!(
        status.ipc_socket_path.as_deref(),
        Some(
            workspace
                .join(".ipc/llm.sock")
                .display()
                .to_string()
                .as_str()
        )
    );

    let ipc_mode = std::fs::metadata(workspace.join(".ipc"))
        .expect(".ipc dir exists")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(ipc_mode, 0o700);

    let rules = planned_rules(&plan);
    let canon_ws = std::fs::canonicalize(&workspace).expect("canonical workspace");
    let canon_extra = std::fs::canonicalize(&extra_ro).expect("canonical extra");
    assert!(
        rules
            .iter()
            .any(|r| r.path == canon_ws && r.access == PathAccess::ReadWrite),
        "workspace must be granted ReadWrite in Landlock plan"
    );
    assert!(
        rules
            .iter()
            .any(|r| r.path == canon_extra && r.access == PathAccess::Read),
        "extra rule must be granted Read in Landlock plan"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn custody_receipt_bypass_is_refused_with_custody_receipt_invalid() {
    // REQ-0038-10: Attempting to mark a landing verified while bypassing uas_domain::LandingReceipt
    // validation fails with CustodyReceiptInvalid (reason: CustodyBypassRefused).
    let slot_dir = scratch_dir("custody-bypass");
    let table = Arc::new(HostSlotLockTable::new(&slot_dir, 2).expect("lock table"));
    let reconciler = OperatorReconciler::new(OperatorMode::Controller, "ceres", table);

    // 1. Direct bypass attempt without a receipt payload.
    let mut bypass_slot = sample_worker_slot("slot-bypass", "Warm");
    bypass_slot.spec.claim_verified_without_receipt = true;
    let decision = reconciler
        .reconcile_worker_slot(&mut bypass_slot, 1_700_000_400)
        .expect("reconcile returns TerminalFailure decision");
    assert_eq!(
        decision,
        ReconcileDecision::TerminalFailure {
            reason: "CustodyBypassRefused".to_string()
        }
    );
    let status = bypass_slot.status.as_ref().expect("status");
    assert_eq!(status.phase.as_deref(), Some("Failed"));
    assert!(!status.verified_landing);
    let cond = status
        .conditions
        .iter()
        .find(|c| c.type_ == "CustodyReceiptInvalid")
        .expect("CustodyReceiptInvalid condition");
    assert_eq!(cond.status, "False");
    assert_eq!(cond.reason, "CustodyBypassRefused");

    // 2. Forged receipt claiming "verified" when verify_exit == 3 (rejected by uas_domain::LandingReceipt).
    let mut forged_slot = sample_worker_slot("slot-forged", "Warm");
    let mut valid_facts = LandingFacts {
        slot_id: SlotId::new("slot-forged").unwrap(),
        run_hash: "blake3:1111".to_string(),
        audit: AuditStatus::Attested,
        cpu_cap: "200000 100000".to_string(),
        blackwall_run: "bw-1".to_string(),
        changeset_hash: "blake3:2222".to_string(),
        base_sha: "abcdef0".to_string(),
        stale_base: false,
        development_head: None,
        verify_cmd: Some("cargo test -j 2".to_string()),
        verify_exit: Some(0),
        branch: Some("blackwall/bw-1".to_string()),
        pr_number: Some(42),
        merge_commit: None,
        evaluator_touched: false,
        cost_source: CostSource::Reported,
        at: 1_700_000_400,
    };
    let valid_receipt = LandingReceipt::new(valid_facts.clone()).expect("valid receipt");
    assert_eq!(valid_receipt.evidence(), EvidenceTier::Verified);

    // Tamper the JSON so verify_exit is 3 while keeping evidence="verified".
    let mut forged_json: serde_json::Value =
        serde_json::to_value(&valid_receipt).expect("serialize receipt");
    forged_json["verify_exit"] = serde_json::json!(3);
    forged_slot.spec.landing_receipt_json = Some(forged_json.to_string());

    let forged_decision = reconciler
        .reconcile_worker_slot(&mut forged_slot, 1_700_000_410)
        .expect("forged receipt handled");
    assert_eq!(
        forged_decision,
        ReconcileDecision::TerminalFailure {
            reason: "CustodyBypassRefused".to_string()
        }
    );

    // 3. Positive control: valid Verified LandingReceipt passes custody verification.
    valid_facts.slot_id = SlotId::new("slot-verified").unwrap();
    let good_receipt = LandingReceipt::new(valid_facts).expect("good receipt");
    let mut verified_slot = sample_worker_slot("slot-verified", "Warm");
    verified_slot.spec.landing_receipt_json =
        Some(serde_json::to_string(&good_receipt).expect("json"));

    let ok_decision = reconciler
        .reconcile_worker_slot(&mut verified_slot, 1_700_000_420)
        .expect("valid receipt reconciles");
    assert!(matches!(ok_decision, ReconcileDecision::Reconciled { .. }));
    assert!(
        verified_slot
            .status
            .as_ref()
            .expect("status")
            .verified_landing
    );

    let _ = std::fs::remove_dir_all(&slot_dir);
}

#[test]
fn uas_domain_has_zero_kubernetes_dependencies() {
    // REQ-0038-11: uas-domain must remain 100% pure with zero kube / k8s-openapi / schemars deps.
    let domain_manifest_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../uas-domain/Cargo.toml");
    let content =
        std::fs::read_to_string(&domain_manifest_path).expect("read uas-domain/Cargo.toml");
    for forbidden in ["kube", "k8s-openapi", "schemars", "tokio"] {
        assert!(
            !content.contains(forbidden),
            "uas-domain/Cargo.toml must not depend on {forbidden}"
        );
    }
}

#[test]
fn spec_limits_override_uas_pool_environment_variables() {
    // REQ-0038-13: Setting WorkerSlotSpec.limits (memory_max_bytes, pids_max) overrides
    // UAS_SLOT_MEMORY_MAX and UAS_SLOT_PIDS_MAX when configuring uas-pool.
    let slot_dir = scratch_dir("spec-limits");
    let table = Arc::new(HostSlotLockTable::new(&slot_dir, 2).expect("lock table"));
    let reconciler = OperatorReconciler::new(OperatorMode::NodeAgent, "ceres", table);

    let mut slot = sample_worker_slot("slot-limited", "Warm");
    slot.spec.limits = Some(SlotLimitsSpec {
        memory_max_bytes: Some(4 * 1024 * 1024 * 1024), // 4 GiB
        memory_high_bytes: None,
        pids_max: Some(256),
    });

    let make_probe = || HostProbe {
        mem_total: Some(64 * 1024 * 1024 * 1024),
        env: Box::new(|key| match key {
            "UAS_SLOT_MEMORY_MAX" => Some((16u64 * 1024 * 1024 * 1024).to_string()),
            "UAS_SLOT_PIDS_MAX" => Some("2048".to_string()),
            _ => None,
        }),
        free_bytes: Box::new(|_| Ok(100 * 1024 * 1024 * 1024)),
    };

    let resolved = resolve_slot_limits_with_probe(&slot, 2, make_probe()).expect("resolve limits");
    assert_eq!(resolved.memory_source, Some(LimitSource::Config));
    assert_eq!(resolved.memory_max_bytes, Some(4 * 1024 * 1024 * 1024));
    assert_eq!(resolved.pids_max, 256);

    let decision = reconciler
        .reconcile_worker_slot_with_probe(&mut slot, 1_700_000_500, make_probe())
        .expect("reconcile with probe");
    assert!(matches!(decision, ReconcileDecision::Reconciled { .. }));

    let status = slot.status.as_ref().expect("status");
    assert_eq!(
        status.resolved_memory_max_bytes,
        Some(4 * 1024 * 1024 * 1024)
    );
    assert_eq!(status.resolved_pids_max, Some(256));

    let _ = std::fs::remove_dir_all(&slot_dir);
}

#[cfg(feature = "k8s")]
#[test]
fn generated_crds_match_group_version_and_names() {
    use kube::CustomResourceExt;

    let ws_crd = WorkerSlot::crd();
    assert_eq!(ws_crd.spec.group, "uas.unsigned.gg");
    assert_eq!(ws_crd.spec.names.kind, "WorkerSlot");
    assert_eq!(ws_crd.spec.names.plural, "workerslots");
    assert_eq!(ws_crd.spec.scope, "Namespaced");
    assert_eq!(ws_crd.spec.versions[0].name, "v1alpha1");
    assert!(ws_crd.spec.versions[0].subresources.is_some());

    let sb_crd = AgentSandbox::crd();
    assert_eq!(sb_crd.spec.group, "uas.unsigned.gg");
    assert_eq!(sb_crd.spec.names.kind, "AgentSandbox");
    assert_eq!(sb_crd.spec.names.plural, "agentsandboxes");
    assert_eq!(sb_crd.spec.scope, "Namespaced");
    assert_eq!(sb_crd.spec.versions[0].name, "v1alpha1");
    assert!(sb_crd.spec.versions[0].subresources.is_some());
}

#[cfg(feature = "k8s")]
fn mark_for_deletion(slot: &mut WorkerSlot) {
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
    use k8s_openapi::jiff::Timestamp;

    slot.metadata.deletion_timestamp = Some(Time(Timestamp::UNIX_EPOCH));
}

#[cfg(not(feature = "k8s"))]
fn mark_for_deletion(slot: &mut WorkerSlot) {
    slot.metadata.deletion_timestamp = Some("2026-10-16T00:00:00Z".to_string());
}
