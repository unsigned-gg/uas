//! Integration tests for Critical Path Method (CPM) and Agent Sprint Wave Strategy.
//!
//! Verification for LeanSpec 0024 and estate orchestration invariants.

use cas_engine::{
    classify_step, validate_verdict, ActivityKind, AgentRole, BranchPruneCompensator,
    CompositeCompensator, CpmSchedule, EngineError, LeaseReturnCompensator, ModelTier,
    SagaFlowCoordinator, SagaStep, WaveCoordinator, WavePlan, WorkspaceMode,
    WorktreeCleanupCompensator, DEFAULT_MAX_WAVE_CONCURRENCY, MAX_VERDICT_LINES,
};

#[test]
fn test_cpm_multi_branch_dag_and_critical_path() {
    // Flow graph:
    // start (10)
    //   ├── branch_fast (5)
    //   └── branch_slow (25) ──> review_slow (10)
    // Both join at merge_gate (5).
    let steps = vec![
        SagaStep::new(
            "start",
            "Initialize workspace",
            ActivityKind::Verify,
            vec![],
        )
        .with_duration(10),
        SagaStep::new(
            "branch_fast",
            "Fast lint probe",
            ActivityKind::Verify,
            vec!["start".into()],
        )
        .with_duration(5),
        SagaStep::new(
            "branch_slow",
            "Slow builder implementation",
            ActivityKind::Impl,
            vec!["start".into()],
        )
        .with_duration(25),
        SagaStep::new(
            "review_slow",
            "Adversarial critic review",
            ActivityKind::Verify,
            vec!["branch_slow".into()],
        )
        .with_duration(10),
        SagaStep::new(
            "merge_gate",
            "Operator gate merge",
            ActivityKind::Gate,
            vec!["branch_fast".into(), "review_slow".into()],
        )
        .with_duration(5),
    ];

    let cpm = CpmSchedule::compute(&steps).expect("valid CPM DAG computation");

    // Total duration: start(10) + branch_slow(25) + review_slow(10) + merge_gate(5) = 50
    assert_eq!(cpm.total_duration, 50);
    assert_eq!(
        cpm.critical_path,
        vec!["start", "branch_slow", "review_slow", "merge_gate"]
    );

    // Verify step schedule properties
    let start = &cpm.steps["start"];
    assert_eq!(start.es, 0);
    assert_eq!(start.ef, 10);
    assert_eq!(start.ls, 0);
    assert_eq!(start.lf, 10);
    assert_eq!(start.slack, 0);
    assert!(start.is_critical);

    let branch_slow = &cpm.steps["branch_slow"];
    assert_eq!(branch_slow.es, 10);
    assert_eq!(branch_slow.ef, 35);
    assert_eq!(branch_slow.ls, 10);
    assert_eq!(branch_slow.lf, 35);
    assert_eq!(branch_slow.slack, 0);
    assert!(branch_slow.is_critical);

    let review_slow = &cpm.steps["review_slow"];
    assert_eq!(review_slow.es, 35);
    assert_eq!(review_slow.ef, 45);
    assert_eq!(review_slow.ls, 35);
    assert_eq!(review_slow.lf, 45);
    assert_eq!(review_slow.slack, 0);
    assert!(review_slow.is_critical);

    let branch_fast = &cpm.steps["branch_fast"];
    assert_eq!(branch_fast.es, 10);
    assert_eq!(branch_fast.ef, 15);
    // LF for branch_fast is LS(merge_gate) = 45.
    // LS for branch_fast = 45 - 5 = 40.
    assert_eq!(branch_fast.ls, 40);
    assert_eq!(branch_fast.lf, 45);
    assert_eq!(branch_fast.slack, 30);
    assert!(!branch_fast.is_critical);

    let merge = &cpm.steps["merge_gate"];
    assert_eq!(merge.es, 45);
    assert_eq!(merge.ef, 50);
    assert_eq!(merge.ls, 45);
    assert_eq!(merge.lf, 50);
    assert_eq!(merge.slack, 0);
    assert!(merge.is_critical);
}

#[test]
fn test_cpm_cycle_and_missing_dep_detection() {
    // Cyclic flow
    let cyclic = vec![
        SagaStep::new("a", "Step A", ActivityKind::Impl, vec!["c".into()]),
        SagaStep::new("b", "Step B", ActivityKind::Impl, vec!["a".into()]),
        SagaStep::new("c", "Step C", ActivityKind::Impl, vec!["b".into()]),
    ];
    let err = CpmSchedule::compute(&cyclic).unwrap_err();
    assert!(matches!(err, EngineError::InvalidFlow { .. }));

    // Missing dependency
    let missing = vec![SagaStep::new(
        "orphan",
        "Orphan step",
        ActivityKind::Verify,
        vec!["ghost".into()],
    )];
    let err2 = CpmSchedule::compute(&missing).unwrap_err();
    assert!(matches!(err2, EngineError::InvalidFlow { .. }));
}

#[test]
fn test_wave_planning_concurrency_and_critical_path_priority() {
    // Construct 5 parallel steps ready at once.
    // Concurrency limit is 3.
    // Steps 1 & 2 lead into a 100s critical path; steps 3, 4, 5 lead into 0s remainder.
    let steps = vec![
        SagaStep::new("crit1", "Critical 1", ActivityKind::Impl, vec![]).with_duration(20),
        SagaStep::new("crit2", "Critical 2", ActivityKind::Impl, vec![]).with_duration(30),
        SagaStep::new("noncrit1", "Slack 1", ActivityKind::Verify, vec![]).with_duration(5),
        SagaStep::new("noncrit2", "Slack 2", ActivityKind::Verify, vec![]).with_duration(5),
        SagaStep::new("noncrit3", "Slack 3", ActivityKind::Verify, vec![]).with_duration(5),
        SagaStep::new(
            "crit_sink",
            "Critical sink",
            ActivityKind::Gate,
            vec!["crit1".into(), "crit2".into()],
        )
        .with_duration(10),
    ];

    let plan = WavePlan::plan(&steps, DEFAULT_MAX_WAVE_CONCURRENCY).expect("valid wave plan");
    assert_eq!(plan.max_concurrency, 3);

    // Wave 0 must be allocated at most 3 items, with critical path steps taking priority.
    let wave0 = &plan.waves[0];
    assert_eq!(wave0.wave_index, 0);
    assert_eq!(wave0.steps.len(), 3);

    let wave0_ids: Vec<&str> = wave0.steps.iter().map(|s| s.step.id.as_str()).collect();
    // Both crit1 and crit2 must be scheduled in wave 0 because they have zero float slack
    assert!(wave0_ids.contains(&"crit1"));
    assert!(wave0_ids.contains(&"crit2"));

    // Total steps across all waves matches total steps in definition
    assert_eq!(plan.step_count(), 6);
}

#[test]
fn test_estate_role_and_model_classification() {
    let probe_step = SagaStep::new(
        "lint-probe",
        "Run clippy and fmt checks",
        ActivityKind::Verify,
        vec![],
    );
    let (role, model, ws) = classify_step(&probe_step);
    assert_eq!(role, AgentRole::Probe);
    assert_eq!(model, ModelTier::Fast);
    assert_eq!(ws, WorkspaceMode::ReadOnly);

    let builder_step = SagaStep::new(
        "builder-task",
        "Implement eBPF probe logic",
        ActivityKind::Impl,
        vec![],
    );
    let (role, model, ws) = classify_step(&builder_step);
    assert_eq!(role, AgentRole::Builder);
    assert_eq!(model, ModelTier::Pro);
    assert_eq!(ws, WorkspaceMode::VirtualShare);

    let critic_step = SagaStep::new(
        "critic-review",
        "Adversarial review of probe diff",
        ActivityKind::Verify,
        vec![],
    );
    let (role, model, ws) = classify_step(&critic_step);
    assert_eq!(role, AgentRole::Critic);
    assert_eq!(model, ModelTier::Pro);
    assert_eq!(ws, WorkspaceMode::ReadOnly);

    let gate_step = SagaStep::new(
        "operator-gate",
        "Sign-off and landing merge",
        ActivityKind::Gate,
        vec![],
    );
    let (role, model, ws) = classify_step(&gate_step);
    assert_eq!(role, AgentRole::MainSession);
    assert_eq!(model, ModelTier::Main);
    assert_eq!(ws, WorkspaceMode::MainHost);
}

#[test]
fn test_verdict_line_count_validation() {
    let compact_verdict = "Verdict: GREEN\nTests: 42 passed, 0 failed\nCoverage: 98%\n";
    assert!(validate_verdict(compact_verdict).is_ok());

    let exact_limit: String = (0..MAX_VERDICT_LINES)
        .map(|i| format!("line {i}\n"))
        .collect();
    assert!(validate_verdict(&exact_limit).is_ok());

    let oversized_dump: String = (0..MAX_VERDICT_LINES + 5)
        .map(|i| format!("overflow line {i}\n"))
        .collect();
    let err = validate_verdict(&oversized_dump).unwrap_err();
    assert!(matches!(err, EngineError::InvalidFlow { .. }));
}

#[test]
fn test_wave_coordinator_execution_and_compensator_rollback() {
    let wt_comp = WorktreeCleanupCompensator::new();
    wt_comp.register_worktree("/tmp/worktrees/slot-builder-wave");
    let br_comp = BranchPruneCompensator::new();
    br_comp.register_branch("blackwall/wave-branch");
    let lr_comp = LeaseReturnCompensator::new();
    lr_comp.register_lease("lease-wave-slot");

    let mut composite = CompositeCompensator::new();
    composite.add(Box::new(wt_comp.clone()));
    composite.add(Box::new(br_comp.clone()));
    composite.add(Box::new(lr_comp.clone()));

    let steps = vec![
        SagaStep::new("step1", "Probe check", ActivityKind::Verify, vec![]),
        SagaStep::new(
            "step2",
            "Builder implementation",
            ActivityKind::Impl,
            vec!["step1".into()],
        ),
        SagaStep::new(
            "step3",
            "Critic review",
            ActivityKind::Verify,
            vec!["step2".into()],
        ),
    ];

    let plan = WavePlan::plan(&steps, 3).expect("valid plan");
    let mut coordinator = WaveCoordinator::new(plan, Box::new(composite));

    assert_eq!(coordinator.current_wave_idx(), 0);
    let wave0 = coordinator.current_wave().unwrap();
    assert_eq!(wave0.steps.len(), 1);
    assert_eq!(wave0.steps[0].step.id, "step1");

    // Advance fails before marking step1 completed
    let err = coordinator.advance_wave().unwrap_err();
    assert!(matches!(err, EngineError::InvalidFlow { .. }));

    // Mark step1 complete with a valid verdict
    coordinator
        .mark_step_completed("step1", Some("Verified probe clean".into()))
        .expect("verdict valid");
    assert!(coordinator.is_current_wave_completed());

    // Advance to wave 1
    let next_step_id = coordinator.advance_wave().unwrap().unwrap().steps[0]
        .step
        .id
        .clone();
    assert_eq!(next_step_id, "step2");
    assert_eq!(coordinator.current_wave_idx(), 1);

    // Mark step2 complete
    coordinator
        .mark_step_completed("step2", Some("Builder changes applied".into()))
        .expect("verdict valid");

    // Simulate failure at step3 in wave 2 -> rollback in reverse order
    let receipts = coordinator
        .rollback("step3", "Critic found invariant violation")
        .expect("rollback succeeded");

    // Compensation executed for completed steps: step2 then step1
    assert!(!receipts.is_empty());
    assert!(wt_comp.tracked_worktrees().is_empty());
    assert!(br_comp.tracked_branches().is_empty());
    assert!(lr_comp.tracked_leases().is_empty());
}

#[test]
fn test_saga_flow_coordinator_cpm_and_wave_plan_api() {
    let steps = vec![
        SagaStep::new("a", "Step A", ActivityKind::Verify, vec![]).with_duration(10),
        SagaStep::new("b", "Step B", ActivityKind::Impl, vec!["a".into()]).with_duration(20),
    ];

    let coord = SagaFlowCoordinator::new(steps, Box::new(CompositeCompensator::new()));
    let cpm = coord.cpm_schedule().expect("cpm schedule succeeds");
    assert_eq!(cpm.total_duration, 30);
    assert_eq!(cpm.critical_path, vec!["a", "b"]);

    let wave_plan = coord.plan_waves(2).expect("wave plan succeeds");
    assert_eq!(wave_plan.total_waves(), 2);
    assert_eq!(wave_plan.step_count(), 2);
}
