// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Durable workflow session lifecycle manager and event replayer.

use uas_domain::{DomainError, LeaseId, LostSlot, SlotId};

use crate::compensator::receipts_or_failure;
use crate::{
    Activity, ActivityDisposition, ActivityResult, CompensationReceipt, Compensator, EngineError,
    EventStore, InMemoryEventStore, NoopCompensator, SessionId, SessionState, WorkflowEvent,
};

/// A durable workflow session managing execution turns and event logging.
pub struct WorkflowSession<S: EventStore = InMemoryEventStore> {
    session_id: SessionId,
    state: SessionState,
    slot_id: Option<SlotId>,
    lease_id: Option<LeaseId>,
    store: S,
    compensator: Box<dyn Compensator>,
    compensation_receipts: Vec<CompensationReceipt>,
}

impl WorkflowSession<InMemoryEventStore> {
    /// Create a new session with in-memory event storage and a default no-op compensator.
    pub fn new(session_id: SessionId) -> Self {
        Self::with_store_and_compensator(
            session_id,
            InMemoryEventStore::new(),
            Box::new(NoopCompensator),
        )
    }

    /// Rebuild session state from a recorded event sequence without side effects.
    pub fn replay_from_events(
        session_id: SessionId,
        events: &[WorkflowEvent],
    ) -> Result<Self, EngineError> {
        let mut session = WorkflowSession::new(session_id);
        for event in events {
            match event {
                WorkflowEvent::SessionStarted {
                    sequence,
                    slot_id,
                    lease_id,
                    timestamp: _,
                    ..
                } => {
                    if *sequence != session.store.len() as u64 {
                        return Err(EngineError::ReplayFailed {
                            sequence: *sequence,
                            reason: "non-sequential sequence number".into(),
                        });
                    }
                    session.slot_id = Some(slot_id.clone());
                    session.lease_id = Some(lease_id.clone());
                    session.state = SessionState::Running;
                    session.store.append(event.clone())?;
                }
                WorkflowEvent::ActivityStarted { sequence, .. } => {
                    if *sequence != session.store.len() as u64 {
                        return Err(EngineError::ReplayFailed {
                            sequence: *sequence,
                            reason: "non-sequential sequence number".into(),
                        });
                    }
                    session.store.append(event.clone())?;
                }
                WorkflowEvent::ActivityCompleted { sequence, .. } => {
                    if *sequence != session.store.len() as u64 {
                        return Err(EngineError::ReplayFailed {
                            sequence: *sequence,
                            reason: "non-sequential sequence number".into(),
                        });
                    }
                    session.store.append(event.clone())?;
                }
                WorkflowEvent::ActivityFailed { sequence, .. } => {
                    if *sequence != session.store.len() as u64 {
                        return Err(EngineError::ReplayFailed {
                            sequence: *sequence,
                            reason: "non-sequential sequence number".into(),
                        });
                    }
                    session.store.append(event.clone())?;
                }
                WorkflowEvent::Compensating { sequence, .. } => {
                    if *sequence != session.store.len() as u64 {
                        return Err(EngineError::ReplayFailed {
                            sequence: *sequence,
                            reason: "non-sequential sequence number".into(),
                        });
                    }
                    session.state = SessionState::Compensating;
                    session.store.append(event.clone())?;
                }
                WorkflowEvent::WaveStep { sequence, .. } => {
                    if *sequence != session.store.len() as u64 {
                        return Err(EngineError::ReplayFailed {
                            sequence: *sequence,
                            reason: "non-sequential sequence number".into(),
                        });
                    }
                    session.store.append(event.clone())?;
                }
                WorkflowEvent::SessionFinished {
                    sequence,
                    final_state,
                    ..
                } => {
                    if *sequence != session.store.len() as u64 {
                        return Err(EngineError::ReplayFailed {
                            sequence: *sequence,
                            reason: "non-sequential sequence number".into(),
                        });
                    }
                    session.state = *final_state;
                    session.store.append(event.clone())?;
                }
            }
        }
        Ok(session)
    }
}

impl<S: EventStore> WorkflowSession<S> {
    /// Create a session with custom event store and compensator.
    pub fn with_store_and_compensator(
        session_id: SessionId,
        store: S,
        compensator: Box<dyn Compensator>,
    ) -> Self {
        Self {
            session_id,
            state: SessionState::Unstarted,
            slot_id: None,
            lease_id: None,
            store,
            compensator,
            compensation_receipts: Vec::new(),
        }
    }

    /// Attach a compensator, for example to a session rebuilt by replay.
    #[must_use]
    pub fn with_compensator(mut self, compensator: Box<dyn Compensator>) -> Self {
        self.compensator = compensator;
        self
    }

    /// Map an activity error to a disposition (spec 0040 D5).
    ///
    /// `LeaseNotFound` for this session's own lease means the pool restarted and lost the
    /// slot. The compensators run once with the [`LostSlot`] reason and the result is
    /// `Readmit`. Every other error takes the plain [`Self::fail_activity`] path and returns
    /// `Failed`. The engine has no lease port, so the caller acquires the fresh lease.
    pub fn on_activity_error(
        &mut self,
        activity: &Activity,
        err: &DomainError,
        timestamp: u64,
    ) -> Result<ActivityDisposition, EngineError> {
        if let (DomainError::LeaseNotFound(id), Some(own)) = (err, self.lease_id.as_ref()) {
            if id == own.as_str() {
                let lost = LostSlot::new(own);
                self.fail_activity(activity, lost.to_string(), timestamp)?;
                return Ok(ActivityDisposition::Readmit(lost));
            }
        }
        self.fail_activity(activity, err.to_string(), timestamp)?;
        Ok(ActivityDisposition::Failed)
    }

    /// Access the session identifier.
    pub fn id(&self) -> &SessionId {
        &self.session_id
    }

    /// Current lifecycle state.
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Associated slot binding, if started.
    pub fn slot_id(&self) -> Option<&SlotId> {
        self.slot_id.as_ref()
    }

    /// Associated lease binding, if started.
    pub fn lease_id(&self) -> Option<&LeaseId> {
        self.lease_id.as_ref()
    }

    /// Access recorded events in the log.
    pub fn events(&self) -> &[WorkflowEvent] {
        self.store.events()
    }

    /// Access compensation receipts emitted during rollback, if any.
    pub fn compensation_receipts(&self) -> &[CompensationReceipt] {
        &self.compensation_receipts
    }

    /// Check if an activity has already been completed in this session's history.
    pub fn is_activity_completed(&self, activity_id: &str) -> bool {
        self.store.events().iter().any(|ev| match ev {
            WorkflowEvent::ActivityCompleted { result, .. } => result.activity_id == activity_id,
            _ => false,
        })
    }

    /// Retrieve the cached result of an already completed activity, if present.
    pub fn cached_result(&self, activity_id: &str) -> Option<&ActivityResult> {
        self.store.events().iter().find_map(|ev| match ev {
            WorkflowEvent::ActivityCompleted { result, .. }
                if result.activity_id == activity_id =>
            {
                Some(result)
            }
            _ => None,
        })
    }

    /// Start the workflow session by binding an acquired slot and lease.
    pub fn start(
        &mut self,
        slot_id: SlotId,
        lease_id: LeaseId,
        timestamp: u64,
    ) -> Result<(), EngineError> {
        self.transition_to(SessionState::Running)?;
        self.slot_id = Some(slot_id.clone());
        self.lease_id = Some(lease_id.clone());

        tracing::debug!(
            session_id = %self.session_id,
            %slot_id,
            %lease_id,
            "session started"
        );

        let seq = self.store.len() as u64;
        self.store.append(WorkflowEvent::SessionStarted {
            sequence: seq,
            session_id: self.session_id.clone(),
            slot_id,
            lease_id,
            timestamp,
        })
    }

    /// Begin an activity within the session.
    pub fn start_activity(
        &mut self,
        activity: Activity,
        timestamp: u64,
    ) -> Result<(), EngineError> {
        if self.state != SessionState::Running {
            return Err(EngineError::InvalidStateTransition {
                from: self.state,
                to: SessionState::Running,
                reason: "cannot start activity unless session is in Running state",
            });
        }
        tracing::debug!(
            session_id = %self.session_id,
            activity_id = %activity.id,
            "activity started"
        );
        let seq = self.store.len() as u64;
        self.store.append(WorkflowEvent::ActivityStarted {
            sequence: seq,
            activity,
            timestamp,
        })
    }

    /// Record successful completion of an activity.
    pub fn complete_activity(
        &mut self,
        result: ActivityResult,
        timestamp: u64,
    ) -> Result<(), EngineError> {
        if self.state != SessionState::Running {
            return Err(EngineError::InvalidStateTransition {
                from: self.state,
                to: SessionState::Running,
                reason: "cannot complete activity unless session is in Running state",
            });
        }
        tracing::debug!(
            session_id = %self.session_id,
            activity_id = %result.activity_id,
            "activity completed"
        );
        let seq = self.store.len() as u64;
        self.store.append(WorkflowEvent::ActivityCompleted {
            sequence: seq,
            result,
            timestamp,
        })
    }

    /// Fail an activity and trigger saga compensation rollback.
    ///
    /// The compensator runs once. Every receipt it returns is kept, and the log ends with
    /// `SessionFinished` in `Failed`. Returns `Ok(())` when the compensator returns `Ok`, even
    /// if one of its receipts reports failure. Returns `CompensationIncomplete` only when the
    /// compensator itself returns `Err`; the session still ends in `Failed` with that receipt kept.
    pub fn fail_activity(
        &mut self,
        activity: &Activity,
        reason: String,
        timestamp: u64,
    ) -> Result<(), EngineError> {
        if self.state != SessionState::Running {
            return Err(EngineError::InvalidStateTransition {
                from: self.state,
                to: SessionState::Compensating,
                reason: "cannot fail activity unless session is in Running state",
            });
        }

        tracing::warn!(
            session_id = %self.session_id,
            activity_id = %activity.id,
            %reason,
            "activity failed, compensating"
        );

        let seq = self.store.len() as u64;
        self.store.append(WorkflowEvent::ActivityFailed {
            sequence: seq,
            activity_id: activity.id.clone(),
            reason: reason.clone(),
            timestamp,
        })?;

        self.state = SessionState::Compensating;
        let comp_seq = self.store.len() as u64;
        self.store.append(WorkflowEvent::Compensating {
            sequence: comp_seq,
            reason: reason.clone(),
            timestamp,
        })?;

        // Run the compensator. Keep every receipt, even when the compensator returned Err.
        let result = self.compensator.compensate(activity, &reason);
        let direct_err = result.is_err();
        let receipts = receipts_or_failure(self.compensator.name(), activity, result);
        self.compensation_receipts.extend(receipts.clone());

        self.state = SessionState::Failed;
        let fin_seq = self.store.len() as u64;
        self.store.append(WorkflowEvent::SessionFinished {
            sequence: fin_seq,
            final_state: SessionState::Failed,
            timestamp,
        })?;

        if direct_err {
            let failed = receipts.iter().filter(|r| !r.success).count();
            return Err(EngineError::CompensationIncomplete { failed, receipts });
        }
        Ok(())
    }

    /// Conclude the session with success.
    pub fn finish_success(&mut self, timestamp: u64) -> Result<(), EngineError> {
        self.transition_to(SessionState::Succeeded)?;
        tracing::info!(session_id = %self.session_id, "session finished successfully");
        let seq = self.store.len() as u64;
        self.store.append(WorkflowEvent::SessionFinished {
            sequence: seq,
            final_state: SessionState::Succeeded,
            timestamp,
        })
    }

    fn transition_to(&mut self, target: SessionState) -> Result<(), EngineError> {
        if !self.state.can_transition_to(target) {
            return Err(EngineError::InvalidStateTransition {
                from: self.state,
                to: target,
                reason: "illegal state transition",
            });
        }
        self.state = target;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensator::FailOnCompensator;
    use crate::{ActivityKind, CompositeCompensator};
    use uas_domain::{Attestation, EvidenceTier, PrincipalRef};

    #[test]
    fn full_successful_session_lifecycle() {
        let sess_id = SessionId::new("sess-1").unwrap();
        let slot_id = SlotId::new("slot-1").unwrap();
        let lease_id = LeaseId::new("lease-1").unwrap();

        let mut sess = WorkflowSession::new(sess_id.clone());
        assert_eq!(sess.state(), SessionState::Unstarted);

        sess.start(slot_id, lease_id, 100).unwrap();
        assert_eq!(sess.state(), SessionState::Running);

        let act = Activity::new("act-1", "Verify gates", ActivityKind::Verify);
        sess.start_activity(act.clone(), 105).unwrap();

        let att = Attestation {
            signer: PrincipalRef::new("uas/agent-1").unwrap(),
            evidence: EvidenceTier::Verified,
            receipt: "make check: ok".into(),
            at: 110,
        };
        let res = ActivityResult {
            activity_id: act.id,
            success: true,
            attestation: att,
            summary: Some("passed".into()),
        };
        sess.complete_activity(res, 110).unwrap();
        sess.finish_success(120).unwrap();

        assert_eq!(sess.state(), SessionState::Succeeded);
        assert_eq!(sess.events().len(), 4);

        // Replay verification
        let replayed = WorkflowSession::replay_from_events(sess_id, sess.events()).unwrap();
        assert_eq!(replayed.state(), SessionState::Succeeded);
        assert_eq!(replayed.events().len(), 4);
    }

    #[test]
    fn session_failure_triggers_compensation() {
        let sess_id = SessionId::new("sess-fail").unwrap();
        let slot_id = SlotId::new("slot-1").unwrap();
        let lease_id = LeaseId::new("lease-1").unwrap();

        let mut sess = WorkflowSession::new(sess_id);
        sess.start(slot_id, lease_id, 100).unwrap();

        let act = Activity::new("act-fail", "Run broken check", ActivityKind::Verify);
        sess.start_activity(act.clone(), 105).unwrap();
        sess.fail_activity(&act, "make check exit 1".into(), 110)
            .unwrap();

        assert_eq!(sess.state(), SessionState::Failed);
        assert_eq!(sess.events().len(), 5);
    }

    /// Start a session with `compensator`, then start activity `act-x`.
    fn session_running_act_x(
        compensator: Box<dyn Compensator>,
    ) -> (WorkflowSession<InMemoryEventStore>, Activity) {
        let mut sess = WorkflowSession::with_store_and_compensator(
            SessionId::new("sess-comp-err").unwrap(),
            InMemoryEventStore::new(),
            compensator,
        );
        sess.start(
            SlotId::new("slot-1").unwrap(),
            LeaseId::new("lease-1").unwrap(),
            100,
        )
        .unwrap();
        let act = Activity::new("act-x", "Run", ActivityKind::Verify);
        sess.start_activity(act.clone(), 105).unwrap();
        (sess, act)
    }

    #[test]
    fn fail_activity_records_receipt_when_compensator_errs() {
        let (mut sess, act) = session_running_act_x(Box::new(FailOnCompensator::new("act-x")));

        let err = sess.fail_activity(&act, "boom".into(), 110).unwrap_err();
        let EngineError::CompensationIncomplete { failed, receipts } = err else {
            panic!("expected CompensationIncomplete, got {err:?}");
        };
        assert_eq!(failed, 1);
        assert_eq!(receipts.len(), 1);
        assert!(!receipts[0].success);
        assert_eq!(receipts[0].handler_name, "fail_on");
        assert_eq!(sess.compensation_receipts(), receipts.as_slice());
        assert_eq!(sess.state(), SessionState::Failed);
        assert_eq!(sess.events().len(), 5);
        assert!(matches!(
            sess.events().last(),
            Some(WorkflowEvent::SessionFinished {
                final_state: SessionState::Failed,
                ..
            })
        ));
    }

    #[test]
    fn replay_after_compensator_error_yields_failed() {
        let (mut sess, act) = session_running_act_x(Box::new(FailOnCompensator::new("act-x")));
        let _ = sess.fail_activity(&act, "boom".into(), 110);

        let replayed = WorkflowSession::replay_from_events(
            SessionId::new("sess-comp-err").unwrap(),
            sess.events(),
        )
        .unwrap();
        assert_eq!(replayed.state(), SessionState::Failed);
        assert_eq!(replayed.events().len(), 5);
    }

    #[test]
    fn fail_activity_composite_child_failure_stays_ok() {
        let comp = CompositeCompensator::new().with(FailOnCompensator::new("act-x"));
        let (mut sess, act) = session_running_act_x(Box::new(comp));

        sess.fail_activity(&act, "boom".into(), 110).unwrap();
        assert_eq!(sess.compensation_receipts().len(), 1);
        assert!(!sess.compensation_receipts()[0].success);
        assert_eq!(sess.state(), SessionState::Failed);
        assert_eq!(sess.events().len(), 5);
    }
}
