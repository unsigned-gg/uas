// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Reverie memory daemon client integration.
//!
//! Reverie runs on `http://127.0.0.1:7437` and provides persistent episodic and
//! semantic memory for estate agents.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use uas_domain::Attestation;

use crate::EngineError;

#[cfg(feature = "reverie-http")]
pub mod http;
#[cfg(feature = "reverie-http")]
pub mod outbox;

/// A structured memory entry to persist in Reverie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReverieMemoryRecord {
    /// Associated project key (e.g. "uas", "cortex").
    pub project: String,
    /// Associated session identifier.
    pub session_id: String,
    /// Topic key for grouping (e.g. "uas/harness/session").
    pub topic_key: String,
    /// Summary narrative of the execution.
    pub summary: String,
    /// Cryptographic attestation and evidence.
    pub attestation: Attestation,
}

/// Request to search context memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReverieContextQuery {
    /// Semantic search query string.
    pub query: String,
    /// Optional project scope filter.
    pub project: Option<String>,
    /// Result count limit.
    pub limit: usize,
}

/// Client port for interacting with the Reverie memory daemon.
pub trait ReverieClient: Send + Sync {
    /// Record an attestation and memory record.
    fn record(&self, entry: ReverieMemoryRecord) -> Result<(), EngineError>;

    /// Search prior memory context.
    fn search(&self, query: &ReverieContextQuery) -> Result<Vec<ReverieMemoryRecord>, EngineError>;
}

/// In-memory client implementation for hermetic testing.
#[derive(Debug, Default, Clone)]
pub struct InMemoryReverieClient {
    records: Arc<Mutex<Vec<ReverieMemoryRecord>>>,
}

impl InMemoryReverieClient {
    /// Create a new in-memory client.
    pub fn new() -> Self {
        Self::default()
    }

    /// Retrieve all recorded entries.
    pub fn entries(&self) -> Vec<ReverieMemoryRecord> {
        self.records.lock().unwrap().clone()
    }
}

impl ReverieClient for InMemoryReverieClient {
    fn record(&self, entry: ReverieMemoryRecord) -> Result<(), EngineError> {
        self.records.lock().unwrap().push(entry);
        Ok(())
    }

    fn search(&self, query: &ReverieContextQuery) -> Result<Vec<ReverieMemoryRecord>, EngineError> {
        let entries = self.records.lock().unwrap();
        let query_lower = query.query.to_lowercase();
        let matches: Vec<ReverieMemoryRecord> = entries
            .iter()
            .filter(|e| {
                if let Some(ref proj) = query.project {
                    if &e.project != proj {
                        return false;
                    }
                }
                e.summary.to_lowercase().contains(&query_lower)
                    || e.topic_key.to_lowercase().contains(&query_lower)
            })
            .take(query.limit)
            .cloned()
            .collect();
        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uas_domain::{EvidenceTier, PrincipalRef};

    #[test]
    fn record_and_search() {
        let client = InMemoryReverieClient::new();
        let att = Attestation {
            signer: PrincipalRef::new("uas/builder-1").unwrap(),
            evidence: EvidenceTier::Verified,
            receipt: "make check: 0 errors".into(),
            at: 1000,
        };
        let record = ReverieMemoryRecord {
            project: "uas".into(),
            session_id: "sess-1".into(),
            topic_key: "uas/build".into(),
            summary: "Executed workspace verification gate".into(),
            attestation: att,
        };

        client.record(record).unwrap();
        assert_eq!(client.entries().len(), 1);

        let query = ReverieContextQuery {
            query: "verification".into(),
            project: Some("uas".into()),
            limit: 10,
        };
        let results = client.search(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].session_id, "sess-1");

        let miss_query = ReverieContextQuery {
            query: "missing".into(),
            project: Some("uas".into()),
            limit: 10,
        };
        let empty = client.search(&miss_query).unwrap();
        assert!(empty.is_empty());
    }
}
