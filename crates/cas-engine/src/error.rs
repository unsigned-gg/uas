//! Engine error definitions.

use cas_domain::DomainError;
use thiserror::Error;

/// Errors produced by the durable workflow engine.
#[derive(Debug, Error)]
pub enum EngineError {
    /// An underlying domain error.
    #[error("domain error: {0}")]
    Domain(#[from] DomainError),

    /// Invalid state transition for a session.
    #[error("invalid session state transition from {from:?} to {to:?}: {reason}")]
    InvalidStateTransition {
        /// Current state.
        from: crate::SessionState,
        /// Attempted destination state.
        to: crate::SessionState,
        /// Detail why transition was rejected.
        reason: &'static str,
    },

    /// Activity failure.
    #[error("activity {id} failed: {reason}")]
    ActivityFailed {
        /// Identifier of the activity.
        id: String,
        /// Detail on why it failed.
        reason: String,
    },

    /// Compensation failure during rollback.
    #[error("compensation failed for {activity_id}: {reason}")]
    CompensationFailed {
        /// Identifier of the activity being compensated.
        activity_id: String,
        /// Reason for compensation failure.
        reason: String,
    },

    /// Event replay error.
    #[error("event replay failed at sequence {sequence}: {reason}")]
    ReplayFailed {
        /// Replay sequence number.
        sequence: u64,
        /// Reason for failure.
        reason: String,
    },
}
