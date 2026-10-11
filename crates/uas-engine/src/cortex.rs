// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Cortex mesh telemetry and settle queue integration.
//!
//! Provides event envelopes for Cortex agent mesh delivery and settlement cards
//! for the operator settle queue at `/v1/settle/pending`.

use serde::{Deserialize, Serialize};
use uas_domain::{Attestation, DomainError};

use crate::EngineError;

/// Structured agent intent, mirroring the cortex mesh `MessageIntent` contract
/// (`cortex-domain`). Wire form is snake_case: `ask`, `task`, `query`, `note`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshIntent {
    /// Decision or confirmation required from recipient before proceeding.
    Ask,
    /// Delegated unit of work expecting a result or artifact.
    Task,
    /// Inquiry expecting an informational reply.
    Query,
    /// Informational note, observation, or telemetry with no required response.
    Note,
}

impl std::fmt::Display for MeshIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ask => write!(f, "ask"),
            Self::Task => write!(f, "task"),
            Self::Query => write!(f, "query"),
            Self::Note => write!(f, "note"),
        }
    }
}

/// An attestation event broadcasted over the Cortex agent mesh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CortexMeshEvent {
    /// Associated session identifier.
    pub session_id: String,
    /// Originating mesh lane (e.g. `ceres/estate/w25:p3`).
    pub lane: String,
    /// Originating peer role (e.g. `session-agy-ceres-estate-w25-p3`).
    pub role: String,
    /// Event category (e.g. `uas.attestation`, `uas.session.started`).
    pub kind: String,
    /// Attestation receipt payload.
    pub attestation: Attestation,
    /// Emission timestamp in epoch seconds.
    pub timestamp: u64,
    /// Structured agent intent, mirroring the cortex mesh envelope contract.
    /// Absent is distinct from defaulted on the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<MeshIntent>,
    /// Bounded recursion hop counter; increments on each agent-to-agent hop.
    /// Mirrors the cortex mesh contract (max 10 hops, then human escalation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hop_count: Option<u32>,
}

impl CortexMeshEvent {
    /// Maximum permitted agent-to-agent recursion hops before human
    /// escalation; mirrors `Envelope::MAX_RECURSION_HOPS` in cortex-domain.
    pub const MAX_RECURSION_HOPS: u32 = 10;

    /// Advance the recursion hop count. Errors past the hop budget; the caller
    /// must escalate to a human instead of forwarding. A failed hop leaves the
    /// count untouched.
    pub fn increment_hop(&mut self) -> Result<u32, EngineError> {
        let next = self.hop_count.unwrap_or(0) + 1;
        if next > Self::MAX_RECURSION_HOPS {
            return Err(EngineError::Domain(DomainError::Invalid {
                kind: "hop count",
                value: next.to_string(),
                reason: "exceeds maximum recursion limit of 10; requires human escalation",
            }));
        }
        self.hop_count = Some(next);
        Ok(next)
    }
}

/// Status of an operator settle approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettleStatus {
    /// Awaiting operator review.
    Pending,
    /// Approved by the operator.
    Approved,
    /// Rejected by the operator.
    Rejected,
}

/// Settle card matching the `/v1/settle/pending` contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettleCard {
    /// Unique settlement request identifier.
    pub id: String,
    /// Associated session identifier.
    pub session_id: String,
    /// Step or activity requiring approval.
    pub step_id: String,
    /// Title of the action to approve.
    pub title: String,
    /// Operator rationale or context.
    pub rationale: String,
    /// Diff summary or target branch.
    pub diff_summary: Option<String>,
    /// Settlement state.
    pub status: SettleStatus,
    /// Creation timestamp in epoch seconds.
    pub created_at: u64,
}

impl SettleCard {
    /// Create a new pending settle card.
    pub fn new(
        id: impl Into<String>,
        session_id: impl Into<String>,
        step_id: impl Into<String>,
        title: impl Into<String>,
        rationale: impl Into<String>,
        created_at: u64,
    ) -> Self {
        Self {
            id: id.into(),
            session_id: session_id.into(),
            step_id: step_id.into(),
            title: title.into(),
            rationale: rationale.into(),
            diff_summary: None,
            status: SettleStatus::Pending,
            created_at,
        }
    }

    /// Mark the settle card approved.
    pub fn approve(&mut self) {
        self.status = SettleStatus::Approved;
    }

    /// Mark the settle card rejected.
    pub fn reject(&mut self) {
        self.status = SettleStatus::Rejected;
    }
}

/// In-memory queue managing pending settlement requests.
#[derive(Debug, Default, Clone)]
pub struct SettleQueue {
    cards: std::sync::Arc<std::sync::Mutex<Vec<SettleCard>>>,
}

impl SettleQueue {
    /// Create a new empty settle queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a new settlement request.
    pub fn enqueue(&self, card: SettleCard) -> Result<(), EngineError> {
        self.cards.lock().unwrap().push(card);
        Ok(())
    }

    /// List all pending settlement cards.
    pub fn pending(&self) -> Vec<SettleCard> {
        self.cards
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.status == SettleStatus::Pending)
            .cloned()
            .collect()
    }

    /// Resolve a settlement card by id.
    pub fn resolve(&self, id: &str, approve: bool) -> Result<SettleCard, EngineError> {
        let mut cards = self.cards.lock().unwrap();
        let card = cards
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| EngineError::StoreError(format!("settle card not found: {id}")))?;

        if approve {
            card.approve();
        } else {
            card.reject();
        }
        Ok(card.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uas_domain::{EvidenceTier, PrincipalRef};

    #[test]
    fn mesh_event_round_trip() {
        let att = Attestation {
            signer: PrincipalRef::new("uas/builder-1").unwrap(),
            evidence: EvidenceTier::Verified,
            receipt: "receipt-1".into(),
            at: 1000,
        };
        let event = CortexMeshEvent {
            session_id: "sess-1".into(),
            lane: "ceres/estate/w25:p3".into(),
            role: "session-agy-ceres-estate-w25-p3".into(),
            kind: "uas.attestation".into(),
            attestation: att,
            timestamp: 1000,
            intent: None,
            hop_count: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: CortexMeshEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, event);
    }

    fn event(intent: Option<MeshIntent>, hops: Option<u32>) -> CortexMeshEvent {
        CortexMeshEvent {
            session_id: "sess-1".into(),
            lane: "ceres/estate/w25:p5".into(),
            role: "session-omp-ceres-estate-w25-p5".into(),
            kind: "uas.attestation".into(),
            attestation: Attestation {
                signer: PrincipalRef::new("uas/builder-1").unwrap(),
                evidence: EvidenceTier::Verified,
                receipt: "receipt-1".into(),
                at: 1000,
            },
            timestamp: 1000,
            intent,
            hop_count: hops,
        }
    }

    #[test]
    fn mesh_event_intent_and_hops_round_trip() {
        let event = event(Some(MeshIntent::Task), Some(3));
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"intent\":\"task\""));
        assert!(json.contains("\"hop_count\":3"));
        let parsed: CortexMeshEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, event);
    }

    #[test]
    fn mesh_event_absent_fields_stay_absent() {
        // Pre-contract JSON has no intent/hop_count keys, and serializing None
        // emits none: absent is distinct from defaulted on the wire.
        let event = event(None, None);
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("intent"));
        assert!(!json.contains("hop_count"));
        let parsed: CortexMeshEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.intent, None);
        assert_eq!(parsed.hop_count, None);
    }

    #[test]
    fn hop_increment_bounds_at_ten() {
        let mut event = event(None, None);
        assert_eq!(event.increment_hop().unwrap(), 1);
        event.hop_count = Some(CortexMeshEvent::MAX_RECURSION_HOPS - 1);
        assert_eq!(
            event.increment_hop().unwrap(),
            CortexMeshEvent::MAX_RECURSION_HOPS
        );
        let err = event.increment_hop().unwrap_err();
        assert!(err.to_string().contains("human escalation"));
        // A failed hop leaves the count untouched.
        assert_eq!(event.hop_count, Some(CortexMeshEvent::MAX_RECURSION_HOPS));
    }

    #[test]
    fn mesh_intent_display_matches_wire() {
        assert_eq!(MeshIntent::Ask.to_string(), "ask");
        assert_eq!(MeshIntent::Task.to_string(), "task");
        assert_eq!(MeshIntent::Query.to_string(), "query");
        assert_eq!(MeshIntent::Note.to_string(), "note");
    }

    #[test]
    fn settle_queue_lifecycle() {
        let queue = SettleQueue::new();
        let mut card = SettleCard::new(
            "card-1",
            "sess-1",
            "step-gate",
            "Promote to main",
            "CI green, promote development to main",
            1000,
        );
        card.diff_summary = Some("3 commits, 5 files changed".into());

        queue.enqueue(card).unwrap();
        assert_eq!(queue.pending().len(), 1);

        let resolved = queue.resolve("card-1", true).unwrap();
        assert_eq!(resolved.status, SettleStatus::Approved);
        assert_eq!(queue.pending().len(), 0);
    }
}
