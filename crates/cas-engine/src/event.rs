//! Event-sourced workflow event log and replay storage.

use cas_domain::{LeaseId, SlotId};
use serde::{Deserialize, Serialize};

use crate::{Activity, ActivityResult, EngineError, SessionId, SessionState};

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
