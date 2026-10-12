// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Read model: one row per workflow session, folded from its event log.

use uas_domain::{EvidenceTier, SlotId};
use uas_engine::{SessionId, SessionState, WorkflowEvent};

/// One session row on the deck. Built by folding the session's event log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// Session identifier.
    pub session_id: SessionId,
    /// Slot the session holds.
    pub slot_id: SlotId,
    /// Lifecycle state, as a replay of the same log reports it.
    pub state: SessionState,
    /// Title of the activity now running, if any.
    pub current_activity: Option<String>,
    /// Evidence tier of the last completed activity, if any.
    pub last_evidence: Option<EvidenceTier>,
    /// Epoch seconds of the newest event folded in.
    pub updated_at: u64,
}

impl SessionRow {
    /// Fold an event log into a row. Returns `None` when the log has no `SessionStarted`.
    pub fn from_events(events: &[WorkflowEvent]) -> Option<Self> {
        let mut row: Option<Self> = None;
        for event in events {
            match event {
                WorkflowEvent::SessionStarted {
                    session_id,
                    slot_id,
                    timestamp,
                    ..
                } => {
                    if row.is_none() {
                        row = Some(Self {
                            session_id: session_id.clone(),
                            slot_id: slot_id.clone(),
                            state: SessionState::Running,
                            current_activity: None,
                            last_evidence: None,
                            updated_at: *timestamp,
                        });
                    }
                }
                other => {
                    if let Some(current) = row.as_mut() {
                        current.apply(other);
                    }
                }
            }
        }
        row
    }

    /// Fold one event that follows `SessionStarted` into the row.
    fn apply(&mut self, event: &WorkflowEvent) {
        self.updated_at = event.timestamp();
        match event {
            WorkflowEvent::ActivityStarted { activity, .. } => {
                self.current_activity = Some(activity.title.clone());
            }
            WorkflowEvent::ActivityCompleted { result, .. } => {
                self.last_evidence = Some(result.attestation.evidence);
                self.current_activity = None;
            }
            WorkflowEvent::ActivityFailed { .. } => {
                self.current_activity = None;
            }
            WorkflowEvent::Compensating { .. } => {
                self.state = SessionState::Compensating;
            }
            WorkflowEvent::SessionFinished { final_state, .. } => {
                self.state = *final_state;
            }
            // The deck ignores wave steps and harness stream events in this slice.
            WorkflowEvent::WaveStep { .. } => {}
            WorkflowEvent::HarnessStream { .. } => {}
            WorkflowEvent::SessionStarted { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uas_domain::{Attestation, LeaseId, PrincipalRef};
    use uas_engine::{Activity, ActivityKind, ActivityResult, WorkflowSession};

    fn sid() -> SessionId {
        SessionId::new("sess-1").unwrap()
    }

    fn started() -> WorkflowEvent {
        WorkflowEvent::SessionStarted {
            sequence: 0,
            session_id: sid(),
            slot_id: SlotId::new("slot-1").unwrap(),
            lease_id: LeaseId::new("lease-1").unwrap(),
            timestamp: 100,
        }
    }

    fn activity_started(sequence: u64, title: &str, timestamp: u64) -> WorkflowEvent {
        WorkflowEvent::ActivityStarted {
            sequence,
            activity: Activity::new("act-1", title, ActivityKind::Verify),
            timestamp,
        }
    }

    fn activity_completed(sequence: u64, evidence: EvidenceTier, timestamp: u64) -> WorkflowEvent {
        WorkflowEvent::ActivityCompleted {
            sequence,
            result: ActivityResult {
                activity_id: "act-1".into(),
                success: true,
                attestation: Attestation {
                    signer: PrincipalRef::new("uas/builder-1").unwrap(),
                    evidence,
                    receipt: "receipt".into(),
                    at: timestamp,
                },
                summary: None,
                memory: None,
            },
            timestamp,
        }
    }

    fn activity_failed(sequence: u64, timestamp: u64) -> WorkflowEvent {
        WorkflowEvent::ActivityFailed {
            sequence,
            activity_id: "act-1".into(),
            reason: "exit 1".into(),
            timestamp,
        }
    }

    #[test]
    fn session_row_folds_events() {
        let mut log = vec![started(), activity_started(1, "make check", 111)];
        let row = SessionRow::from_events(&log).expect("log has SessionStarted");
        assert_eq!(row.session_id, sid());
        assert_eq!(row.slot_id.as_str(), "slot-1");
        assert_eq!(row.state, SessionState::Running);
        assert_eq!(row.current_activity.as_deref(), Some("make check"));
        assert_eq!(row.last_evidence, None);
        assert_eq!(row.updated_at, 111);

        log.push(activity_completed(2, EvidenceTier::Verified, 120));
        let row = SessionRow::from_events(&log).unwrap();
        assert_eq!(row.current_activity, None, "completed clears the activity");
        assert_eq!(row.last_evidence, Some(EvidenceTier::Verified));
        assert_eq!(row.updated_at, 120);

        log.push(activity_started(3, "lint", 130));
        log.push(activity_failed(4, 140));
        let row = SessionRow::from_events(&log).unwrap();
        assert_eq!(row.current_activity, None, "failed clears the activity");
        assert_eq!(
            row.last_evidence,
            Some(EvidenceTier::Verified),
            "failure keeps last evidence"
        );
        assert_eq!(row.updated_at, 140);
    }

    #[test]
    fn session_row_none_without_start() {
        assert_eq!(SessionRow::from_events(&[]), None);
        let log = vec![activity_started(0, "orphan", 5)];
        assert_eq!(SessionRow::from_events(&log), None);
    }

    #[test]
    fn session_row_state_matches_replay() {
        let running = vec![started()];
        let compensating = vec![
            started(),
            WorkflowEvent::Compensating {
                sequence: 1,
                reason: "step failed".into(),
                timestamp: 105,
            },
        ];
        let succeeded = vec![
            started(),
            activity_started(1, "make check", 110),
            activity_completed(2, EvidenceTier::Verified, 120),
            WorkflowEvent::SessionFinished {
                sequence: 3,
                final_state: SessionState::Succeeded,
                timestamp: 125,
            },
        ];
        let failed = vec![
            started(),
            activity_failed(1, 110),
            WorkflowEvent::Compensating {
                sequence: 2,
                reason: "step failed".into(),
                timestamp: 111,
            },
            WorkflowEvent::SessionFinished {
                sequence: 3,
                final_state: SessionState::Failed,
                timestamp: 115,
            },
        ];
        for log in [running, compensating, succeeded, failed] {
            let row = SessionRow::from_events(&log).unwrap();
            let replayed = WorkflowSession::replay_from_events(sid(), &log).unwrap();
            assert_eq!(row.state, replayed.state());
        }
    }
}
