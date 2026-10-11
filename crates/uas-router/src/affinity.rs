// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Rendezvous (Highest Random Weight) replica choice (spec 0017 D3).
//!
//! For a prefix hash and a replica id, the weight is the first 8 bytes, big-endian, of
//! `SHA-256(replica_id bytes ++ prefix hash bytes)`. The highest weight wins. On equal
//! weight the smaller replica id wins. Only healthy replicas are candidates.
//!
//! When every replica is unhealthy, `choose` falls back to a deterministic round-robin
//! over all replicas (spec 0017 D5). The fallback picks the replica at index
//! `u64::from_be_bytes(prefix_hash[0..8]) % n` in ascending id order, and returns
//! [`ReplicaChoice::Fallback`] so the caller can log it.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::prefix::PrefixHash;

/// Errors from replica registration and choice.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RouterError {
    /// The table holds no replica. An all-unhealthy table falls back (D5).
    #[error("no replicas registered")]
    NoHealthyReplicas,
    /// A replica id was empty.
    #[error("invalid replica id: must not be empty")]
    InvalidReplicaId,
    /// A request body was not a usable chat-completions payload.
    #[error("malformed request payload: {0}")]
    InvalidPayload(String),
    /// The upstream replica did not answer before the deadline.
    #[error("upstream did not respond in time")]
    UpstreamTimeout,
    /// The upstream replica closed the stream before the completion ended.
    #[error("upstream closed the stream mid-completion")]
    StreamTerminated,
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

/// The outcome of `AffinityTable::choose`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplicaChoice {
    /// A healthy replica won the rendezvous ranking for the prefix hash.
    Affinity(ReplicaId),
    /// Every replica is unhealthy. The replica was picked by the D5 round-robin.
    Fallback(ReplicaId),
}

impl ReplicaChoice {
    /// The chosen replica, whichever rule picked it.
    pub fn replica(&self) -> &ReplicaId {
        match self {
            ReplicaChoice::Affinity(id) | ReplicaChoice::Fallback(id) => id,
        }
    }

    /// True when every replica was unhealthy and the D5 fallback picked this one.
    pub fn is_fallback(&self) -> bool {
        matches!(self, ReplicaChoice::Fallback(_))
    }
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
    ///
    /// When no replica is healthy, returns [`ReplicaChoice::Fallback`] with the replica at
    /// index `u64::from_be_bytes(hash[0..8]) % n` in ascending id order (spec 0017 D5).
    /// Returns `NoHealthyReplicas` only when the table holds no replica.
    pub fn choose(&self, hash: &PrefixHash) -> Result<ReplicaChoice, RouterError> {
        if let Some(id) = self.ranked(hash).into_iter().next() {
            return Ok(ReplicaChoice::Affinity(id));
        }
        let n = self.replicas.len();
        if n == 0 {
            return Err(RouterError::NoHealthyReplicas);
        }
        let mut first = [0u8; 8];
        first.copy_from_slice(&hash.0[..8]);
        let index = (u64::from_be_bytes(first) % n as u64) as usize;
        self.replicas
            .keys()
            .nth(index)
            .cloned()
            .map(ReplicaChoice::Fallback)
            .ok_or(RouterError::NoHealthyReplicas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefix::PrefixInput;

    #[test]
    fn router_error_display_names_each_variant() {
        assert_eq!(
            RouterError::NoHealthyReplicas.to_string(),
            "no replicas registered"
        );
        assert_eq!(
            RouterError::InvalidReplicaId.to_string(),
            "invalid replica id: must not be empty"
        );
        assert_eq!(
            RouterError::InvalidPayload("not json".into()).to_string(),
            "malformed request payload: not json"
        );
        assert_eq!(
            RouterError::UpstreamTimeout.to_string(),
            "upstream did not respond in time"
        );
        assert_eq!(
            RouterError::StreamTerminated.to_string(),
            "upstream closed the stream mid-completion"
        );
    }

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
            assert!(!first.is_fallback(), "a healthy table never falls back");
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
        assert_eq!(t.choose(&k), Ok(ReplicaChoice::Affinity(ranked[1].clone())));
        assert!(!t.ranked(&k).contains(&ranked[0]));
    }

    #[test]
    fn choose_errors_only_on_an_empty_table() {
        let k = keys(1)[0];
        assert_eq!(
            AffinityTable::new().choose(&k),
            Err(RouterError::NoHealthyReplicas)
        );
        let mut t = table(2);
        for i in 0..2 {
            t.set_healthy(&rid(i), false);
        }
        assert!(matches!(t.choose(&k), Ok(ReplicaChoice::Fallback(_))));
    }

    #[test]
    fn fallback_known_answer() {
        let k = PrefixInput {
            system: vec!["You are a builder.".to_string()],
            tools: vec![],
        }
        .hash();
        let mut four = table(4);
        let mut three = table(3);
        for i in 0..4 {
            four.set_healthy(&rid(i), false);
        }
        for i in 0..3 {
            three.set_healthy(&rid(i), false);
        }
        // 0x51e05212264af2b9 % 4 == 1 and % 3 == 2 (python3, 2026-10-10).
        assert_eq!(four.choose(&k), Ok(ReplicaChoice::Fallback(rid(1))));
        assert_eq!(three.choose(&k), Ok(ReplicaChoice::Fallback(rid(2))));
        // n = 7 pins the byte order: big-endian gives 2, little-endian would give 5
        // (python3, 2026-10-10). The n = 3 and n = 4 cases agree under both.
        let mut seven = table(7);
        for i in 0..7 {
            seven.set_healthy(&rid(i), false);
        }
        assert_eq!(seven.choose(&k), Ok(ReplicaChoice::Fallback(rid(2))));
    }

    #[test]
    fn fallback_is_deterministic_and_spreads() {
        let mut ascending = AffinityTable::new();
        let mut descending = AffinityTable::new();
        for i in 0..4 {
            ascending.insert(rid(i));
            ascending.set_healthy(&rid(i), false);
        }
        for i in (0..4).rev() {
            descending.insert(rid(i));
            descending.set_healthy(&rid(i), false);
        }
        let mut seen = [0usize; 4];
        for k in keys(1000) {
            let first = ascending.choose(&k).expect("non-empty table");
            assert!(first.is_fallback());
            assert_eq!(first, descending.choose(&k).expect("non-empty table"));
            let idx = (0..4)
                .find(|i| rid(*i) == *first.replica())
                .expect("a known replica");
            seen[idx] += 1;
        }
        assert!(
            seen.iter().all(|n| *n > 0),
            "control: every replica receives a key, got {seen:?}"
        );
    }

    #[test]
    fn fallback_ends_when_one_replica_recovers() {
        let mut t = table(4);
        for i in 0..4 {
            t.set_healthy(&rid(i), false);
        }
        t.set_healthy(&rid(2), true);
        for k in keys(200) {
            assert_eq!(t.choose(&k), Ok(ReplicaChoice::Affinity(rid(2))));
        }
    }

    #[test]
    fn removing_a_replica_moves_only_its_keys() {
        let mut t = table(4);
        let ks = keys(1000);
        let before: Vec<ReplicaId> = ks
            .iter()
            .map(|k| t.choose(k).expect("healthy").replica().clone())
            .collect();
        let gone = rid(2);
        t.remove(&gone);
        let mut moved = 0;
        for (k, b) in ks.iter().zip(before) {
            let after = t.choose(k).expect("healthy").replica().clone();
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
        let before: Vec<ReplicaId> = ks
            .iter()
            .map(|k| t.choose(k).expect("healthy").replica().clone())
            .collect();
        let added = rid(3);
        t.insert(added.clone());
        let mut moved = 0;
        for (k, b) in ks.iter().zip(before) {
            let after = t.choose(k).expect("healthy").replica().clone();
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
