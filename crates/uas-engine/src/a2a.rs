// SPDX-License-Identifier: Apache-2.0 OR MIT
//! A2A multi-agent messaging, artifact references, and peer loss compensation (spec 0030).
//!
//! Implements out-of-band artifact referencing for large binary payloads (REQ-0030-02)
//! and deterministic saga compensation triggers upon peer disconnects (REQ-0030-04).

use serde::{Deserialize, Serialize};

use crate::{Activity, EngineError, WorkflowSession};

/// Threshold above which binary payloads must be stored out-of-band (64 KiB).
pub const MAX_INLINE_PAYLOAD_BYTES: usize = 64 * 1024;

/// Content-addressed reference to an out-of-band artifact (spec 0030 REQ-0030-02, spec 0021).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Storage URI of the artifact in the object repository.
    pub uri: String,
    /// Cryptographic digest (for example BLAKE3 or SHA-256) of the artifact content.
    pub digest: String,
    /// Total payload size in bytes.
    pub size_bytes: u64,
    /// MIME media type.
    pub mime_type: String,
}

/// Message payload carried across agent-to-agent boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessagePayload {
    /// Inlined binary payload (bytes <= [`MAX_INLINE_PAYLOAD_BYTES`]).
    Inline {
        /// Raw content bytes.
        data: Vec<u8>,
    },
    /// Out-of-band artifact reference (bytes > [`MAX_INLINE_PAYLOAD_BYTES`]).
    Reference(ArtifactRef),
}

impl MessagePayload {
    /// Encapsulates a payload, automatically offloading to an artifact reference if it exceeds 64 KiB.
    pub fn auto_offload(
        data: Vec<u8>,
        uri_generator: impl FnOnce() -> String,
        digest_generator: impl FnOnce(&[u8]) -> String,
        mime_type: impl Into<String>,
    ) -> Self {
        if data.len() > MAX_INLINE_PAYLOAD_BYTES {
            let digest = digest_generator(&data);
            let uri = uri_generator();
            Self::Reference(ArtifactRef {
                uri,
                digest,
                size_bytes: data.len() as u64,
                mime_type: mime_type.into(),
            })
        } else {
            Self::Inline { data }
        }
    }

    /// True if the payload was offloaded out-of-band.
    pub fn is_reference(&self) -> bool {
        matches!(self, Self::Reference(_))
    }
}

/// Inter-agent message envelope containing routing and payload metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aMessageEnvelope {
    /// Unique message identifier.
    pub message_id: String,
    /// Sender agent peer address.
    pub sender: String,
    /// Recipient agent peer address.
    pub recipient: String,
    /// Message payload (inline or artifact reference).
    pub payload: MessagePayload,
    /// Timestamp in epoch seconds.
    pub timestamp: u64,
}

/// Triggers deterministic saga compensation when a delegated peer agent disconnects mid-task (REQ-0030-04).
pub fn handle_peer_disconnect(
    session: &mut WorkflowSession,
    peer_address: &str,
    active_activity: &Activity,
    timestamp: u64,
) -> Result<(), EngineError> {
    let failure_reason = format!("delegated peer disconnected mid-task: {peer_address}");
    session.fail_activity(active_activity, failure_reason, timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityKind, SessionId, SessionState};
    use uas_domain::{LeaseId, SlotId};

    #[test]
    fn small_payload_stays_inline() {
        let small_data = vec![0x42; 1024]; // 1 KiB
        let payload = MessagePayload::auto_offload(
            small_data.clone(),
            || "s3://artifacts/small".into(),
            |_| "digest-small".into(),
            "application/octet-stream",
        );

        match payload {
            MessagePayload::Inline { data } => assert_eq!(data, small_data),
            MessagePayload::Reference(_) => panic!("expected inline payload"),
        }
    }

    #[test]
    fn large_payload_offloaded_to_artifact_reference() {
        let large_data = vec![0x99; 128 * 1024]; // 128 KiB > 64 KiB
        let payload = MessagePayload::auto_offload(
            large_data,
            || "s3://artifacts/large-digest".into(),
            |_| "sha256-large-digest".into(),
            "application/octet-stream",
        );

        match payload {
            MessagePayload::Reference(artifact) => {
                assert_eq!(artifact.uri, "s3://artifacts/large-digest");
                assert_eq!(artifact.digest, "sha256-large-digest");
                assert_eq!(artifact.size_bytes, 128 * 1024);
                assert_eq!(artifact.mime_type, "application/octet-stream");
            }
            MessagePayload::Inline { .. } => panic!("expected reference payload"),
        }
    }

    #[test]
    fn peer_disconnect_triggers_saga_compensation() {
        let session_id = SessionId::new("sess-a2a-test").unwrap();
        let slot_id = SlotId::new("slot-a2a-test").unwrap();
        let lease_id = LeaseId::new("lease-a2a-test").unwrap();

        let mut session = WorkflowSession::new(session_id);
        session.start(slot_id, lease_id, 1000).unwrap();

        let activity = Activity::new("act-delegate", "delegate to critic", ActivityKind::Verify);
        session.start_activity(activity.clone(), 1005).unwrap();
        assert_eq!(session.state(), SessionState::Running);

        let res = handle_peer_disconnect(&mut session, "cortex://critic/lane-1", &activity, 1010);
        assert!(res.is_ok());
        assert_eq!(session.state(), SessionState::Failed);
    }
}
