//! Rendezvous (Highest Random Weight) replica choice (spec 0017 D3).
//!
//! For a prefix hash and a replica id, the weight is the first 8 bytes, big-endian, of
//! `SHA-256(replica_id bytes ++ prefix hash bytes)`. The highest weight wins. On equal
//! weight the smaller replica id wins. Only healthy replicas are candidates.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::prefix::PrefixHash;

/// Errors from replica registration and choice.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RouterError {
    /// No replica in the table is healthy.
    #[error("no healthy replicas")]
    NoHealthyReplicas,
    /// A replica id was empty.
    #[error("invalid replica id: must not be empty")]
    InvalidReplicaId,
}

/// A replica identifier. Never empty.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplicaId(String);

impl ReplicaId {
    /// Build a replica id. Rejects the empty string.
    pub fn new(id: impl Into<String>) -> Result<Self, RouterError> {
        let id: String = id.into();
        if id.is_empty() {
            Err(RouterError::InvalidReplicaId)
        } else {
            Ok(Self(id))
        }
    }

    /// The id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// HRW weight: the first 8 bytes, big-endian, of `SHA-256(replica id bytes ++ prefix hash)`.
fn weight(id: &ReplicaId, hash: &PrefixHash) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(id.0.as_bytes());
    hasher.update(hash.0);
    let digest = hasher.finalize();
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(first)
}

/// Ranking order: higher weight first; on equal weight, the smaller replica id first.
fn by_weight_then_id(a: &(u64, &ReplicaId), b: &(u64, &ReplicaId)) -> Ordering {
    b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1))
}

/// The set of replicas and their health flags.
#[derive(Debug, Clone, Default)]
pub struct AffinityTable {
    replicas: BTreeMap<ReplicaId, bool>,
}

impl AffinityTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a replica, healthy. An existing entry is replaced.
    pub fn insert(&mut self, id: ReplicaId) {
        self.replicas.insert(id, true);
    }

    /// Remove a replica. Removing an unknown id does nothing.
    pub fn remove(&mut self, id: &ReplicaId) {
        self.replicas.remove(id);
    }

    /// Set a replica's health. An unknown id does nothing.
    pub fn set_healthy(&mut self, id: &ReplicaId, healthy: bool) {
        if let Some(flag) = self.replicas.get_mut(id) {
            *flag = healthy;
        }
    }

    /// Healthy replicas, highest weight first, for this prefix hash.
    pub fn ranked(&self, hash: &PrefixHash) -> Vec<ReplicaId> {
        let mut scored: Vec<(u64, &ReplicaId)> = self
            .replicas
            .iter()
            .filter(|(_, healthy)| **healthy)
            .map(|(id, _)| (weight(id, hash), id))
            .collect();
        scored.sort_by(by_weight_then_id);
        scored.into_iter().map(|(_, id)| id.clone()).collect()
    }

    /// The healthy replica with the highest weight for this prefix hash.
    pub fn choose(&self, hash: &PrefixHash) -> Result<ReplicaId, RouterError> {
        self.ranked(hash)
            .into_iter()
            .next()
            .ok_or(RouterError::NoHealthyReplicas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefix::PrefixInput;

    fn rid(i: usize) -> ReplicaId {
        ReplicaId::new(format!("replica-{i}")).expect("non-empty")
    }

    fn table(n: usize) -> AffinityTable {
        let mut t = AffinityTable::new();
        for i in 0..n {
            t.insert(rid(i));
        }
        t
    }

    fn keys(n: usize) -> Vec<PrefixHash> {
        (0..n)
            .map(|i| {
                PrefixInput {
                    system: vec![format!("sys-{i}")],
                    tools: vec![],
                }
                .hash()
            })
            .collect()
    }

    #[test]
    fn replica_id_rejects_empty() {
        assert_eq!(ReplicaId::new(""), Err(RouterError::InvalidReplicaId));
        assert!(ReplicaId::new("a").is_ok());
    }

    #[test]
    fn choose_is_deterministic() {
        let ascending = table(4);
        let mut descending = AffinityTable::new();
        for i in (0..4).rev() {
            descending.insert(rid(i));
        }
        for k in keys(200) {
            let first = ascending.choose(&k).expect("healthy table");
            assert_eq!(first, ascending.choose(&k).expect("healthy table"));
            assert_eq!(first, descending.choose(&k).expect("healthy table"));
        }
    }

    #[test]
    fn choose_skips_unhealthy_replica() {
        let mut t = table(4);
        let k = keys(1)[0];
        let ranked = t.ranked(&k);
        assert_eq!(ranked.len(), 4);
        t.set_healthy(&ranked[0], false);
        assert_eq!(t.choose(&k), Ok(ranked[1].clone()));
        assert!(!t.ranked(&k).contains(&ranked[0]));
    }

    #[test]
    fn choose_errors_when_no_healthy_replica() {
        let k = keys(1)[0];
        assert_eq!(
            AffinityTable::new().choose(&k),
            Err(RouterError::NoHealthyReplicas)
        );
        let mut t = table(2);
        for i in 0..2 {
            t.set_healthy(&rid(i), false);
        }
        assert_eq!(t.choose(&k), Err(RouterError::NoHealthyReplicas));
    }

    #[test]
    fn removing_a_replica_moves_only_its_keys() {
        let mut t = table(4);
        let ks = keys(1000);
        let before: Vec<ReplicaId> = ks.iter().map(|k| t.choose(k).expect("healthy")).collect();
        let gone = rid(2);
        t.remove(&gone);
        let mut moved = 0;
        for (k, b) in ks.iter().zip(before) {
            let after = t.choose(k).expect("healthy");
            if b == gone {
                moved += 1;
                assert_ne!(after, gone);
            } else {
                assert_eq!(after, b);
            }
        }
        assert!(
            moved > 0,
            "control: some keys must have been on the removed replica"
        );
    }

    #[test]
    fn adding_a_replica_moves_keys_only_to_it() {
        let mut t = table(3);
        let ks = keys(1000);
        let before: Vec<ReplicaId> = ks.iter().map(|k| t.choose(k).expect("healthy")).collect();
        let added = rid(3);
        t.insert(added.clone());
        let mut moved = 0;
        for (k, b) in ks.iter().zip(before) {
            let after = t.choose(k).expect("healthy");
            if after != b {
                moved += 1;
                assert_eq!(after, added);
            }
        }
        assert!(moved > 0, "control: the new replica must take some keys");
    }

    #[test]
    fn ranking_matches_independent_known_answer() {
        // Oracle: Python hashlib. Weight = first 8 bytes big-endian of sha256(id ++ prefix16),
        // prefix from sha256("SYS:18:You are a builder.|TOOLS:") after the 2026-10-10 D1 amendment.
        // replica-0 17628686350455170347 > replica-3 9131027170909936301
        //   > replica-1 5087872496897775711 > replica-2 2118516450390433603
        let k = PrefixInput {
            system: vec!["You are a builder.".to_string()],
            tools: vec![],
        }
        .hash();
        let hex: String = k.0.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "51e05212264af2b92ebb193e57101068");
        let order: Vec<String> = table(4)
            .ranked(&k)
            .iter()
            .map(|r| r.as_str().to_string())
            .collect();
        assert_eq!(order, ["replica-0", "replica-3", "replica-1", "replica-2"]);
    }

    #[test]
    fn ranking_breaks_equal_weight_by_smaller_id() {
        let (r0, r1, r9) = (rid(0), rid(1), rid(9));
        let mut scored = [(5u64, &r1), (7u64, &r9), (5u64, &r0)];
        scored.sort_by(by_weight_then_id);
        let order: Vec<&str> = scored.iter().map(|(_, id)| id.as_str()).collect();
        assert_eq!(order, ["replica-9", "replica-0", "replica-1"]);
    }
}
