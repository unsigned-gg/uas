//! Soma reflex policy gateway integration.
//!
//! Evaluates incoming activities against [`cas_domain::PolicyTier`] constraints
//! before execution in a substrate slot.

use cas_domain::{DomainError, PolicyTier, SomaClassification};
use serde::{Deserialize, Serialize};

use crate::{Activity, ActivityKind, EngineError};

/// Soma admission gate for enforcing policy rules on workflow activities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SomaAdmissionGate {
    classification: SomaClassification,
}

impl SomaAdmissionGate {
    /// Create a new admission gate with the evaluated Soma classification.
    pub fn new(classification: SomaClassification) -> Self {
        Self { classification }
    }

    /// Retrieve the underlying policy tier.
    pub fn tier(&self) -> PolicyTier {
        self.classification.tier
    }

    /// Retrieve the evaluated intent.
    pub fn intent(&self) -> &str {
        &self.classification.intent
    }

    /// Check if the activity is permitted to execute under the active policy tier.
    pub fn admit_activity(&self, activity: &Activity) -> Result<(), EngineError> {
        let is_write = activity.kind == ActivityKind::Impl;
        let is_gated = activity.kind == ActivityKind::Gate;

        self.classification
            .check_admission(is_write, is_gated)
            .map_err(|e| match e {
                DomainError::PolicyViolation {
                    action,
                    tier,
                    reason,
                } => EngineError::PolicyViolation(format!("{action} on {tier} ({reason})")),
                other => EngineError::Domain(other),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier0_permits_verify_rejects_impl() {
        let class = SomaClassification::new(PolicyTier::Tier0, "inspect", "read only").unwrap();
        let gate = SomaAdmissionGate::new(class);

        let verify_act = Activity::new("act-1", "make check", ActivityKind::Verify);
        assert!(gate.admit_activity(&verify_act).is_ok());

        let impl_act = Activity::new("act-2", "edit code", ActivityKind::Impl);
        assert!(gate.admit_activity(&impl_act).is_err());
    }

    #[test]
    fn tier1_permits_impl_rejects_unclassified_gate() {
        let class = SomaClassification::new(PolicyTier::Tier1, "build", "sandboxed build").unwrap();
        let gate = SomaAdmissionGate::new(class);

        let impl_act = Activity::new("act-1", "edit code", ActivityKind::Impl);
        assert!(gate.admit_activity(&impl_act).is_ok());

        let gate_act = Activity::new("act-2", "operator signoff", ActivityKind::Gate);
        assert!(gate.admit_activity(&gate_act).is_err());
    }

    #[test]
    fn tier2_permits_gate_and_impl() {
        let class =
            SomaClassification::new(PolicyTier::Tier2, "promote", "release promotion").unwrap();
        let gate = SomaAdmissionGate::new(class);

        let gate_act = Activity::new("act-1", "operator signoff", ActivityKind::Gate);
        assert!(gate.admit_activity(&gate_act).is_ok());
    }
}
