// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Workflow session definitions and lifecycle state machine.

use serde::{Deserialize, Serialize};

use crate::EngineError;

/// Unique identifier for a workflow session.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    /// Create a validated session identifier.
    pub fn new(id: impl Into<String>) -> Result<Self, EngineError> {
        let s = id.into();
        if s.is_empty() {
            return Err(uas_domain::DomainError::Invalid {
                kind: "session id",
                value: s,
                reason: "must not be empty",
            }
            .into());
        }
        if s.len() > 64 {
            return Err(uas_domain::DomainError::Invalid {
                kind: "session id",
                value: s,
                reason: "must not exceed 64 characters",
            }
            .into());
        }
        Ok(Self(s))
    }

    /// Access the session identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Lifecycle states of a durable workflow session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Initial unstarted state before acquiring slot.
    Unstarted,
    /// Workflow session is actively executing activities.
    Running,
    /// Step failure occurred and compensators are rolling back state.
    Compensating,
    /// Session finished successfully with passing gates.
    Succeeded,
    /// Session finished with unrecoverable failure.
    Failed,
}

impl SessionState {
    /// Check whether this state is terminal (cannot transition further).
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }

    /// Check whether a transition from `self` to `target` is allowed.
    pub fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Unstarted, Self::Running)
                | (Self::Running, Self::Compensating)
                | (Self::Running, Self::Succeeded)
                | (Self::Running, Self::Failed)
                | (Self::Compensating, Self::Failed)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_session_id() {
        let id = SessionId::new("sess-123").unwrap();
        assert_eq!(id.as_str(), "sess-123");
        assert_eq!(format!("{id}"), "sess-123");
    }

    #[test]
    fn rejects_empty_id() {
        assert!(SessionId::new("").is_err());
    }

    #[test]
    fn state_transitions() {
        assert!(SessionState::Unstarted.can_transition_to(SessionState::Running));
        assert!(SessionState::Running.can_transition_to(SessionState::Succeeded));
        assert!(SessionState::Running.can_transition_to(SessionState::Compensating));
        assert!(SessionState::Compensating.can_transition_to(SessionState::Failed));

        assert!(!SessionState::Succeeded.can_transition_to(SessionState::Running));
        assert!(!SessionState::Failed.can_transition_to(SessionState::Running));
        assert!(SessionState::Succeeded.is_terminal());
        assert!(SessionState::Failed.is_terminal());
    }
}
