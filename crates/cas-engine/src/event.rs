//! Event-sourced workflow event log and replay storage.

use cas_domain::{LeaseId, SlotId};
use serde::{Deserialize, Serialize};

use crate::{Activity, ActivityResult, AgentRole, EngineError, SessionId, SessionState};

/// Event recorded in an append-only workflow log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowEvent {
    /// Session was started with bindings to an acquired slot and lease.
    SessionStarted {
        /// Sequence number in event log.
        sequence: u64,
        /// Session identifier.
        session_id: SessionId,
        /// Slot binding.
        slot_id: SlotId,
        /// Lease binding.
        lease_id: LeaseId,
        /// Epoch timestamp in seconds.
        timestamp: u64,
    },
    /// An activity started execution.
    ActivityStarted {
        /// Sequence number.
        sequence: u64,
        /// Activity details.
        activity: Activity,
        /// Epoch timestamp in seconds.
        timestamp: u64,
    },
    /// An activity completed with an attestation receipt.
    ActivityCompleted {
        /// Sequence number.
        sequence: u64,
        /// Execution outcome.
        result: ActivityResult,
        /// Epoch timestamp in seconds.
        timestamp: u64,
    },
    /// An activity failed during execution.
    ActivityFailed {
        /// Sequence number.
        sequence: u64,
        /// Identifier of failed activity.
        activity_id: String,
        /// Failure details.
        reason: String,
        /// Epoch timestamp in seconds.
        timestamp: u64,
    },
    /// Rollback/compensation phase entered.
    Compensating {
        /// Sequence number.
        sequence: u64,
        /// Cause for compensation.
        reason: String,
        /// Epoch timestamp in seconds.
        timestamp: u64,
    },
    /// One wave step ran on the custody chain (spec 0025 D5). A fact is `None` when the
    /// step did not reach it.
    WaveStep {
        /// Sequence number.
        sequence: u64,
        /// The step id from the wave plan.
        step: String,
        /// The wave index in the plan.
        wave: u32,
        /// The role that ran the step.
        role: AgentRole,
        /// The persona that ran the step, when a model ran it (spec 0025 D6).
        persona: Option<String>,
        /// How the step ended.
        outcome: WaveOutcome,
        /// The 12-character Blackwall run prefix (spec 0025 D12).
        run: Option<String>,
        /// The 12-character changeset prefix (spec 0025 D12).
        changeset: Option<String>,
        /// The wave base commit.
        base: Option<String>,
        /// The landing branch, `blackwall/<run>`.
        branch: Option<String>,
        /// The reconciled head commit.
        head: Option<String>,
        /// The pull request number, when one was opened.
        pr: Option<u64>,
        /// Epoch timestamp in seconds.
        timestamp: u64,
    },
    /// Session concluded in a terminal state.
    SessionFinished {
        /// Sequence number.
        sequence: u64,
        /// Terminal state reached.
        final_state: SessionState,
        /// Epoch timestamp in seconds.
        timestamp: u64,
    },
}

/// How one wave step ended. A step that is not ready writes no event (spec 0025 D5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum WaveOutcome {
    /// The step passed. A Builder has a reconciled head; a Critic has a no-defects verdict.
    Passed,
    /// The step is a MainSession gate. The saga gate holds it until a person approves.
    AwaitingGate,
    /// The step halted with a reason from the spec 0025 admission and halt table.
    Halted {
        /// The halt reason, for example `EvaluatorTouched`.
        reason: String,
    },
}

impl WorkflowEvent {
    /// Sequence number of this event.
    pub fn sequence(&self) -> u64 {
        match self {
            Self::SessionStarted { sequence, .. } => *sequence,
            Self::ActivityStarted { sequence, .. } => *sequence,
            Self::ActivityCompleted { sequence, .. } => *sequence,
            Self::ActivityFailed { sequence, .. } => *sequence,
            Self::Compensating { sequence, .. } => *sequence,
            Self::SessionFinished { sequence, .. } => *sequence,
            Self::WaveStep { sequence, .. } => *sequence,
        }
    }

    /// Timestamp of this event in epoch seconds.
    pub fn timestamp(&self) -> u64 {
        match self {
            Self::SessionStarted { timestamp, .. } => *timestamp,
            Self::ActivityStarted { timestamp, .. } => *timestamp,
            Self::ActivityCompleted { timestamp, .. } => *timestamp,
            Self::ActivityFailed { timestamp, .. } => *timestamp,
            Self::Compensating { timestamp, .. } => *timestamp,
            Self::SessionFinished { timestamp, .. } => *timestamp,
            Self::WaveStep { timestamp, .. } => *timestamp,
        }
    }
}

/// Trait for append-only event log persistence.
pub trait EventStore: Send + Sync {
    /// Append an event to the log.
    fn append(&mut self, event: WorkflowEvent) -> Result<(), EngineError>;

    /// Retrieve all recorded events.
    fn events(&self) -> &[WorkflowEvent];

    /// Number of recorded events.
    fn len(&self) -> usize {
        self.events().len()
    }

    /// Check whether the store is empty.
    fn is_empty(&self) -> bool {
        self.events().is_empty()
    }
}

/// In-memory event store for deterministic execution and testing.
#[derive(Debug, Default, Clone)]
pub struct InMemoryEventStore {
    events: Vec<WorkflowEvent>,
}

impl InMemoryEventStore {
    /// Create a new empty in-memory event store.
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl EventStore for InMemoryEventStore {
    fn append(&mut self, event: WorkflowEvent) -> Result<(), EngineError> {
        let expected_seq = self.events.len() as u64;
        if event.sequence() != expected_seq {
            return Err(EngineError::ReplayFailed {
                sequence: event.sequence(),
                reason: format!("out of order event sequence; expected {expected_seq}"),
            });
        }
        self.events.push(event);
        Ok(())
    }

    fn events(&self) -> &[WorkflowEvent] {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_step_event_carries_custody_facts() {
        let ev = WorkflowEvent::WaveStep {
            sequence: 0,
            step: "build-a".into(),
            wave: 1,
            role: AgentRole::Builder,
            persona: Some("cas-builder".into()),
            outcome: WaveOutcome::Halted {
                reason: "EvaluatorTouched".into(),
            },
            run: Some("bw-0123456789".into()),
            changeset: Some("blake3-abcd".into()),
            base: Some("056d7c7abcdef".into()),
            branch: Some("blackwall/bw-0123456789".into()),
            head: None,
            pr: None,
            timestamp: 42,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"wave_step\""), "{json}");
        assert!(json.contains("\"result\":\"halted\""), "{json}");
        let back: WorkflowEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ev);
        assert_eq!(back.sequence(), 0);
        assert_eq!(back.timestamp(), 42);
    }

    #[test]
    fn event_store_appends_sequentially() {
        let mut store = InMemoryEventStore::new();
        assert!(store.is_empty());

        let ev0 = WorkflowEvent::Compensating {
            sequence: 0,
            reason: "test".into(),
            timestamp: 10,
        };
        assert!(store.append(ev0).is_ok());
        assert_eq!(store.len(), 1);

        // Sequence mismatch fails
        let ev_bad = WorkflowEvent::Compensating {
            sequence: 3,
            reason: "bad".into(),
            timestamp: 11,
        };
        assert!(store.append(ev_bad).is_err());
    }
}
