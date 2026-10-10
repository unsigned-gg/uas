//! The `run_hash` transcript digest (spec 0014 D7, CER-2720).
//!
//! `run_hash` is a per-run transcript digest, not a content digest: it absorbs
//! every [`ExecEvent`] in ring-buffer delivery order, in the 168-byte wire form,
//! and `timestamp_ns` and `pid` are part of that form. Two runs of the same
//! commands never hash equal. Replaying one recorded stream does.
//!
//! The receipt carries the dropped-event count. A non-zero count means the
//! kernel could not reserve ring-buffer space for some records, so the
//! transcript is incomplete and the receipt is not attested.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::wire::ExecEvent;

/// Incremental SHA-256 over an execution transcript.
#[derive(Clone, Debug)]
pub struct RunHash {
    hasher: Sha256,
    events: u64,
    dropped_events: u64,
}

impl Default for RunHash {
    fn default() -> Self {
        Self::new()
    }
}

impl RunHash {
    /// Starts an empty transcript.
    pub fn new() -> Self {
        Self {
            hasher: Sha256::new(),
            events: 0,
            dropped_events: 0,
        }
    }

    /// Absorbs one record in wire order.
    pub fn absorb(&mut self, event: &ExecEvent) {
        self.hasher.update(event.to_bytes());
        self.events += 1;
    }

    /// Records that the kernel dropped `count` records since the last call.
    pub fn record_dropped(&mut self, count: u64) {
        self.dropped_events = self.dropped_events.saturating_add(count);
    }

    /// Number of records absorbed so far.
    pub fn events(&self) -> u64 {
        self.events
    }

    /// Number of records the kernel reported dropped so far.
    pub fn dropped_events(&self) -> u64 {
        self.dropped_events
    }

    /// Finalizes the digest into a receipt with an explicit CPU cap (spec 0020).
    pub fn finish_with_cap(self, cpu_cap: impl Into<String>) -> AuditReceipt {
        let digest = self.hasher.finalize();
        AuditReceipt {
            run_hash: hex(&digest),
            events: self.events,
            dropped_events: self.dropped_events,
            attested: self.dropped_events == 0,
            cpu_cap: cpu_cap.into(),
        }
    }

    /// Finalizes the digest into a receipt with default "uncapped" CPU cap.
    pub fn finish(self) -> AuditReceipt {
        self.finish_with_cap("uncapped")
    }
}

/// Default cpu_cap value for deserialization fallback (spec 0020).
fn default_cpu_cap() -> String {
    "uncapped".to_string()
}

/// What a slot session attaches to its Saga receipt (spec 0014, audit hash rule 5 and 6; spec 0020).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditReceipt {
    /// Hex SHA-256 of the transcript.
    pub run_hash: String,
    /// Records absorbed.
    pub events: u64,
    /// Records the kernel dropped. Non-zero invalidates the transcript claim.
    pub dropped_events: u64,
    /// `true` when `dropped_events` is zero.
    pub attested: bool,
    /// CPU cap enforced during the run ("200000 100000", "max", or "uncapped") (spec 0020).
    #[serde(default = "default_cpu_cap")]
    pub cpu_cap: String,
}

/// Lower-case hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream() -> Vec<ExecEvent> {
        vec![
            ExecEvent::new(10, 100, 1, 1000, "cargo", "/usr/bin/cargo"),
            ExecEvent::new(20, 101, 100, 1000, "rustc", "/usr/bin/rustc"),
            ExecEvent::new(30, 102, 100, 1000, "cc", "/usr/bin/cc"),
        ]
    }

    #[test]
    fn empty_transcript_is_sha256_of_nothing() {
        let receipt = RunHash::new().finish();
        assert_eq!(
            receipt.run_hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(receipt.events, 0);
        assert!(receipt.attested);
    }

    #[test]
    fn identical_streams_hash_equal() {
        let mut a = RunHash::new();
        let mut b = RunHash::new();
        for event in stream() {
            a.absorb(&event);
            b.absorb(&event);
        }
        let (a, b) = (a.finish(), b.finish());
        assert_eq!(a, b);
        assert_eq!(a.events, 3);
        assert_eq!(a.run_hash.len(), 64);
    }

    #[test]
    fn order_changes_the_hash() {
        let events = stream();
        let mut forward = RunHash::new();
        let mut reversed = RunHash::new();
        for event in &events {
            forward.absorb(event);
        }
        for event in events.iter().rev() {
            reversed.absorb(event);
        }
        assert_ne!(forward.finish().run_hash, reversed.finish().run_hash);
    }

    #[test]
    fn timestamp_changes_the_hash() {
        let mut a = RunHash::new();
        let mut b = RunHash::new();
        a.absorb(&ExecEvent::new(1, 5, 1, 0, "make", "/usr/bin/make"));
        b.absorb(&ExecEvent::new(2, 5, 1, 0, "make", "/usr/bin/make"));
        assert_ne!(a.finish().run_hash, b.finish().run_hash);
    }

    #[test]
    fn hash_equals_sha256_of_concatenated_wire_bytes() {
        let events = stream();
        let mut run = RunHash::new();
        let mut concat = Vec::new();
        for event in &events {
            run.absorb(event);
            concat.extend_from_slice(&event.to_bytes());
        }
        assert_eq!(run.finish().run_hash, hex(&Sha256::digest(&concat)));
    }

    #[test]
    fn dropped_events_unattest_the_receipt() {
        let mut run = RunHash::new();
        run.absorb(&stream()[0]);
        run.record_dropped(2);
        run.record_dropped(1);
        let receipt = run.finish();
        assert_eq!(receipt.dropped_events, 3);
        assert!(!receipt.attested);
    }

    #[test]
    fn receipt_serializes_to_json() {
        let receipt = RunHash::new().finish();
        assert_eq!(receipt.cpu_cap, "uncapped");
        let json = serde_json::to_string(&receipt).expect("serialize");
        assert!(json.contains("\"attested\":true"));
        assert!(json.contains("\"cpu_cap\":\"uncapped\""));
        let back: AuditReceipt = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, receipt);
    }

    #[test]
    fn finish_with_cap_records_custom_cpu_cap() {
        let receipt = RunHash::new().finish_with_cap("200000 100000");
        assert_eq!(receipt.cpu_cap, "200000 100000");
        assert!(receipt.attested);
    }

    #[test]
    fn receipt_deserializes_missing_cpu_cap_as_uncapped() {
        let legacy_json = r#"{"run_hash":"abc","events":1,"dropped_events":0,"attested":true}"#;
        let receipt: AuditReceipt = serde_json::from_str(legacy_json).expect("deserialize legacy");
        assert_eq!(receipt.cpu_cap, "uncapped");
    }
}
