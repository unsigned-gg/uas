// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Soma reflex policy tier classification and admission gates.
//!
//! Soma classifies incoming tasks into one of three policy tiers:
//! - [`PolicyTier::Tier0`]: Read-only, inspection, or dry-run. Zero side-effects allowed.
//! - [`PolicyTier::Tier1`]: Sandboxed builder execution in a Landlock jail and virtual worktree.
//! - [`PolicyTier::Tier2`]: Gated irreversible actions requiring operator approval before execution.

use serde::{Deserialize, Serialize};

use crate::DomainError;

/// The policy classification assigned to a task or session by Soma.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PolicyTier {
    /// Tier 0: Read-only, inspection, or dry-run.
    #[serde(rename = "tier-0")]
    Tier0,
    /// Tier 1: Compartmentalized, sandboxed execution.
    #[serde(rename = "tier-1")]
    Tier1,
    /// Tier 2: Irreversible or high blast-radius actions requiring operator gate.
    #[serde(rename = "tier-2")]
    Tier2,
}

impl PolicyTier {
    /// Returns true if this policy tier permits filesystem modifications.
    pub const fn can_write(&self) -> bool {
        match self {
            Self::Tier0 => false,
            Self::Tier1 | Self::Tier2 => true,
        }
    }

    /// Returns true if execution under this tier requires a sandbox jail.
    pub const fn requires_sandbox(&self) -> bool {
        match self {
            Self::Tier0 => false,
            Self::Tier1 => true,
            Self::Tier2 => false, // Operator-approved actions may touch host or cluster infra
        }
    }

    /// Returns true if this tier represents an irreversible action requiring operator signoff.
    pub const fn requires_operator_gate(&self) -> bool {
        match self {
            Self::Tier0 | Self::Tier1 => false,
            Self::Tier2 => true,
        }
    }

    /// String identifier for this policy tier.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Tier0 => "tier-0",
            Self::Tier1 => "tier-1",
            Self::Tier2 => "tier-2",
        }
    }
}

/// Soma task classification metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SomaClassification {
    /// Policy tier evaluated by the reflex gateway.
    pub tier: PolicyTier,
    /// Category or intent identified (e.g. "build", "inspect", "promote").
    pub intent: String,
    /// Rationale for the classification.
    pub rationale: String,
}

impl SomaClassification {
    /// Creates a new classification.
    pub fn new(
        tier: PolicyTier,
        intent: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let intent = intent.into();
        let rationale = rationale.into();
        if intent.trim().is_empty() {
            return Err(DomainError::Invalid {
                kind: "soma intent",
                value: intent,
                reason: "must not be empty",
            });
        }
        Ok(Self {
            tier,
            intent,
            rationale,
        })
    }

    /// Validates whether an action is permissible under this classification.
    pub fn check_admission(&self, is_write: bool, is_gated: bool) -> Result<(), DomainError> {
        if is_write && !self.tier.can_write() {
            return Err(DomainError::PolicyViolation {
                action: "write operation".into(),
                tier: self.tier.as_str().into(),
                reason: "writes forbidden under Tier 0 (read-only)",
            });
        }
        if is_gated && !self.tier.requires_operator_gate() {
            return Err(DomainError::PolicyViolation {
                action: "gated irreversible operation".into(),
                tier: self.tier.as_str().into(),
                reason: "irreversible actions require Tier 2 classification",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_tier_predicates() {
        assert!(!PolicyTier::Tier0.can_write());
        assert!(!PolicyTier::Tier0.requires_sandbox());
        assert!(!PolicyTier::Tier0.requires_operator_gate());

        assert!(PolicyTier::Tier1.can_write());
        assert!(PolicyTier::Tier1.requires_sandbox());
        assert!(!PolicyTier::Tier1.requires_operator_gate());

        assert!(PolicyTier::Tier2.can_write());
        assert!(!PolicyTier::Tier2.requires_sandbox());
        assert!(PolicyTier::Tier2.requires_operator_gate());
    }

    #[test]
    fn admission_checks() {
        let read_class =
            SomaClassification::new(PolicyTier::Tier0, "inspect", "read-only inspection").unwrap();
        assert!(read_class.check_admission(false, false).is_ok());
        assert!(read_class.check_admission(true, false).is_err());

        let build_class =
            SomaClassification::new(PolicyTier::Tier1, "build", "sandboxed compile").unwrap();
        assert!(build_class.check_admission(true, false).is_ok());
        assert!(build_class.check_admission(true, true).is_err());

        let release_class =
            SomaClassification::new(PolicyTier::Tier2, "promote", "release promotion").unwrap();
        assert!(release_class.check_admission(true, true).is_ok());
    }
}
