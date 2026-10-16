// SPDX-License-Identifier: Apache-2.0 OR MIT
use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::{DomainError, Lease, PrincipalRef};

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

impl Attestation {
    /// Verify that the attestation `signer` is a WIMSE (`draft-klrc-aiagent-auth-03`)
    /// agent SPIFFE ID (`spiffe://<trust_domain>/agent/<team>/<agent_id>`) in
    /// `expected_trust_domain`, returning `(team, agent_id)`.
    pub fn verify_wimse_agent_signer(
        &self,
        expected_trust_domain: &str,
    ) -> Result<(&str, &str), DomainError> {
        self.signer.parse_wimse_agent(expected_trust_domain)
    }

    /// Verify that the attestation `signer` is a WIMSE (`draft-klrc-aiagent-auth-03`)
    /// Conclave Elf SPIFFE ID (`spiffe://<trust_domain>/conclave/<mesh>/elf/<elf_id>`) in
    /// `expected_trust_domain`, returning `(mesh, elf_id)`.
    pub fn verify_wimse_conclave_elf_signer(
        &self,
        expected_trust_domain: &str,
    ) -> Result<(&str, &str), DomainError> {
        self.signer.parse_wimse_conclave_elf(expected_trust_domain)
    }

    /// Verify that this attestation is bound to `lease` (signed by `lease.holder` or an
    /// authorized upstream actor in `lease.delegation()`) and, when `required_delegator` is
    /// provided, that `required_delegator` appears in the lease's RFC 8693 `act` chain.
    pub fn verify_lease_delegation(
        &self,
        lease: &Lease,
        required_delegator: Option<&PrincipalRef>,
    ) -> Result<(), DomainError> {
        if self.signer != lease.holder && !lease.delegated_by(&self.signer) {
            return Err(DomainError::Invalid {
                kind: "attestation signer",
                value: self.signer.to_string(),
                reason: "signer does not match lease holder or delegation chain",
            });
        }
        if let Some(required) = required_delegator {
            if !lease.delegated_by(required) {
                return Err(DomainError::Invalid {
                    kind: "attestation delegation",
                    value: required.to_string(),
                    reason: "required delegator missing from lease act chain",
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Budget, LeaseId, LeaseToken, SlotId};
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

    #[test]
    fn attestation_verifies_wimse_signer_and_rfc8693_lease_delegation() {
        let agent = PrincipalRef::wimse_agent("unsigned.gg", "rd", "builder-01").unwrap();
        let coordinator =
            PrincipalRef::wimse_agent("unsigned.gg", "estate", "coordinator-01").unwrap();
        let operator = PrincipalRef::new("user:ctodie").unwrap();
        let outsider = PrincipalRef::wimse_agent("unsigned.gg", "rd", "rogue-99").unwrap();

        let lease = Lease::new(
            LeaseId::new("lease-1").unwrap(),
            SlotId::new("slot-1").unwrap(),
            agent.clone(),
            LeaseToken::new("s3cr3t-token-value").unwrap(),
            100,
            200,
            Budget::new(1000, 60).unwrap(),
        )
        .unwrap()
        .with_delegation(vec![coordinator.clone(), operator.clone()])
        .unwrap();

        let att = Attestation {
            signer: agent,
            evidence: Verified,
            receipt: "sha256:deadbeef".into(),
            at: 120,
        };
        assert_eq!(
            att.verify_wimse_agent_signer("unsigned.gg").unwrap(),
            ("rd", "builder-01")
        );
        assert!(att.verify_lease_delegation(&lease, Some(&operator)).is_ok());
        assert!(att
            .verify_lease_delegation(&lease, Some(&outsider))
            .is_err());

        let rogue_att = Attestation {
            signer: outsider,
            evidence: Verified,
            receipt: "sha256:deadbeef".into(),
            at: 120,
        };
        assert!(rogue_att.verify_lease_delegation(&lease, None).is_err());
    }
}
