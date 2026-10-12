// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Agent-to-Agent (A2A) protocol gateway and peer negotiation (spec 0030).
//!
//! Provides protocol version negotiation, Cortex Mesh peer addressing,
//! and cross-framework message routing for multi-agent workflows.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Supported A2A wire protocol versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum A2aVersion {
    /// Initial baseline A2A protocol (v1.0).
    #[serde(rename = "1.0")]
    V1_0 = 1,
    /// Enhanced A2A protocol with artifact referencing (v1.1).
    #[serde(rename = "1.1")]
    V1_1 = 2,
}

impl A2aVersion {
    /// Negotiates the highest mutually supported protocol version.
    ///
    /// REQ-0030-07: Downgrades to the highest mutually supported version.
    pub fn negotiate(client: &[Self], supported: &[Self]) -> Result<Self, A2aError> {
        let mut matches: Vec<Self> = client
            .iter()
            .copied()
            .filter(|v| supported.contains(v))
            .collect();
        matches.sort_unstable();
        matches.pop().ok_or(A2aError::VersionMismatch)
    }

    /// String representation of the version.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::V1_0 => "1.0",
            Self::V1_1 => "1.1",
        }
    }
}

/// Errors raised by the A2A protocol gateway.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum A2aError {
    /// No mutually supported protocol version found.
    #[error("protocol version negotiation failed: no mutual version")]
    VersionMismatch,
    /// The peer address format is invalid.
    #[error("invalid peer address: {0}")]
    InvalidAddress(String),
    /// Wildcard routing keys are strictly forbidden.
    #[error("wildcard addresses are forbidden to prevent context leakage")]
    WildcardForbidden,
    /// Peer cryptographic authentication failed.
    #[error("peer authentication failed: {0}")]
    AuthenticationFailed(String),
    /// Target peer is not registered or reachable.
    #[error("target peer not found: {0}")]
    PeerNotFound(String),
}

/// Peer address referencing an agent over the Cortex Mesh (spec 0030 REQ-0030-06).
///
/// Addresses must follow the format `cortex://<role>/<lane>` or `cortex://<role>`.
/// Wildcards (`*`, `?`) are rejected to prevent broadcast leakage.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct A2aPeerAddress {
    role: String,
    lane: Option<String>,
}

impl A2aPeerAddress {
    /// Parses and validates a Cortex Mesh peer address string.
    pub fn parse(s: &str) -> Result<Self, A2aError> {
        if s.contains('*') || s.contains('?') {
            return Err(A2aError::WildcardForbidden);
        }

        let without_scheme = s
            .strip_prefix("cortex://")
            .ok_or_else(|| A2aError::InvalidAddress("missing 'cortex://' scheme".into()))?;

        if without_scheme.is_empty() {
            return Err(A2aError::InvalidAddress("empty role".into()));
        }

        let parts: Vec<&str> = without_scheme.split('/').collect();
        match parts.as_slice() {
            [role] if !role.is_empty() => Ok(Self {
                role: (*role).to_string(),
                lane: None,
            }),
            [role, lane] if !role.is_empty() && !lane.is_empty() => Ok(Self {
                role: (*role).to_string(),
                lane: Some((*lane).to_string()),
            }),
            _ => Err(A2aError::InvalidAddress(
                "address must be cortex://<role> or cortex://<role>/<lane>".into(),
            )),
        }
    }

    /// The agent role.
    pub fn role(&self) -> &str {
        &self.role
    }

    /// The agent lane, if specified.
    pub fn lane(&self) -> Option<&str> {
        self.lane.as_deref()
    }

    /// Formats the peer address as a URI.
    pub fn as_uri(&self) -> String {
        match &self.lane {
            Some(l) => format!("cortex://{}/{}", self.role, l),
            None => format!("cortex://{}", self.role),
        }
    }
}

/// Handshake request sent by a peer initiating an A2A session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aHandshakeRequest {
    /// Supported versions offered by the client peer.
    pub versions: Vec<A2aVersion>,
    /// Pre-shared or signed authentication token.
    pub auth_token: String,
}

/// Handshake response returned by the gateway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aHandshakeResponse {
    /// Negotiated protocol version.
    pub negotiated_version: A2aVersion,
    /// Established session identifier.
    pub session_id: String,
}

/// Message envelope dispatched across A2A peers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aEnvelope {
    /// Source peer address.
    pub source: A2aPeerAddress,
    /// Target peer address.
    pub target: A2aPeerAddress,
    /// Payload payload content or reference.
    pub payload: serde_json::Value,
    /// Protocol version used for this transmission.
    pub version: A2aVersion,
}

/// A2A Gateway mediating multi-agent cross-framework connections (spec 0030 REQ-0030-03).
pub struct A2aGateway {
    supported_versions: Vec<A2aVersion>,
    expected_token: String,
    peers: BTreeMap<A2aPeerAddress, A2aVersion>,
    next_session_id: u64,
}

impl A2aGateway {
    /// Creates a new gateway with expected authentication token.
    pub fn new(expected_token: impl Into<String>) -> Self {
        Self {
            supported_versions: vec![A2aVersion::V1_0, A2aVersion::V1_1],
            expected_token: expected_token.into(),
            peers: BTreeMap::new(),
            next_session_id: 1,
        }
    }

    /// Returns the supported protocol versions.
    pub fn supported_versions(&self) -> &[A2aVersion] {
        &self.supported_versions
    }

    /// Handles an incoming connection handshake from a peer address.
    pub fn connect(
        &mut self,
        address: &A2aPeerAddress,
        req: &A2aHandshakeRequest,
    ) -> Result<A2aHandshakeResponse, A2aError> {
        if req.auth_token != self.expected_token {
            return Err(A2aError::AuthenticationFailed("invalid token".into()));
        }

        let negotiated = A2aVersion::negotiate(&req.versions, &self.supported_versions)?;
        let session_id = format!("a2a-sess-{}", self.next_session_id);
        self.next_session_id += 1;

        self.peers.insert(address.clone(), negotiated);

        Ok(A2aHandshakeResponse {
            negotiated_version: negotiated,
            session_id,
        })
    }

    /// Dispatches an envelope to a registered target peer.
    pub fn dispatch(&self, envelope: &A2aEnvelope) -> Result<(), A2aError> {
        let peer_version = self
            .peers
            .get(&envelope.target)
            .ok_or_else(|| A2aError::PeerNotFound(envelope.target.as_uri()))?;

        if envelope.version > *peer_version {
            return Err(A2aError::VersionMismatch);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_negotiation_selects_highest_common() {
        let client = [A2aVersion::V1_0, A2aVersion::V1_1];
        let supported = [A2aVersion::V1_0];
        let negotiated = A2aVersion::negotiate(&client, &supported).unwrap();
        assert_eq!(negotiated, A2aVersion::V1_0);

        let both = [A2aVersion::V1_0, A2aVersion::V1_1];
        let negotiated_both = A2aVersion::negotiate(&both, &both).unwrap();
        assert_eq!(negotiated_both, A2aVersion::V1_1);
    }

    #[test]
    fn version_negotiation_fails_when_disjoint() {
        let client = [A2aVersion::V1_1];
        let supported = [A2aVersion::V1_0];
        assert_eq!(
            A2aVersion::negotiate(&client, &supported),
            Err(A2aError::VersionMismatch)
        );
    }

    #[test]
    fn peer_address_parsing_and_wildcard_rejection() {
        let valid = A2aPeerAddress::parse("cortex://builder/lane-1").unwrap();
        assert_eq!(valid.role(), "builder");
        assert_eq!(valid.lane(), Some("lane-1"));
        assert_eq!(valid.as_uri(), "cortex://builder/lane-1");

        let without_lane = A2aPeerAddress::parse("cortex://critic").unwrap();
        assert_eq!(without_lane.role(), "critic");
        assert_eq!(without_lane.lane(), None);

        assert_eq!(
            A2aPeerAddress::parse("cortex://*/lane"),
            Err(A2aError::WildcardForbidden)
        );
        assert_eq!(
            A2aPeerAddress::parse("cortex://builder?"),
            Err(A2aError::WildcardForbidden)
        );
        assert!(matches!(
            A2aPeerAddress::parse("http://builder"),
            Err(A2aError::InvalidAddress(_))
        ));
    }

    #[test]
    fn gateway_handshake_and_dispatch() {
        let mut gateway = A2aGateway::new("secret-key");
        let peer_addr = A2aPeerAddress::parse("cortex://uas-critic/lane-w1").unwrap();

        let req = A2aHandshakeRequest {
            versions: vec![A2aVersion::V1_0, A2aVersion::V1_1],
            auth_token: "secret-key".into(),
        };

        let resp = gateway.connect(&peer_addr, &req).unwrap();
        assert_eq!(resp.negotiated_version, A2aVersion::V1_1);
        assert!(resp.session_id.starts_with("a2a-sess-"));

        let envelope = A2aEnvelope {
            source: A2aPeerAddress::parse("cortex://uas-builder/lane-w0").unwrap(),
            target: peer_addr.clone(),
            payload: serde_json::json!({"action": "review"}),
            version: A2aVersion::V1_1,
        };

        assert!(gateway.dispatch(&envelope).is_ok());

        let unknown_peer = A2aPeerAddress::parse("cortex://unknown").unwrap();
        let unknown_envelope = A2aEnvelope {
            source: peer_addr,
            target: unknown_peer,
            payload: serde_json::json!({}),
            version: A2aVersion::V1_0,
        };
        assert!(matches!(
            gateway.dispatch(&unknown_envelope),
            Err(A2aError::PeerNotFound(_))
        ));
    }
}
