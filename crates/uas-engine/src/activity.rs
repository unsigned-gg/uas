//! Workflow activity definitions, kinds, and execution outcomes.

use serde::{Deserialize, Serialize};
use uas_domain::{Attestation, LostSlot};

/// Discrete classification of an activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    /// Pure verification or static check (e.g. tests, lint, type checks).
    Verify,
    /// Code generation, refactoring, or file modification.
    Impl,
    /// Operator gate or external sign-off signal.
    Gate,
}

/// Description of an activity scheduled within a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    /// Activity identifier.
    pub id: String,
    /// Human-readable title or prompt.
    pub title: String,
    /// Kind of activity.
    pub kind: ActivityKind,
}

impl Activity {
    /// Create a new activity specification.
    pub fn new(id: impl Into<String>, title: impl Into<String>, kind: ActivityKind) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            kind,
        }
    }
}

/// Outcome of executing an activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityResult {
    /// Identifier of the activity that executed.
    pub activity_id: String,
    /// Whether the execution completed successfully.
    pub success: bool,
    /// Attestation certifying the evidence tier and receipt.
    pub attestation: Attestation,
    /// Optional output summary or diagnostic text.
    pub summary: Option<String>,
}

/// What the caller does after an activity error (spec 0040 D5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityDisposition {
    /// The activity failed and the session ended in `Failed`.
    Failed,
    /// The pool no longer knows the lease. The compensators ran and the session
    /// ended in `Failed`. The caller acquires a fresh lease and re-admits the task.
    Readmit(LostSlot),
}

#[cfg(test)]
mod tests {
    use super::*;
    use uas_domain::{EvidenceTier, PrincipalRef};

    #[test]
    fn activity_construction() {
        let act = Activity::new("act-1", "Run make check", ActivityKind::Verify);
        assert_eq!(act.id, "act-1");
        assert_eq!(act.kind, ActivityKind::Verify);
    }

    #[test]
    fn activity_result_round_trip() {
        let att = Attestation {
            signer: PrincipalRef::new("uas/builder-1").unwrap(),
            evidence: EvidenceTier::Verified,
            receipt: "make check: ok".into(),
            at: 1000,
        };
        let res = ActivityResult {
            activity_id: "act-1".into(),
            success: true,
            attestation: att,
            summary: Some("all gates passed".into()),
        };
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"verified\""));
    }
}
