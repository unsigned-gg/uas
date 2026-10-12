// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Workflow activity definitions, kinds, and execution outcomes.

use serde::{Deserialize, Serialize};
use uas_domain::{Attestation, LostSlot};

/// Discrete classification of an activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    /// Pure verification or static check (e.g. tests, lint, type checks).
    Verify,
    /// Code generation, refactoring, or file modification.
    Impl,
    /// Operator gate or external sign-off signal.
    Gate,
}

/// Description of an activity scheduled within a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    /// Activity identifier.
    pub id: String,
    /// Human-readable title or prompt.
    pub title: String,
    /// Kind of activity.
    pub kind: ActivityKind,
}

impl Activity {
    /// Create a new activity specification.
    pub fn new(id: impl Into<String>, title: impl Into<String>, kind: ActivityKind) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            kind,
        }
    }
}

use std::fmt;

/// Reverie memory status for an activity result (spec 0044 D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryStatus {
    /// Outbox directory refused (permissions/symlink) or append failed.
    OutboxRefused,
    /// Health probe or context search failed; activity continued with empty context.
    Degraded,
    /// Write-behind records are pending in the outbox.
    Outbox(usize),
    /// Reverie daemon responded live and outbox is clear.
    Live,
}

impl fmt::Display for MemoryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutboxRefused => write!(f, "outbox(refused)"),
            Self::Degraded => write!(f, "degraded"),
            Self::Outbox(n) => write!(f, "outbox({n})"),
            Self::Live => write!(f, "live"),
        }
    }
}

impl Serialize for MemoryStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for MemoryStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == "outbox(refused)" {
            Ok(Self::OutboxRefused)
        } else if s == "degraded" {
            Ok(Self::Degraded)
        } else if s == "live" {
            Ok(Self::Live)
        } else if let Some(rest) = s.strip_prefix("outbox(").and_then(|r| r.strip_suffix(')')) {
            let n = rest.parse::<usize>().map_err(serde::de::Error::custom)?;
            Ok(Self::Outbox(n))
        } else {
            Err(serde::de::Error::custom(format!(
                "unknown memory status: {s}"
            )))
        }
    }
}

/// Outcome of executing an activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityResult {
    /// Identifier of the activity that executed.
    pub activity_id: String,
    /// Whether the execution completed successfully.
    pub success: bool,
    /// Attestation certifying the evidence tier and receipt.
    pub attestation: Attestation,
    /// Optional output summary or diagnostic text.
    pub summary: Option<String>,
    /// Reverie memory status (spec 0044 D5).
    #[serde(default)]
    pub memory: Option<MemoryStatus>,
}

/// What the caller does after an activity error (spec 0040 D5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityDisposition {
    /// The activity failed and the session ended in `Failed`.
    Failed,
    /// The pool no longer knows the lease. The compensators ran and the session
    /// ended in `Failed`. The caller acquires a fresh lease and re-admits the task.
    Readmit(LostSlot),
}

#[cfg(test)]
mod tests {
    use super::*;
    use uas_domain::{EvidenceTier, PrincipalRef};

    #[test]
    fn activity_construction() {
        let act = Activity::new("act-1", "Run make check", ActivityKind::Verify);
        assert_eq!(act.id, "act-1");
        assert_eq!(act.kind, ActivityKind::Verify);
    }

    #[test]
    fn activity_result_round_trip() {
        let att = Attestation {
            signer: PrincipalRef::new("uas/builder-1").unwrap(),
            evidence: EvidenceTier::Verified,
            receipt: "make check: ok".into(),
            at: 1000,
        };
        let res = ActivityResult {
            activity_id: "act-1".into(),
            success: true,
            attestation: att,
            summary: Some("all gates passed".into()),
            memory: Some(MemoryStatus::Live),
        };
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"verified\""));
        assert!(json.contains("\"memory\":\"live\""));

        let deserialized: ActivityResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.memory, Some(MemoryStatus::Live));

        // Spec 0044 D5: None when memory field absent in older serialized events
        let legacy_json = r#"{"activity_id":"act-legacy","success":true,"attestation":{"signer":"uas/builder-1","evidence":"verified","receipt":"ok","at":1000},"summary":null}"#;
        let legacy_res: ActivityResult = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(legacy_res.memory, None);

        // Test display formatting and parsing for each variant
        assert_eq!(MemoryStatus::OutboxRefused.to_string(), "outbox(refused)");
        assert_eq!(MemoryStatus::Degraded.to_string(), "degraded");
        assert_eq!(MemoryStatus::Outbox(3).to_string(), "outbox(3)");
        assert_eq!(MemoryStatus::Live.to_string(), "live");

        let de_refused: MemoryStatus = serde_json::from_str("\"outbox(refused)\"").unwrap();
        assert_eq!(de_refused, MemoryStatus::OutboxRefused);
        let de_outbox3: MemoryStatus = serde_json::from_str("\"outbox(3)\"").unwrap();
        assert_eq!(de_outbox3, MemoryStatus::Outbox(3));
    }
}
