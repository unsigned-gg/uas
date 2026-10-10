//! Landing receipts for changesets that Blackwall reconciled and opened as pull requests.
//!
//! A receipt records the custody facts of one landing and the evidence tier
//! those facts support. The tier rule lives here: `verified` only when the
//! verify command exited zero and no evaluator file changed (spec 0022 D2,
//! spec 0023 D3). The subprocess adapter lives in `cas-engine`.

use serde::{Deserialize, Serialize};

use crate::{EvidenceTier, SlotId};

/// Audit state of the slot run that produced a changeset (spec 0014).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AuditStatus {
    /// Every exec event reached the transcript digest.
    Attested,
    /// Some exec events were dropped. The landing still proceeds (spec 0022 D4).
    Unattested {
        /// How many exec events were dropped.
        dropped_events: u64,
    },
}

/// Where the cost figures of a landing came from (spec 0023 D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostSource {
    /// The harness reported the cost.
    Reported,
    /// The harness reported no cost.
    None,
}

/// The custody facts of one landing, before its evidence tier is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandingFacts {
    /// The slot whose changeset landed.
    pub slot_id: SlotId,
    /// The slot run hash from the spec 0014 audit receipt.
    pub run_hash: String,
    /// Whether the slot audit attested every event.
    pub audit: AuditStatus,
    /// The CPU cap the slot ran under: `200000 100000`, `max` or `uncapped` (spec 0020).
    pub cpu_cap: String,
    /// The Blackwall run hash, from `blackwall show`.
    pub blackwall_run: String,
    /// The BLAKE3 id of the changeset.
    pub changeset_hash: String,
    /// The base commit the changeset was reconciled against.
    pub base_sha: String,
    /// True when the base was not an ancestor of the development head. Nothing landed.
    pub stale_base: bool,
    /// The verify command that reconcile ran.
    pub verify_cmd: String,
    /// The exit code of the verify command. `None` when reconcile did not run.
    pub verify_exit: Option<i32>,
    /// The landing branch, `blackwall/<short run>`, when one was created.
    pub branch: Option<String>,
    /// The pull request number, when one was opened.
    pub pr_number: Option<u64>,
    /// True when the changeset touched an evaluator file (spec 0023 D3).
    pub evaluator_touched: bool,
    /// Where the cost figures came from (spec 0023 D2).
    pub cost_source: CostSource,
    /// When the landing was recorded, in epoch seconds.
    pub at: u64,
}

/// A landing receipt whose evidence tier follows the tier rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawLandingReceipt")]
pub struct LandingReceipt {
    #[serde(flatten)]
    facts: LandingFacts,
    evidence: EvidenceTier,
}

/// A rejected landing receipt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LandingReceiptError {
    /// The evidence tier is not the tier the rule gives for these facts.
    #[error(
        "evidence {evidence:?} does not follow from verify exit {verify_exit:?} \
         and evaluator_touched {evaluator_touched}: the rule gives {expected:?}"
    )]
    EvidenceMismatch {
        /// The tier the receipt claims.
        evidence: EvidenceTier,
        /// The tier the rule gives.
        expected: EvidenceTier,
        /// The verify exit code the receipt records.
        verify_exit: Option<i32>,
        /// The evaluator flag the receipt records.
        evaluator_touched: bool,
    },
}

/// The wire form that a receipt must pass before it is accepted.
#[derive(Deserialize)]
struct RawLandingReceipt {
    #[serde(flatten)]
    facts: LandingFacts,
    evidence: EvidenceTier,
}

impl LandingReceipt {
    /// Build a receipt. The evidence tier follows from the facts.
    pub fn new(facts: LandingFacts) -> Self {
        let evidence = Self::evidence_for(facts.verify_exit, facts.evaluator_touched);
        Self { facts, evidence }
    }

    /// The tier rule: `verified` iff the verify command exited zero and no
    /// evaluator file changed. Otherwise `reported`.
    pub fn evidence_for(verify_exit: Option<i32>, evaluator_touched: bool) -> EvidenceTier {
        if verify_exit == Some(0) && !evaluator_touched {
            EvidenceTier::Verified
        } else {
            EvidenceTier::Reported
        }
    }

    /// The custody facts of this landing.
    pub fn facts(&self) -> &LandingFacts {
        &self.facts
    }

    /// The evidence tier of this landing.
    pub fn evidence(&self) -> EvidenceTier {
        self.evidence
    }
}

impl TryFrom<RawLandingReceipt> for LandingReceipt {
    type Error = LandingReceiptError;

    fn try_from(raw: RawLandingReceipt) -> Result<Self, Self::Error> {
        let expected = Self::evidence_for(raw.facts.verify_exit, raw.facts.evaluator_touched);
        if raw.evidence != expected {
            return Err(LandingReceiptError::EvidenceMismatch {
                evidence: raw.evidence,
                expected,
                verify_exit: raw.facts.verify_exit,
                evaluator_touched: raw.facts.evaluator_touched,
            });
        }
        Ok(Self {
            facts: raw.facts,
            evidence: raw.evidence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(verify_exit: Option<i32>, evaluator_touched: bool) -> LandingFacts {
        LandingFacts {
            slot_id: SlotId::new("slot-1").unwrap(),
            run_hash: "run-abc".into(),
            audit: AuditStatus::Attested,
            cpu_cap: "200000 100000".into(),
            blackwall_run: "bw-0123456789abcdef".into(),
            changeset_hash: "blake3-deadbeef".into(),
            base_sha: "056d7c7".into(),
            stale_base: false,
            verify_cmd: "make check JOBS=2 && make hygiene JOBS=2".into(),
            verify_exit,
            branch: Some("blackwall/bw-0123456789".into()),
            pr_number: Some(57),
            evaluator_touched,
            cost_source: CostSource::None,
            at: 1_000,
        }
    }

    #[test]
    fn landing_receipt_roundtrip() {
        let receipt = LandingReceipt::new(facts(Some(0), false));
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("\"evidence\":\"verified\""), "{json}");
        let back: LandingReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(back, receipt);
    }

    #[test]
    fn landing_receipt_tier_rule() {
        let cases = [
            (Some(0), false, EvidenceTier::Verified),
            (Some(0), true, EvidenceTier::Reported),
            (Some(2), false, EvidenceTier::Reported),
            (None, false, EvidenceTier::Reported),
        ];
        for (verify_exit, touched, want) in cases {
            let receipt = LandingReceipt::new(facts(verify_exit, touched));
            assert_eq!(
                receipt.evidence(),
                want,
                "exit {verify_exit:?}, touched {touched}"
            );
        }
    }

    #[test]
    fn landing_receipt_rejects_verified_with_nonzero_exit() {
        let receipt = LandingReceipt::new(facts(Some(2), false));
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("\"evidence\":\"reported\""), "{json}");
        let forged = json.replace("\"evidence\":\"reported\"", "\"evidence\":\"verified\"");
        assert!(serde_json::from_str::<LandingReceipt>(&forged).is_err());
    }

    #[test]
    fn landing_receipt_unattested_keeps_dropped_count() {
        let mut f = facts(Some(0), false);
        f.audit = AuditStatus::Unattested { dropped_events: 3 };
        let receipt = LandingReceipt::new(f);
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("\"dropped_events\":3"), "{json}");
        let back: LandingReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.facts().audit,
            AuditStatus::Unattested { dropped_events: 3 }
        );
    }
}
