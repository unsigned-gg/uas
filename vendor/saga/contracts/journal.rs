//! Run journal port. Pure types and a trait; storage lives in adapters (bifrost `orlog`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One append-only journal entry. `seq` is assigned by the journal on append
/// (the value passed in is ignored); it starts at 1 per run.
///
/// `payload` is the event body as JSON text. saga-domain carries no JSON
/// value type, so the adapter owns parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEvent {
    pub run_id: String,
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub kind: String,
    pub task: Option<String>,
    pub attempt: Option<String>,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalError {
    /// The backing store failed.
    Storage(String),
    /// An entry could not be encoded or decoded.
    Codec(String),
}

impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JournalError::Storage(m) => write!(f, "journal storage error: {m}"),
            JournalError::Codec(m) => write!(f, "journal codec error: {m}"),
        }
    }
}

impl std::error::Error for JournalError {}

/// Append-only, per-run ordered event log.
pub trait Journal: Send + Sync {
    /// Append an event; returns the assigned sequence number.
    fn append(&self, ev: &RunEvent) -> Result<u64, JournalError>;
    /// All events of a run in sequence order.
    fn replay(&self, run_id: &str) -> Result<Vec<RunEvent>, JournalError>;
    /// Highest assigned sequence number, or `None` for an unknown run.
    fn head(&self, run_id: &str) -> Result<Option<u64>, JournalError>;
}
