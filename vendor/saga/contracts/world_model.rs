//! Pure domain types for the Engineering World Model.
//!
//! No I/O, no suite dependencies. Inner hexagon.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Effort / reasoning budget level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffortLevel {
    Low,
    Medium,
    High,
    Max,
}

impl EffortLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            EffortLevel::Low => "low",
            EffortLevel::Medium => "medium",
            EffortLevel::High => "high",
            EffortLevel::Max => "max",
        }
    }

    /// Escalate reasoning effort budget for retry/remediation rounds.
    pub fn escalate(&self) -> Self {
        match self {
            EffortLevel::Low => EffortLevel::Medium,
            EffortLevel::Medium => EffortLevel::High,
            EffortLevel::High => EffortLevel::Max,
            EffortLevel::Max => EffortLevel::Max,
        }
    }
}

impl std::fmt::Display for EffortLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A selected model and its effort level.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelEffort {
    pub model: String,
    pub effort: EffortLevel,
}

impl ModelEffort {
    pub fn new(model: impl Into<String>, effort: EffortLevel) -> Self {
        Self {
            model: model.into(),
            effort,
        }
    }

    /// Format for Dagr v3 render chip: `model·effort`.
    pub fn chip(&self) -> String {
        format!("{}·{}", self.model, self.effort.as_str())
    }
}

/// Recorded outcome of a single task attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskOutcomeRecord {
    pub task_id: String,
    pub title: String,
    pub kind: String,
    pub agent: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub state: String, // done | failed | rejected
    pub evidence: String,
    pub receipt: Option<String>,
    pub reason: Option<String>,
    pub duration_ms: u64,
    pub settled_at: DateTime<Utc>,
    /// Tournament score of the winning candidate (specs/0015). `None` for
    /// single-attempt paths or steps without a rollout.
    #[serde(default)]
    pub tournament_score: Option<u64>,
    /// 1-based rank of this attempt among N candidates. `None` when no
    /// tournament was held.
    #[serde(default)]
    pub tournament_rank: Option<u32>,
    /// Breadth of the rollout fan-out. `None` for single-attempt execution.
    #[serde(default)]
    pub rollout_n: Option<u32>,
    /// Harness-reported token usage. `None` when not reported by harness.
    #[serde(default)]
    pub tokens: Option<u64>,
    /// Estimated or calculated cost in USD.
    #[serde(default)]
    pub cost_usd: Option<f64>,
    /// Source of cost accounting: "reported" or "none" (normative ruling: never estimate).
    #[serde(default)]
    pub cost_source: Option<String>,
}

/// Recorded outcome of an entire workflow run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunOutcomeRecord {
    pub run_id: String,
    pub flow: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub tasks: Vec<TaskOutcomeRecord>,
    pub success: bool,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub total_cost_usd: f64,
}

/// Aggregate performance counts for a category.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CategoryStats {
    pub total: u64,
    pub passed: u64,
    pub failed: u64,
}

impl CategoryStats {
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.passed as f64) / (self.total as f64)
        }
    }
}

/// Summary metrics aggregated across the world model.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldModelSummary {
    pub total_runs: u64,
    pub successful_runs: u64,
    pub total_tasks: u64,
    pub successful_tasks: u64,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub total_cost_usd: f64,
    pub by_model: BTreeMap<String, CategoryStats>,
    pub by_agent: BTreeMap<String, CategoryStats>,
    pub by_kind: BTreeMap<String, CategoryStats>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_effort_chip_formatting() {
        let me = ModelEffort::new("claude-3-7-sonnet", EffortLevel::High);
        assert_eq!(me.chip(), "claude-3-7-sonnet·high");
    }

    #[test]
    fn category_stats_rate() {
        let stats = CategoryStats {
            total: 10,
            passed: 8,
            failed: 2,
        };
        assert!((stats.success_rate() - 0.8).abs() < 1e-6);
    }

    #[test]
    fn effort_level_escalation() {
        assert_eq!(EffortLevel::Low.escalate(), EffortLevel::Medium);
        assert_eq!(EffortLevel::Medium.escalate(), EffortLevel::High);
        assert_eq!(EffortLevel::High.escalate(), EffortLevel::Max);
        assert_eq!(EffortLevel::Max.escalate(), EffortLevel::Max);
    }
}
