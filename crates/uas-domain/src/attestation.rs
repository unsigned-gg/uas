// SPDX-License-Identifier: Apache-2.0 OR MIT
use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::PrincipalRef;

/// How strong the evidence behind a claim is. `Verified` is the strongest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTier {
    /// Checked by an independent mechanism.
    Verified,
    /// Stated by the system that did the work, with a receipt.
    Reported,
    /// Inferred from signals.
    Heuristic,
    /// Claimed with no support.
    Asserted,
}

impl EvidenceTier {
    /// True only for [`EvidenceTier::Verified`].
    pub fn satisfies_gate(self) -> bool {
        self == Self::Verified
    }

    /// String identifier for this evidence tier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Reported => "reported",
            Self::Heuristic => "heuristic",
            Self::Asserted => "asserted",
        }
    }

    /// Strength rank: higher is stronger.
    fn rank(self) -> u8 {
        match self {
            Self::Verified => 3,
            Self::Reported => 2,
            Self::Heuristic => 1,
            Self::Asserted => 0,
        }
    }
}

impl PartialOrd for EvidenceTier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EvidenceTier {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank().cmp(&other.rank())
    }
}

/// A signed statement about where a piece of data came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    /// Who made the statement.
    pub signer: PrincipalRef,
    /// How strong the evidence is.
    pub evidence: EvidenceTier,
    /// Pointer to the proof, for example a command, digest or log reference.
    pub receipt: String,
    /// When the statement was made, in epoch seconds.
    pub at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use EvidenceTier::{Asserted, Heuristic, Reported, Verified};

    #[test]
    fn verified_is_strongest_in_a_total_order() {
        assert!(Verified > Reported);
        assert!(Reported > Heuristic);
        assert!(Heuristic > Asserted);
        let mut all = vec![Heuristic, Verified, Asserted, Reported];
        all.sort();
        assert_eq!(all, [Asserted, Heuristic, Reported, Verified]);
        for a in all.iter() {
            for b in all.iter() {
                assert_eq!(a.cmp(b), b.cmp(a).reverse());
            }
        }
    }

    #[test]
    fn only_verified_satisfies_the_gate() {
        assert!(Verified.satisfies_gate());
        assert!(!Reported.satisfies_gate());
        assert!(!Heuristic.satisfies_gate());
        assert!(!Asserted.satisfies_gate());
    }

    #[test]
    fn attestation_round_trips() {
        let a = Attestation {
            signer: PrincipalRef::new("spiffe://uas/agent-1").unwrap(),
            evidence: Reported,
            receipt: "cargo test: 12 passed".into(),
            at: 100,
        };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"reported\""));
        assert_eq!(serde_json::from_str::<Attestation>(&json).unwrap(), a);
    }
}
