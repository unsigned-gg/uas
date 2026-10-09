//! Cortex mesh telemetry and settle queue integration.
//!
//! Provides event envelopes for Cortex agent mesh delivery and settlement cards
//! for the operator settle queue at `/v1/settle/pending`.

use cas_domain::Attestation;
use serde::{Deserialize, Serialize};

use crate::EngineError;

/// An attestation event broadcasted over the Cortex agent mesh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CortexMeshEvent {
    /// Associated session identifier.
    pub session_id: String,
    /// Originating mesh lane (e.g. `ceres/estate/w25:p3`).
    pub lane: String,
    /// Originating peer role (e.g. `session-agy-ceres-estate-w25-p3`).
    pub role: String,
    /// Event category (e.g. `cas.attestation`, `cas.session.started`).
    pub kind: String,
    /// Attestation receipt payload.
    pub attestation: Attestation,
    /// Emission timestamp in epoch seconds.
    pub timestamp: u64,
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
    use cas_domain::{EvidenceTier, PrincipalRef};

    #[test]
    fn mesh_event_round_trip() {
        let att = Attestation {
            signer: PrincipalRef::new("cas/builder-1").unwrap(),
            evidence: EvidenceTier::Verified,
            receipt: "receipt-1".into(),
            at: 1000,
        };
        let event = CortexMeshEvent {
            session_id: "sess-1".into(),
            lane: "ceres/estate/w25:p3".into(),
            role: "session-agy-ceres-estate-w25-p3".into(),
            kind: "cas.attestation".into(),
            attestation: att,
            timestamp: 1000,
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: CortexMeshEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, event);
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
