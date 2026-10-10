//! Landing receipts for changesets that Blackwall reconciled and opened as pull requests.
//!
//! A receipt records the custody facts of one landing and the evidence tier
//! those facts support. The tier rule lives here: `verified` only when the
//! verify command exited zero and no evaluator file changed, unless a human
//! raise lifts the evaluator cap (spec 0022 D2 and D10, spec 0023 D3). The
//! subprocess adapter lives in `cas-engine`.

use serde::{Deserialize, Serialize};

use crate::{EvidenceTier, PrincipalRef, SlotId};

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
    /// No slot audit ran. A wave Builder has no slot (spec 0025 D9).
    NotAudited,
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
    /// The development head that the base was checked against. `Some` iff `stale_base`
    /// (spec 0022 D9). Absent on receipts written before D9.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_head: Option<String>,
    /// The verify command from the repository's `blackwall.toml`. `None` when the file sets
    /// none. Without one, the tier cannot be `verified`.
    pub verify_cmd: Option<String>,
    /// The exit code of the `blackwall reconcile` process. It is not the verify command's own
    /// exit. Reported by the soma lane (spec 0022 F14, `reported`, not yet verified here):
    /// blackwall at `ec076f6` exits 3 when the verify gate fails, 1 on another error and 2 on
    /// a usage error. When reconcile ran, a receipt records only 0 or 3: the other codes are a
    /// `LandingError` with no receipt (spec 0022 D7). `None` when reconcile did not run.
    pub verify_exit: Option<i32>,
    /// The landing branch, `blackwall/<short run>`, when one was created.
    pub branch: Option<String>,
    /// The pull request number, when one was opened.
    pub pr_number: Option<u64>,
    /// The merge commit of the pull request. Spec 0025 D15: set only from a `gh pr view`
    /// pre-read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_commit: Option<String>,
    /// True when the changeset touched an evaluator file (spec 0023 D3).
    pub evaluator_touched: bool,
    /// Where the cost figures came from (spec 0023 D2).
    pub cost_source: CostSource,
    /// When the landing was recorded, in epoch seconds.
    pub at: u64,
}

/// A human raise of a landing's evidence tier above its computed tier (spec 0022 D10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanRaise {
    /// The person who raised the tier.
    pub by: PrincipalRef,
    /// When the raise was made, in epoch seconds.
    pub at: u64,
}

/// A landing receipt whose evidence tier follows the tier rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawLandingReceipt")]
pub struct LandingReceipt {
    #[serde(flatten)]
    facts: LandingFacts,
    evidence: EvidenceTier,
    /// The human raise that lifts the evaluator cap, when one was made (spec 0022 D10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    human_raise: Option<HumanRaise>,
}

/// A rejected landing receipt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LandingReceiptError {
    /// The evidence tier is above the computed tier and no human raise names it.
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
    /// The human raise names a tier above the tier the verify gate allows.
    #[error(
        "evidence {evidence:?} is above the ceiling {ceiling:?} that verify exit \
         {verify_exit:?} allows, even with a human raise"
    )]
    RaiseAboveCeiling {
        /// The tier the receipt claims.
        evidence: EvidenceTier,
        /// The highest tier the verify gate allows.
        ceiling: EvidenceTier,
        /// The verify exit code the receipt records.
        verify_exit: Option<i32>,
    },
    /// `stale_base` and `development_head` disagree. The head is recorded only for a stale base.
    #[error("stale_base {stale_base} does not match development_head {development_head:?}")]
    StaleBaseHead {
        /// The stale flag the receipt records.
        stale_base: bool,
        /// The development head the receipt records.
        development_head: Option<String>,
    },
}

/// The wire form that a receipt must pass before it is accepted.
#[derive(Deserialize)]
struct RawLandingReceipt {
    #[serde(flatten)]
    facts: LandingFacts,
    evidence: EvidenceTier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    human_raise: Option<HumanRaise>,
}

impl LandingReceipt {
    /// Build a receipt. The evidence tier follows from the facts and no human raise applies.
    pub fn new(facts: LandingFacts) -> Result<Self, LandingReceiptError> {
        let evidence = Self::evidence_for(
            facts.verify_cmd.as_deref(),
            facts.verify_exit,
            facts.evaluator_touched,
        );
        Self::with_evidence(facts, evidence, None)
    }

    /// Build a receipt with a stored tier and an optional human raise. The same check runs
    /// as when a receipt is read back, so a receipt that cannot be read back cannot be built.
    pub fn with_evidence(
        facts: LandingFacts,
        evidence: EvidenceTier,
        human_raise: Option<HumanRaise>,
    ) -> Result<Self, LandingReceiptError> {
        check(&facts, evidence, human_raise.as_ref())?;
        Ok(Self {
            facts,
            evidence,
            human_raise,
        })
    }

    /// The tier rule: `verified` iff a verify command is set, reconcile exited zero and no
    /// evaluator file changed. Otherwise `reported`.
    pub fn evidence_for(
        verify_cmd: Option<&str>,
        verify_exit: Option<i32>,
        evaluator_touched: bool,
    ) -> EvidenceTier {
        if verify_cmd.is_some() && verify_exit == Some(0) && !evaluator_touched {
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

    /// The same receipt with the merge commit set. The tier and the human raise stay.
    pub fn with_merge_commit(mut self, oid: String) -> Self {
        self.facts.merge_commit = Some(oid);
        self
    }

    /// The human raise of this landing, when one was made.
    pub fn human_raise(&self) -> Option<&HumanRaise> {
        self.human_raise.as_ref()
    }
}

impl TryFrom<RawLandingReceipt> for LandingReceipt {
    type Error = LandingReceiptError;

    fn try_from(raw: RawLandingReceipt) -> Result<Self, Self::Error> {
        check(&raw.facts, raw.evidence, raw.human_raise.as_ref())?;
        Ok(Self {
            facts: raw.facts,
            evidence: raw.evidence,
            human_raise: raw.human_raise,
        })
    }
}

/// The rank of a tier. Higher is stronger: `verified` > `reported` > `heuristic` > `asserted`.
fn rank(tier: EvidenceTier) -> u8 {
    match tier {
        EvidenceTier::Verified => 3,
        EvidenceTier::Reported => 2,
        EvidenceTier::Heuristic => 1,
        EvidenceTier::Asserted => 0,
    }
}

/// The whole receipt rule (spec 0022 D2, D9, D10).
///
/// The stale-base head must agree with the stale flag. The stored tier must not exceed the
/// computed tier. Above the computed tier, only a human raise is allowed, and the raise lifts
/// the evaluator cap only: the tier must stay within the ceiling that ignores the evaluator.
fn check(
    facts: &LandingFacts,
    evidence: EvidenceTier,
    human_raise: Option<&HumanRaise>,
) -> Result<(), LandingReceiptError> {
    if facts.stale_base != facts.development_head.is_some() {
        return Err(LandingReceiptError::StaleBaseHead {
            stale_base: facts.stale_base,
            development_head: facts.development_head.clone(),
        });
    }
    let computed = LandingReceipt::evidence_for(
        facts.verify_cmd.as_deref(),
        facts.verify_exit,
        facts.evaluator_touched,
    );
    if rank(evidence) <= rank(computed) {
        return Ok(());
    }
    if human_raise.is_none() {
        return Err(LandingReceiptError::EvidenceMismatch {
            evidence,
            expected: computed,
            verify_exit: facts.verify_exit,
            evaluator_touched: facts.evaluator_touched,
        });
    }
    let ceiling =
        LandingReceipt::evidence_for(facts.verify_cmd.as_deref(), facts.verify_exit, false);
    if rank(evidence) > rank(ceiling) {
        return Err(LandingReceiptError::RaiseAboveCeiling {
            evidence,
            ceiling,
            verify_exit: facts.verify_exit,
        });
    }
    Ok(())
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
            development_head: None,
            verify_cmd: Some("make check JOBS=2 && make hygiene JOBS=2".into()),
            verify_exit,
            branch: Some("blackwall/bw-0123456789".into()),
            pr_number: Some(57),
            merge_commit: None,
            evaluator_touched,
            cost_source: CostSource::None,
            at: 1_000,
        }
    }

    #[test]
    fn landing_receipt_roundtrip() {
        let receipt = LandingReceipt::new(facts(Some(0), false)).unwrap();
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("\"evidence\":\"verified\""), "{json}");
        let back: LandingReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(back, receipt);
    }

    #[test]
    fn landing_receipt_tier_rule() {
        let cmd = Some("make check JOBS=2");
        let cases = [
            (cmd, Some(0), false, EvidenceTier::Verified),
            (cmd, Some(0), true, EvidenceTier::Reported),
            (cmd, Some(1), false, EvidenceTier::Reported),
            (None, Some(0), false, EvidenceTier::Reported),
        ];
        for (verify_cmd, verify_exit, touched, want) in cases {
            let mut f = facts(verify_exit, touched);
            f.verify_cmd = verify_cmd.map(str::to_owned);
            let receipt = LandingReceipt::new(f).unwrap();
            assert_eq!(
                receipt.evidence(),
                want,
                "cmd {verify_cmd:?}, exit {verify_exit:?}, touched {touched}"
            );
        }
    }

    #[test]
    fn landing_receipt_rejects_verified_with_nonzero_exit() {
        let receipt = LandingReceipt::new(facts(Some(1), false)).unwrap();
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("\"evidence\":\"reported\""), "{json}");
        let forged = json.replace("\"evidence\":\"reported\"", "\"evidence\":\"verified\"");
        assert!(serde_json::from_str::<LandingReceipt>(&forged).is_err());
    }

    #[test]
    fn landing_receipt_rejects_forged_raise_on_the_wire() {
        // Control: a valid raise over the evaluator cap serializes and parses.
        let valid = LandingReceipt::with_evidence(
            facts(Some(0), true),
            EvidenceTier::Verified,
            Some(raise("ctodie@cerebral.work")),
        )
        .unwrap();
        let json = serde_json::to_string(&valid).unwrap();
        assert!(json.contains("\"human_raise\""), "{json}");
        assert!(serde_json::from_str::<LandingReceipt>(&json).is_ok());
        // Forgery: the same document with the verify exit flipped to 1. A raise
        // lifts only the evaluator cap, so `try_from` must refuse `verified`.
        assert!(json.contains("\"verify_exit\":0"), "{json}");
        let forged = json.replace("\"verify_exit\":0", "\"verify_exit\":1");
        assert!(
            serde_json::from_str::<LandingReceipt>(&forged).is_err(),
            "forged raise parsed: {forged}"
        );
    }

    #[test]
    fn landing_receipt_unattested_keeps_dropped_count() {
        let mut f = facts(Some(0), false);
        f.audit = AuditStatus::Unattested { dropped_events: 3 };
        let receipt = LandingReceipt::new(f).unwrap();
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("\"dropped_events\":3"), "{json}");
        let back: LandingReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.facts().audit,
            AuditStatus::Unattested { dropped_events: 3 }
        );
    }

    #[test]
    fn landing_receipt_not_audited_roundtrip() {
        // A wave run has no slot, so no audit ran at all (spec 0025 D9).
        let mut f = facts(Some(0), false);
        f.audit = AuditStatus::NotAudited;
        f.run_hash = String::new();
        let receipt = LandingReceipt::new(f).unwrap();
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(
            json.contains("\"audit\":{\"status\":\"not_audited\"}"),
            "{json}"
        );
        let back: LandingReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(back.facts().audit, AuditStatus::NotAudited);
    }

    #[test]
    fn landing_receipt_merge_commit_absent_is_none() {
        let receipt = LandingReceipt::new(facts(Some(0), false)).unwrap();
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(
            !json.contains("merge_commit"),
            "None is not serialized: {json}"
        );
        let back: LandingReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(back.facts().merge_commit, None, "absent is None");
        let oid = "4e7c000000000000000000000000000000000000".to_owned();
        let raised = LandingReceipt::with_evidence(
            facts(Some(0), true),
            EvidenceTier::Verified,
            Some(raise("alice")),
        )
        .unwrap();
        for r in [receipt, raised] {
            let (tier, human) = (r.evidence(), r.human_raise().cloned());
            let merged = r.with_merge_commit(oid.clone());
            assert_eq!(merged.facts().merge_commit.as_deref(), Some(oid.as_str()));
            assert_eq!(
                (merged.evidence(), merged.human_raise().cloned()),
                (tier, human)
            );
            let json = serde_json::to_string(&merged).unwrap();
            let back: LandingReceipt = serde_json::from_str(&json).unwrap();
            assert_eq!(back, merged, "Some round-trips");
        }
    }

    fn raise(who: &str) -> HumanRaise {
        HumanRaise {
            by: PrincipalRef::new(who).unwrap(),
            at: 2_000,
        }
    }

    #[test]
    fn landing_receipt_rejects_raise_without_human() {
        // Exit 0 and a verify command, but the evaluator changed: the computed tier is reported.
        let f = facts(Some(0), true);
        let got = LandingReceipt::with_evidence(f, EvidenceTier::Verified, None);
        assert!(
            matches!(
                got,
                Err(LandingReceiptError::EvidenceMismatch {
                    evidence: EvidenceTier::Verified,
                    expected: EvidenceTier::Reported,
                    ..
                })
            ),
            "{got:?}"
        );
    }

    #[test]
    fn landing_receipt_rejects_raise_to_verified_with_nonzero_exit() {
        // The human raise lifts the evaluator cap only. Exit 1 caps the tier at reported.
        let f = facts(Some(1), false);
        let got = LandingReceipt::with_evidence(
            f,
            EvidenceTier::Verified,
            Some(raise("ctodie@cerebral.work")),
        );
        assert!(
            matches!(
                got,
                Err(LandingReceiptError::RaiseAboveCeiling {
                    evidence: EvidenceTier::Verified,
                    ceiling: EvidenceTier::Reported,
                    ..
                })
            ),
            "{got:?}"
        );
    }

    #[test]
    fn landing_receipt_accepts_human_raise_over_evaluator_cap() {
        let f = facts(Some(0), true);
        let receipt = LandingReceipt::with_evidence(
            f,
            EvidenceTier::Verified,
            Some(raise("ctodie@cerebral.work")),
        )
        .unwrap();
        assert_eq!(receipt.evidence(), EvidenceTier::Verified);
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("\"human_raise\""), "{json}");
        let back: LandingReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(back, receipt);
        let by = back.human_raise().map(|r| r.by.as_str().to_owned());
        assert_eq!(by.as_deref(), Some("ctodie@cerebral.work"));
    }

    #[test]
    fn landing_receipt_accepts_tier_below_computed() {
        let receipt =
            LandingReceipt::with_evidence(facts(Some(0), false), EvidenceTier::Reported, None)
                .unwrap();
        assert_eq!(receipt.evidence(), EvidenceTier::Reported);
        assert_eq!(receipt.human_raise(), None);
    }

    #[test]
    fn landing_receipt_stale_base_records_development_head() {
        let mut stale = facts(Some(0), false);
        stale.stale_base = true;
        stale.development_head = Some("ffeedd0011".into());
        let receipt = LandingReceipt::new(stale.clone()).unwrap();
        let json = serde_json::to_string(&receipt).unwrap();
        let back: LandingReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(back, receipt);
        assert_eq!(back.facts().development_head.as_deref(), Some("ffeedd0011"));

        let mut no_head = stale.clone();
        no_head.development_head = None;
        assert!(matches!(
            LandingReceipt::new(no_head),
            Err(LandingReceiptError::StaleBaseHead { .. })
        ));

        let mut fresh_with_head = facts(Some(0), false);
        fresh_with_head.development_head = Some("ffeedd0011".into());
        assert!(matches!(
            LandingReceipt::new(fresh_with_head),
            Err(LandingReceiptError::StaleBaseHead { .. })
        ));
    }

    #[test]
    fn landing_receipt_wire_omits_absent_fields() {
        let receipt = LandingReceipt::new(facts(Some(0), false)).unwrap();
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(!json.contains("development_head"), "{json}");
        assert!(!json.contains("human_raise"), "{json}");
        let back: LandingReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(back, receipt);
    }
}
