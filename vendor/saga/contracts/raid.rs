//! Domain types for raid matrix dispatch and batched gate reviews.
//! Zero I/O, zero suite dependencies.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::flow::{Budget, Limits};

/// Configuration for a raid fan-out sweep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidConfig {
    pub name: String,
    pub flow: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<Limits>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<Budget>,
    pub runs: Vec<RaidRunSpec>,
}

/// Specification for one target run in a raid sweep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidRunSpec {
    pub id: String,
    pub repo: String,
    pub flow: String,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

/// Verification status of a worktree branch landing on origin/main.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandingStatus {
    pub landed: bool,
    pub commit: String,
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unmerged_reason: Option<String>,
}

/// Report emitted upon retiring a raid root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaidRetireReport {
    pub raid_id: String,
    pub worktrees_removed: usize,
    pub force_abandon: bool,
    pub retired_at: String,
}

/// Live status report of an existing raid root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidStatusReport {
    pub raid_id: String,
    pub name: String,
    pub root_path: String,
    pub runs: Vec<RaidRunStatus>,
    pub all_landed: bool,
}

/// Status of an individual run within a raid root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidRunStatus {
    pub id: String,
    pub repo: String,
    pub flow: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    pub landed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_tier: Option<crate::evidence::EvidenceTier>,
}

/// Summary of a gate awaiting operator review across any run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingGateSummary {
    pub run_id: String,
    pub flow_id: String,
    pub step_id: String,
    pub step_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    pub elapsed_secs: u64,
    pub tokens_used: u64,
    pub cost_usd: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_receipt: Option<String>,
    pub has_verified_receipt: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<String>,
    pub gate_prompt: String,
}

/// Decision for a gate review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateDecision {
    Approve,
    Deny { reason: String },
}
