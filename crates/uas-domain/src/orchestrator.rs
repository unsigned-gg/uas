// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Pure state machine AST orchestrator nodes for Agent-to-Agent (A2A) multi-agent interop.
//!
//! Spec 0030:
//! - REQ-0030-01: Pure state machine types for `LlmAgent`, `Sequential`, `Parallel`, and `Loop`.
//! - REQ-0030-05: `AgentTool` representing delegated capabilities.
//! - REQ-0030-08: Maximum delegation depth of 5 to prevent infinite loops.

use crate::{DomainError, Provider};

/// Maximum allowed delegation depth in orchestrator AST trees (REQ-0030-08).
pub const MAX_DELEGATION_DEPTH: usize = 5;

/// An agent capability tool representing delegated peer functionality (REQ-0030-05).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentTool {
    /// Name of the delegated tool.
    pub name: String,
    /// Description of the capability.
    pub description: String,
    /// Target peer address or capability route.
    pub target: String,
}

impl AgentTool {
    /// Constructs a validated `AgentTool`.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        target: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let name = name.into();
        let description = description.into();
        let target = target.into();

        if name.trim().is_empty() {
            return Err(DomainError::Invalid {
                kind: "agent_tool_name",
                value: name,
                reason: "tool name cannot be empty",
            });
        }
        if target.trim().is_empty() {
            return Err(DomainError::Invalid {
                kind: "agent_tool_target",
                value: target,
                reason: "tool target cannot be empty",
            });
        }

        Ok(Self {
            name,
            description,
            target,
        })
    }
}

/// An autonomous LLM-backed agent leaf node (REQ-0030-01).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LlmAgent {
    /// Unique agent node identifier.
    pub id: String,
    /// Underlying model provider.
    pub provider: Provider,
    /// Model name or alias.
    pub model: String,
    /// System charter or instruction prompt.
    pub system_prompt: String,
    /// Delegated capabilities available to this agent.
    pub tools: Vec<AgentTool>,
}

impl LlmAgent {
    /// Constructs a new `LlmAgent` node.
    pub fn new(
        id: impl Into<String>,
        provider: Provider,
        model: impl Into<String>,
        system_prompt: impl Into<String>,
        tools: Vec<AgentTool>,
    ) -> Result<Self, DomainError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(DomainError::Invalid {
                kind: "agent_id",
                value: id,
                reason: "agent id cannot be empty",
            });
        }
        Ok(Self {
            id,
            provider,
            model: model.into(),
            system_prompt: system_prompt.into(),
            tools,
        })
    }
}

/// A deterministic sequential execution chain of orchestrator steps (REQ-0030-01).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Sequential {
    /// Ordered sequence of child steps.
    pub steps: Vec<OrchestratorNode>,
}

impl Sequential {
    /// Creates a new `Sequential` chain.
    pub fn new(steps: Vec<OrchestratorNode>) -> Self {
        Self { steps }
    }
}

/// A concurrent parallel branch of orchestrator steps (REQ-0030-01).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Parallel {
    /// Concurrent execution branches.
    pub branches: Vec<OrchestratorNode>,
}

impl Parallel {
    /// Creates a new `Parallel` branching node.
    pub fn new(branches: Vec<OrchestratorNode>) -> Self {
        Self { branches }
    }
}

/// A deterministic bounded loop construct (REQ-0030-01).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Loop {
    /// Inner body node to execute repeatedly.
    pub body: Box<OrchestratorNode>,
    /// Maximum iteration limit before terminating.
    pub max_iterations: u32,
}

impl Loop {
    /// Creates a new `Loop` construct.
    pub fn new(body: OrchestratorNode, max_iterations: u32) -> Self {
        Self {
            body: Box::new(body),
            max_iterations,
        }
    }
}

/// Unified declarative orchestrator AST node enum (REQ-0030-01).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OrchestratorNode {
    /// Autonomous LLM agent node.
    Agent(LlmAgent),
    /// Sequential chain of steps.
    Sequential(Sequential),
    /// Parallel concurrent execution branches.
    Parallel(Parallel),
    /// Bounded iterative loop.
    Loop(Loop),
}

impl OrchestratorNode {
    /// Wrap an `LlmAgent` into a node.
    pub fn agent(agent: LlmAgent) -> Self {
        Self::Agent(agent)
    }

    /// Wrap a list of steps into a `Sequential` node.
    pub fn sequential(steps: Vec<OrchestratorNode>) -> Self {
        Self::Sequential(Sequential::new(steps))
    }

    /// Wrap a list of branches into a `Parallel` node.
    pub fn parallel(branches: Vec<OrchestratorNode>) -> Self {
        Self::Parallel(Parallel::new(branches))
    }

    /// Wrap a body and max iterations into a `Loop` node.
    pub fn loop_node(body: OrchestratorNode, max_iterations: u32) -> Self {
        Self::Loop(Loop::new(body, max_iterations))
    }

    /// Calculates the maximum delegation depth of this AST tree.
    pub fn depth(&self) -> usize {
        match self {
            Self::Agent(_) => 1,
            Self::Sequential(seq) => 1 + seq.steps.iter().map(Self::depth).max().unwrap_or(0),
            Self::Parallel(par) => 1 + par.branches.iter().map(Self::depth).max().unwrap_or(0),
            Self::Loop(l) => 1 + l.body.depth(),
        }
    }

    /// Validates that the AST does not exceed the maximum delegation depth of 5 (REQ-0030-08).
    pub fn validate(&self) -> Result<(), DomainError> {
        let current_depth = self.depth();
        if current_depth > MAX_DELEGATION_DEPTH {
            return Err(DomainError::Invalid {
                kind: "orchestrator_ast",
                value: format!("depth {current_depth}"),
                reason: "delegation depth exceeds maximum limit of 5",
            });
        }
        Ok(())
    }
}
