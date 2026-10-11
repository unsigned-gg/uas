// SPDX-License-Identifier: Apache-2.0 OR MIT
use serde::{Deserialize, Serialize};

use crate::{Attestation, DomainError};

/// A unit of data that moves between components, with its provenance.
///
/// Deserializing runs the same checks as [`Envelope::new`]. Building the
/// struct directly from its public fields does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "EnvelopeRepr")]
pub struct Envelope {
    /// Unique id, 1 to 64 chars of `a-z`, `0-9` and `-`, starting with a letter.
    pub id: String,
    /// What the payload is, for example `task.result`.
    pub kind: String,
    /// The data. The domain does not interpret it.
    pub payload: Vec<u8>,
    /// Who vouches for the data, and how well.
    pub provenance: Attestation,
}

/// The wire shape of an [`Envelope`], checked by [`Envelope::new`].
#[derive(Deserialize)]
struct EnvelopeRepr {
    id: String,
    kind: String,
    payload: Vec<u8>,
    provenance: Attestation,
}

impl TryFrom<EnvelopeRepr> for Envelope {
    type Error = DomainError;

    fn try_from(r: EnvelopeRepr) -> Result<Self, Self::Error> {
        Self::new(r.id, r.kind, r.payload, r.provenance)
    }
}

impl Envelope {
    /// Build an envelope. Rejects an invalid id or an empty kind.
    pub fn new(
        id: impl Into<String>,
        kind: impl Into<String>,
        payload: Vec<u8>,
        provenance: Attestation,
    ) -> Result<Self, DomainError> {
        let id = id.into();
        crate::ident::check("envelope id", &id, 64, "")?;
        let kind = kind.into();
        if kind.is_empty() {
            return Err(DomainError::Invalid {
                kind: "envelope kind",
                value: kind,
                reason: "must not be empty",
            });
        }
        Ok(Self {
            id,
            kind,
            payload,
            provenance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EvidenceTier, PrincipalRef};

    fn att() -> Attestation {
        Attestation {
            signer: PrincipalRef::new("agent-1").unwrap(),
            evidence: EvidenceTier::Verified,
            receipt: "r".into(),
            at: 1,
        }
    }

    #[test]
    fn builds_and_round_trips() {
        let e = Envelope::new("env-1", "task.result", vec![1, 2, 3], att()).unwrap();
        assert_eq!(e.payload, [1, 2, 3]);
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<Envelope>(&json).unwrap(), e);
    }

    #[test]
    fn rejects_bad_id_and_empty_kind() {
        assert!(Envelope::new("Bad Id", "k", vec![], att()).is_err());
        assert!(Envelope::new("env-1", "", vec![], att()).is_err());
    }

    #[test]
    fn serde_rejects_a_bad_id_or_an_empty_kind() {
        let e = Envelope::new("env-1", "task.result", vec![], att()).unwrap();
        let good = serde_json::to_value(&e).unwrap();
        for (field, value) in [
            ("id", serde_json::json!("Bad Id")),
            ("kind", serde_json::json!("")),
        ] {
            let mut v = good.clone();
            v[field] = value;
            assert!(
                serde_json::from_value::<Envelope>(v).is_err(),
                "accepted bad {field}"
            );
        }
        assert_eq!(serde_json::from_value::<Envelope>(good).unwrap(), e);
    }
}
