// SPDX-License-Identifier: Apache-2.0 OR MIT
//! A replayed activity whose lease the pool forgot compensates and readmits
//! (spec 0040 D5, REQ-0040-10).

use uas_domain::{DomainError, Lease, LeaseId, LeaseLedger, LeaseToken, SlotId};
use uas_engine::{
    Activity, ActivityDisposition, ActivityKind, RecordingCompensator, SessionId, SessionState,
    WorkflowSession,
};

/// A ledger that restarted empty: every lease is unknown.
struct ForgetfulLedger;

impl LeaseLedger for ForgetfulLedger {
    fn issue(&mut self, _lease: Lease) -> Result<(), DomainError> {
        unreachable!("not used")
    }
    fn renew(
        &mut self,
        id: &LeaseId,
        _token: &LeaseToken,
        _now: u64,
        _extend_by: u64,
    ) -> Result<Lease, DomainError> {
        Err(DomainError::LeaseNotFound(id.as_str().to_string()))
    }
    fn revoke(&mut self, id: &LeaseId) -> Result<Lease, DomainError> {
        Err(DomainError::LeaseNotFound(id.as_str().to_string()))
    }
    fn find(&self, id: &LeaseId) -> Result<Lease, DomainError> {
        Err(DomainError::LeaseNotFound(id.as_str().to_string()))
    }
}

fn replayed_session(rec: &RecordingCompensator) -> (WorkflowSession, Activity, LeaseId) {
    let sid = SessionId::new("sess-lost").unwrap();
    let lease = LeaseId::new("lease-before-restart").unwrap();
    let activity = Activity::new("act-1", "make check", ActivityKind::Verify);
    let mut original = WorkflowSession::new(sid.clone());
    original
        .start(SlotId::new("slot-1").unwrap(), lease.clone(), 1000)
        .unwrap();
    original.start_activity(activity.clone(), 1005).unwrap();
    let replayed = WorkflowSession::replay_from_events(sid, original.events())
        .unwrap()
        .with_compensator(Box::new(rec.clone()));
    (replayed, activity, lease)
}

#[test]
fn lost_slot_runs_compensators_and_readmits() {
    let rec = RecordingCompensator::new();
    let (mut session, activity, lease) = replayed_session(&rec);
    let err = ForgetfulLedger.find(&lease).unwrap_err();

    let disposition = session.on_activity_error(&activity, &err, 1010).unwrap();

    assert!(matches!(disposition, ActivityDisposition::Readmit(_)));
    let recorded = rec.recorded();
    assert_eq!(recorded.len(), 1, "the compensator runs once");
    assert_eq!(recorded[0].0, "act-1");
    assert!(recorded[0].1.contains("lost slot"), "{}", recorded[0].1);
    assert!(recorded[0].1.contains("lease-before-restart"));
    assert_eq!(session.state(), SessionState::Failed);
}

#[test]
fn lost_slot_ignores_other_domain_errors() {
    let rec = RecordingCompensator::new();
    let (mut session, activity, _) = replayed_session(&rec);
    let err = DomainError::TokenMismatch("bad".into());

    let disposition = session.on_activity_error(&activity, &err, 1010).unwrap();

    assert_eq!(disposition, ActivityDisposition::Failed);
    assert!(!rec.recorded()[0].1.contains("lost slot"));
    assert_eq!(session.state(), SessionState::Failed);
}

#[test]
fn lost_slot_ignores_a_different_lease_id() {
    let rec = RecordingCompensator::new();
    let (mut session, activity, _) = replayed_session(&rec);
    let err = DomainError::LeaseNotFound("some-other-lease".into());

    let disposition = session.on_activity_error(&activity, &err, 1010).unwrap();

    assert_eq!(disposition, ActivityDisposition::Failed);
    assert!(!rec.recorded()[0].1.contains("lost slot"));
}

/// Pushes its label into a shared log when it runs.
struct Labelled(
    &'static str,
    std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
);

impl uas_engine::Compensator for Labelled {
    fn name(&self) -> &'static str {
        self.0
    }
    fn compensate(
        &self,
        activity: &Activity,
        reason: &str,
    ) -> Result<Vec<uas_engine::CompensationReceipt>, uas_engine::EngineError> {
        self.1.lock().unwrap().push(self.0);
        Ok(vec![uas_engine::CompensationReceipt::new(
            &activity.id,
            self.0,
            true,
            reason.to_string(),
            0,
        )])
    }
}

#[test]
fn lost_slot_runs_compensators_in_reverse_order() {
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let composite = uas_engine::CompositeCompensator::new()
        .with(Labelled("first", log.clone()))
        .with(Labelled("second", log.clone()));
    let (session, activity, lease) = replayed_session(&RecordingCompensator::new());
    let mut session = session.with_compensator(Box::new(composite));
    let err = ForgetfulLedger.find(&lease).unwrap_err();

    session.on_activity_error(&activity, &err, 1010).unwrap();

    assert_eq!(*log.lock().unwrap(), vec!["second", "first"]);
}
