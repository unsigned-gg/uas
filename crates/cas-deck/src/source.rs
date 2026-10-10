//! Data sources the deck reads. The traits keep the HTTP layer free of engine wiring.

use std::sync::Arc;

use cas_engine::{EventStore, InMemoryEventStore, SettleCard, SettleQueue, WorkflowEvent};

use crate::DeckError;

/// Read-only access to workflow session logs.
pub trait SessionSource: Send + Sync {
    /// One event log per session, in the order the sessions were recorded.
    fn session_logs(&self) -> Vec<Vec<WorkflowEvent>>;
}

impl SessionSource for Vec<InMemoryEventStore> {
    fn session_logs(&self) -> Vec<Vec<WorkflowEvent>> {
        self.iter().map(|store| store.events().to_vec()).collect()
    }
}

/// Read-only access to the settle queue.
pub trait SettleSource: Send + Sync {
    /// Cards that still wait for an operator decision.
    fn pending_cards(&self) -> Result<Vec<SettleCard>, DeckError>;
}

impl SettleSource for SettleQueue {
    fn pending_cards(&self) -> Result<Vec<SettleCard>, DeckError> {
        Ok(self.pending())
    }
}

/// Shared state for every route.
#[derive(Clone)]
pub struct DeckState {
    sessions: Arc<dyn SessionSource>,
    settle: Arc<dyn SettleSource>,
}

impl DeckState {
    /// Build the state from a session source and a settle source.
    pub fn new(sessions: Arc<dyn SessionSource>, settle: Arc<dyn SettleSource>) -> Self {
        Self { sessions, settle }
    }

    /// The session source.
    pub(crate) fn sessions(&self) -> &dyn SessionSource {
        self.sessions.as_ref()
    }

    /// The settle source.
    pub(crate) fn settle(&self) -> &dyn SettleSource {
        self.settle.as_ref()
    }
}
