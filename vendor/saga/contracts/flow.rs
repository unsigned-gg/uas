//! Flow graph and step specification types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::StepId;

/// A validated, instantiated flow graph — input to bifrost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowGraph {
    pub flow: String,
    pub params: BTreeMap<String, String>,
    pub steps: Vec<StepSpec>,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub budget: Budget,
}

/// Budget caps for a flow run or individual step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Budget {
    /// Maximum tokens allowed. `None` = unbounded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// Maximum cost in USD allowed. `None` = unbounded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cost_usd: Option<f64>,
}

/// Concurrency limits for a flow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Limits {
    /// Maximum number of steps that may run concurrently. `None` = unbounded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_in_flight: Option<usize>,
    /// Per-agent-kind concurrency limits. Keys are agent names (e.g. "claude",
    /// "omp"). A step routed to an agent not in this map is unbounded.
    #[serde(default)]
    pub per_agent: BTreeMap<String, usize>,
}

/// One step in a flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepSpec {
    pub id: StepId,
    pub title: String,
    pub kind: String,
    pub agent: AgentSelection,
    pub cwd: PathKind,
    pub prompt: String,
    pub deps: Vec<StepId>,
    pub inputs: Vec<StepId>,
    pub verify_cmd: Option<Vec<String>>,
    pub rounds_max: Option<u32>,
    pub on_fail: Option<StepId>,
    pub on_pass: Option<StepId>,
    pub gate: Option<GateKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<Budget>,
    /// Persona agent file (`.claude/agents/<name>.md`) the harness passes to claude.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentSelection {
    Named(String),
    Auto,
    Unassigned,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PathKind {
    Project,
    /// A per-run git worktree on its own branch, created by the executor on the first step
    /// that asks for it and shared by every later such step in the run.
    Worktree,
    Abs(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GateKind {
    HeimdalApproval,
}
