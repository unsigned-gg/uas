//! Integration test suite for Saga compensation runner and cleanup handlers.
//!
//! Verification gate for CER-2677.

use cas_domain::{LeaseId, SlotId};
use cas_engine::{
    ActivityKind, BranchPruneCompensator, CompositeCompensator, LeaseReturnCompensator,
    SagaFlowCoordinator, SagaFlowDef, SagaStep, SessionId, SessionState, WorkflowSession,
    WorktreeCleanupCompensator,
};

#[test]
fn saga_compensation_runner_reverse_execution_and_receipts() {
    let wt_comp = WorktreeCleanupCompensator::new();
    wt_comp.register_worktree("/tmp/worktrees/slot-builder-1");

    let br_comp = BranchPruneCompensator::new();
    br_comp.register_branch("blackwall/run-7f8a9b");

    let lr_comp = LeaseReturnCompensator::new();
    lr_comp.register_lease("lease-slot-builder-1");

    let mut composite = CompositeCompensator::new();
    composite.add(Box::new(wt_comp.clone()));
    composite.add(Box::new(br_comp.clone()));
    composite.add(Box::new(lr_comp.clone()));

    let steps = vec![
        SagaStep::new(
            "custody-check",
            "Verify Blackwall sandbox",
            ActivityKind::Verify,
            vec![],
        ),
        SagaStep::new(
            "spec-check",
            "Verify LeanSpecs",
            ActivityKind::Verify,
            vec!["custody-check".into()],
        ),
        SagaStep::new(
            "builder-impl",
            "Implement feature",
            ActivityKind::Impl,
            vec!["spec-check".into()],
        ),
        SagaStep::new(
            "gate-verify",
            "Run verification gates",
            ActivityKind::Verify,
            vec!["builder-impl".into()],
        ),
    ];

    let mut coord = SagaFlowCoordinator::new(steps, Box::new(composite));

    // Execute step 1: custody-check
    let s1 = coord.next_runnable_step().unwrap();
    assert_eq!(s1.id, "custody-check");
    coord.mark_completed("custody-check");

    // Execute step 2: spec-check
    let s2 = coord.next_runnable_step().unwrap();
    assert_eq!(s2.id, "spec-check");
    coord.mark_completed("spec-check");

    // Execute step 3: builder-impl
    let s3 = coord.next_runnable_step().unwrap();
    assert_eq!(s3.id, "builder-impl");
    coord.mark_completed("builder-impl");

    // Step 4: gate-verify fails! Trigger rollback
    let receipts = coord
        .rollback("gate-verify", "make check failed with exit code 1")
        .unwrap();

    // Verify reverse execution order: builder-impl, then spec-check, then custody-check
    // Each step invoked the composite compensator with 3 handlers = 9 receipts total
    assert_eq!(receipts.len(), 9);

    // First compensated step must be builder-impl
    assert_eq!(receipts[0].activity_id, "builder-impl");
    assert_eq!(receipts[0].handler_name, "worktree_cleanup");
    assert!(receipts[0].success);
    assert!(receipts[0]
        .details
        .contains("/tmp/worktrees/slot-builder-1"));

    assert_eq!(receipts[1].activity_id, "builder-impl");
    assert_eq!(receipts[1].handler_name, "branch_prune");
    assert!(receipts[1].details.contains("blackwall/run-7f8a9b"));

    assert_eq!(receipts[2].activity_id, "builder-impl");
    assert_eq!(receipts[2].handler_name, "lease_return");
    assert!(receipts[2].details.contains("lease-slot-builder-1"));

    // Next compensated step in reverse order: spec-check
    assert_eq!(receipts[3].activity_id, "spec-check");
    assert_eq!(receipts[4].activity_id, "spec-check");
    assert_eq!(receipts[5].activity_id, "spec-check");

    // Last compensated step in reverse order: custody-check
    assert_eq!(receipts[6].activity_id, "custody-check");
    assert_eq!(receipts[7].activity_id, "custody-check");
    assert_eq!(receipts[8].activity_id, "custody-check");

    // Verify tracked state has been cleaned up
    assert!(wt_comp.tracked_worktrees().is_empty());
    assert!(br_comp.tracked_branches().is_empty());
    assert!(lr_comp.tracked_leases().is_empty());
}

#[test]
fn saga_flow_parsing_from_disk_flows() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.join("../..");
    let gates_path = if repo_root.join(".saga/flows/gates.toml").exists() {
        repo_root.join(".saga/flows/gates.toml")
    } else {
        std::path::PathBuf::from(".saga/flows/gates.toml")
    };
    let gates_content = std::fs::read_to_string(&gates_path)
        .unwrap_or_else(|e| panic!("should read {}: {e}", gates_path.display()));
    let flow_def = SagaFlowDef::from_toml(&gates_content).expect("gates.toml should parse cleanly");
    assert_eq!(flow_def.flow.name, "gates");
    assert_eq!(flow_def.steps.len(), 4);

    let steps = flow_def.to_steps();
    assert_eq!(steps[0].id, "fmt-check");
    assert_eq!(steps[1].id, "clippy-check");
    assert_eq!(steps[2].id, "test-suite");
    assert_eq!(steps[3].id, "operator-gate");
    assert_eq!(steps[3].kind, ActivityKind::Gate);
    assert_eq!(steps[3].gate.as_deref(), Some("heimdal:approval"));

    let builder_path = if repo_root.join(".saga/flows/cas-builder.toml").exists() {
        repo_root.join(".saga/flows/cas-builder.toml")
    } else {
        std::path::PathBuf::from(".saga/flows/cas-builder.toml")
    };
    let builder_content = std::fs::read_to_string(&builder_path)
        .unwrap_or_else(|e| panic!("should read {}: {e}", builder_path.display()));
    let builder_def =
        SagaFlowDef::from_toml(&builder_content).expect("cas-builder.toml should parse cleanly");
    assert_eq!(builder_def.flow.name, "cas-builder");
    assert_eq!(builder_def.steps.len(), 7);
}

#[test]
fn session_rollback_emits_receipts_on_activity_failure() {
    let session_id = SessionId::new("sess-fail-receipts").unwrap();
    let slot_id = SlotId::new("slot-1").unwrap();
    let lease_id = LeaseId::new("lease-1").unwrap();

    let wt_comp = WorktreeCleanupCompensator::new();
    wt_comp.register_worktree("worktrees/slot-1");

    let mut session = WorkflowSession::with_store_and_compensator(
        session_id,
        cas_engine::InMemoryEventStore::new(),
        Box::new(wt_comp),
    );

    session.start(slot_id, lease_id, 100).unwrap();
    let act = cas_engine::Activity::new("act-failing", "Execute bad task", ActivityKind::Impl);
    session.start_activity(act.clone(), 105).unwrap();

    session
        .fail_activity(&act, "non-zero exit code 42".into(), 110)
        .unwrap();

    assert_eq!(session.state(), SessionState::Failed);
    let receipts = session.compensation_receipts();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].activity_id, "act-failing");
    assert_eq!(receipts[0].handler_name, "worktree_cleanup");
    assert!(receipts[0].success);
    assert!(receipts[0].details.contains("worktrees/slot-1"));
}
