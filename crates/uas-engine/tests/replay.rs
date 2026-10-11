// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Deterministic replay and crash recovery test suite.
//!
//! Verification gate for CER-2679.

use uas_domain::{Attestation, EvidenceTier, LeaseId, PrincipalRef, SlotId};
use uas_engine::{
    Activity, ActivityKind, ActivityResult, EngineError, SessionId, SessionState, WorkflowEvent,
    WorkflowSession,
};

fn dummy_attestation(activity_id: &str, at: u64) -> Attestation {
    Attestation {
        signer: PrincipalRef::new("uas/test").unwrap(),
        evidence: EvidenceTier::Verified,
        receipt: format!("attestation for {activity_id}"),
        at,
    }
}

#[test]
fn test_deterministic_replay_after_session_start() {
    let session_id = SessionId::new("sess-crash-001").unwrap();
    let mut session = WorkflowSession::new(session_id.clone());

    let slot_id = SlotId::new("slot-builder-1").unwrap();
    let lease_id = LeaseId::new("lease-builder-1").unwrap();
    session
        .start(slot_id.clone(), lease_id.clone(), 1000)
        .expect("session start should succeed");

    let events = session.events().to_vec();
    assert_eq!(events.len(), 1);

    // Replay into a fresh session without side effects
    let recovered = WorkflowSession::replay_from_events(session_id.clone(), &events)
        .expect("replay should succeed");

    assert_eq!(recovered.id(), &session_id);
    assert_eq!(recovered.state(), SessionState::Running);
    assert_eq!(recovered.slot_id(), Some(&slot_id));
    assert_eq!(recovered.lease_id(), Some(&lease_id));
    assert_eq!(recovered.events().len(), 1);
}

#[test]
fn test_crash_recovery_step_by_step_and_zero_respend() {
    let session_id = SessionId::new("sess-crash-stepwise").unwrap();
    let mut session = WorkflowSession::new(session_id.clone());

    let slot_id = SlotId::new("slot-builder-2").unwrap();
    let lease_id = LeaseId::new("lease-builder-2").unwrap();
    session
        .start(slot_id.clone(), lease_id.clone(), 1000)
        .expect("start");

    // Step 1: verify
    let act1 = Activity::new(
        "step-1-spec-check",
        "Validate LeanSpecs",
        ActivityKind::Verify,
    );
    session
        .start_activity(act1.clone(), 1010)
        .expect("start act1");
    let res1 = ActivityResult {
        activity_id: "step-1-spec-check".into(),
        success: true,
        attestation: dummy_attestation("step-1-spec-check", 1020),
        summary: Some("All 9 specs valid".into()),
    };
    session
        .complete_activity(res1.clone(), 1020)
        .expect("complete act1");

    // --- SIMULATE CRASH 1: Process killed after Step 1 ---
    let log_at_crash_1 = session.events().to_vec();
    let mut rec1 = WorkflowSession::replay_from_events(session_id.clone(), &log_at_crash_1)
        .expect("recovery at crash 1 should succeed");

    assert_eq!(rec1.state(), SessionState::Running);
    assert!(rec1.is_activity_completed("step-1-spec-check"));
    assert!(!rec1.is_activity_completed("step-2-build"));

    // Verify zero re-spend: cached result is immediately available
    let cached = rec1
        .cached_result("step-1-spec-check")
        .expect("cached result must exist");
    assert_eq!(cached.summary.as_deref(), Some("All 9 specs valid"));

    // Resume execution from rec1: Step 1 is skipped, Step 2 is executed
    let mut step1_executed_count = 0;
    if !rec1.is_activity_completed("step-1-spec-check") {
        step1_executed_count += 1;
    }
    assert_eq!(step1_executed_count, 0, "Step 1 must not re-execute");

    // Execute Step 2
    let act2 = Activity::new("step-2-build", "Compile uas-engine", ActivityKind::Impl);
    rec1.start_activity(act2.clone(), 1030).expect("start act2");
    let res2 = ActivityResult {
        activity_id: "step-2-build".into(),
        success: true,
        attestation: dummy_attestation("step-2-build", 1040),
        summary: Some("Build ok in 0.24s".into()),
    };
    rec1.complete_activity(res2.clone(), 1040)
        .expect("complete act2");

    // --- SIMULATE CRASH 2: Process killed after Step 2 ---
    let log_at_crash_2 = rec1.events().to_vec();
    let mut rec2 = WorkflowSession::replay_from_events(session_id.clone(), &log_at_crash_2)
        .expect("recovery at crash 2 should succeed");

    assert_eq!(rec2.state(), SessionState::Running);
    assert!(rec2.is_activity_completed("step-1-spec-check"));
    assert!(rec2.is_activity_completed("step-2-build"));
    assert!(!rec2.is_activity_completed("step-3-test"));

    // Verify both previous steps are cached
    assert_eq!(
        rec2.cached_result("step-1-spec-check")
            .unwrap()
            .summary
            .as_deref(),
        Some("All 9 specs valid")
    );
    assert_eq!(
        rec2.cached_result("step-2-build")
            .unwrap()
            .summary
            .as_deref(),
        Some("Build ok in 0.24s")
    );

    // Resume execution from rec2: execute Step 3 and finish
    let act3 = Activity::new("step-3-test", "Run test suite", ActivityKind::Verify);
    rec2.start_activity(act3.clone(), 1050).expect("start act3");
    let res3 = ActivityResult {
        activity_id: "step-3-test".into(),
        success: true,
        attestation: dummy_attestation("step-3-test", 1060),
        summary: Some("3 passed; 0 failed".into()),
    };
    rec2.complete_activity(res3.clone(), 1060)
        .expect("complete act3");
    rec2.finish_success(1070).expect("finish session");

    assert_eq!(rec2.state(), SessionState::Succeeded);

    // --- REPLAY FINAL COMPLETED LOG ---
    let final_log = rec2.events().to_vec();
    let rec_final = WorkflowSession::replay_from_events(session_id, &final_log)
        .expect("replay of finished session should succeed");
    assert_eq!(rec_final.state(), SessionState::Succeeded);
    assert!(rec_final.is_activity_completed("step-1-spec-check"));
    assert!(rec_final.is_activity_completed("step-2-build"));
    assert!(rec_final.is_activity_completed("step-3-test"));
}

#[test]
fn test_crash_recovery_after_activity_failure_and_compensating() {
    let session_id = SessionId::new("sess-crash-fail").unwrap();
    let mut session = WorkflowSession::new(session_id.clone());

    session
        .start(
            SlotId::new("slot-1").unwrap(),
            LeaseId::new("lease-1").unwrap(),
            2000,
        )
        .expect("start");

    let act1 = Activity::new("prep", "Setup workspace", ActivityKind::Verify);
    session.start_activity(act1, 2010).expect("act1");
    session
        .complete_activity(
            ActivityResult {
                activity_id: "prep".into(),
                success: true,
                attestation: dummy_attestation("prep", 2020),
                summary: Some("Ready".into()),
            },
            2020,
        )
        .expect("prep ok");

    let act2 = Activity::new("deploy", "Deploy artifact", ActivityKind::Impl);
    session.start_activity(act2.clone(), 2030).expect("act2");
    // Fail act2 -> triggers compensation and transitions to Failed
    session
        .fail_activity(&act2, "Network timeout contacting host".into(), 2040)
        .expect("fail_activity");

    assert_eq!(session.state(), SessionState::Failed);

    // Replay log to verify session recovers into Failed state with recorded events
    let events = session.events().to_vec();
    let recovered = WorkflowSession::replay_from_events(session_id, &events)
        .expect("replay compensating session");

    assert_eq!(recovered.state(), SessionState::Failed);
    assert!(recovered.is_activity_completed("prep"));
    assert!(!recovered.is_activity_completed("deploy"));
}

#[test]
fn test_replay_rejects_corrupted_sequence_numbers() {
    let session_id = SessionId::new("sess-corrupt").unwrap();
    let mut session = WorkflowSession::new(session_id.clone());
    session
        .start(
            SlotId::new("slot-x").unwrap(),
            LeaseId::new("lease-x").unwrap(),
            3000,
        )
        .expect("start");

    let mut events = session.events().to_vec();
    // Tamper with sequence number: set 99 instead of 1
    events.push(WorkflowEvent::ActivityStarted {
        sequence: 99,
        activity: Activity::new("tampered", "Bad seq", ActivityKind::Verify),
        timestamp: 3010,
    });

    let result = WorkflowSession::replay_from_events(session_id, &events);
    match result {
        Err(EngineError::ReplayFailed { sequence, .. }) => {
            assert_eq!(sequence, 99);
        }
        Err(e) => panic!("expected ReplayFailed error, got other error: {e}"),
        Ok(_) => panic!("expected ReplayFailed error, got Ok"),
    }
}

#[test]
fn test_replay_idempotence() {
    let session_id = SessionId::new("sess-idempotence").unwrap();
    let mut session = WorkflowSession::new(session_id.clone());
    session
        .start(
            SlotId::new("slot-idem").unwrap(),
            LeaseId::new("lease-idem").unwrap(),
            4000,
        )
        .expect("start");
    let act = Activity::new("check", "Check status", ActivityKind::Verify);
    session.start_activity(act, 4010).expect("start");
    session
        .complete_activity(
            ActivityResult {
                activity_id: "check".into(),
                success: true,
                attestation: dummy_attestation("check", 4020),
                summary: Some("OK".into()),
            },
            4020,
        )
        .expect("comp");
    session.finish_success(4030).expect("finish");

    let events = session.events().to_vec();

    // Replay twice independently
    let rep1 = WorkflowSession::replay_from_events(session_id.clone(), &events).expect("rep1");
    let rep2 = WorkflowSession::replay_from_events(session_id, &events).expect("rep2");

    assert_eq!(rep1.state(), rep2.state());
    assert_eq!(rep1.slot_id(), rep2.slot_id());
    assert_eq!(rep1.lease_id(), rep2.lease_id());
    assert_eq!(rep1.events().len(), rep2.events().len());
    assert_eq!(
        rep1.cached_result("check").unwrap().summary,
        rep2.cached_result("check").unwrap().summary
    );
}
