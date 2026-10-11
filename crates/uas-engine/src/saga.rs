// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Saga flow execution coordinator and compensation runner.
//!
//! Bridges `.saga/flows/*.toml` definitions with [`crate::WorkflowSession`] executions,
//! enforcing compensators in reverse order upon step failure.

use serde::{Deserialize, Serialize};

use crate::compensator::{finish_rollback, receipts_or_failure};
use crate::{Activity, ActivityKind, CompensationReceipt, Compensator, EngineError};

/// Metadata header for a Saga flow definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowMetadata {
    /// Name of the flow.
    pub name: String,
    /// Human-readable flow description.
    #[serde(default)]
    pub description: Option<String>,
}

/// Raw representation of a step in a TOML flow file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SagaStepDefRaw {
    /// Step identifier.
    pub id: String,
    /// Step title.
    pub title: String,
    /// Activity kind as string ("verify", "impl", "test", "gate").
    pub kind: String,
    /// Optional agent persona ("auto", etc.).
    #[serde(default)]
    pub agent: Option<String>,
    /// Step dependencies.
    #[serde(default)]
    pub deps: Vec<String>,
    /// Verification command.
    #[serde(default)]
    pub verify_cmd: Vec<String>,
    /// Prompt or instruction text.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Approval gate name.
    #[serde(default)]
    pub gate: Option<String>,
    /// Estimated duration in arbitrary time units.
    #[serde(default)]
    pub duration: Option<u64>,
}

/// A parsed Saga flow definition deserialized from `.saga/flows/*.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SagaFlowDef {
    /// Flow header metadata.
    pub flow: FlowMetadata,
    /// List of step specifications.
    #[serde(default, rename = "step")]
    pub steps: Vec<SagaStepDefRaw>,
}

impl SagaFlowDef {
    /// Parse a flow definition from TOML text.
    pub fn from_toml(content: &str) -> Result<Self, EngineError> {
        toml::from_str(content).map_err(|e| EngineError::InvalidFlow {
            reason: format!("failed to parse TOML: {e}"),
        })
    }

    /// Convert this definition into typed [`SagaStep`]s.
    pub fn to_steps(&self) -> Vec<SagaStep> {
        self.steps
            .iter()
            .map(|raw| {
                let kind = match raw.kind.to_ascii_lowercase().as_str() {
                    "verify" | "test" => ActivityKind::Verify,
                    "impl" => ActivityKind::Impl,
                    "gate" => ActivityKind::Gate,
                    _ => ActivityKind::Verify,
                };
                SagaStep {
                    id: raw.id.clone(),
                    title: raw.title.clone(),
                    kind,
                    deps: raw.deps.clone(),
                    verify_cmd: raw.verify_cmd.clone(),
                    gate: raw.gate.clone(),
                    agent: raw.agent.clone(),
                    duration: raw.duration,
                }
            })
            .collect()
    }
}

/// A step definition within a Saga flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SagaStep {
    /// Step identifier (e.g. "custody-check", "spec-check", "operator-gate").
    pub id: String,
    /// Step title.
    pub title: String,
    /// Kind of activity.
    pub kind: ActivityKind,
    /// Prerequisites steps that must complete before this step runs.
    pub deps: Vec<String>,
    /// Verification command (e.g. `["make", "check"]`).
    pub verify_cmd: Vec<String>,
    /// Operator gate identifier (e.g. `heimdal:approval`).
    pub gate: Option<String>,
    /// Optional agent persona or hint ("auto", "probe", "builder", "critic").
    pub agent: Option<String>,
    /// Estimated duration in arbitrary time units.
    pub duration: Option<u64>,
}

impl SagaStep {
    /// Create a new Saga step.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        kind: ActivityKind,
        deps: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            kind,
            deps,
            verify_cmd: Vec::new(),
            gate: None,
            agent: None,
            duration: None,
        }
    }

    /// Builder method to attach a verification command.
    pub fn with_verify_cmd(mut self, cmd: Vec<String>) -> Self {
        self.verify_cmd = cmd;
        self
    }

    /// Builder method to attach an operator gate.
    pub fn with_gate(mut self, gate: impl Into<String>) -> Self {
        self.gate = Some(gate.into());
        self
    }

    /// Builder method to attach an agent persona or hint.
    pub fn with_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }

    /// Builder method to attach an estimated duration.
    pub fn with_duration(mut self, duration: u64) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Return estimated duration or default inferred by [`ActivityKind`].
    pub fn effective_duration(&self) -> u64 {
        if let Some(d) = self.duration {
            return d;
        }
        match self.kind {
            ActivityKind::Verify => 5,
            ActivityKind::Impl => 20,
            ActivityKind::Gate => 1,
        }
    }
}

/// A coordinator running a DAG of Saga steps within a Substrate session.
pub struct SagaFlowCoordinator {
    steps: Vec<SagaStep>,
    completed: Vec<String>,
    compensator: Box<dyn Compensator>,
}

impl SagaFlowCoordinator {
    /// Create a new coordinator with steps and a compensator.
    pub fn new(steps: Vec<SagaStep>, compensator: Box<dyn Compensator>) -> Self {
        Self {
            steps,
            completed: Vec::new(),
            compensator,
        }
    }

    /// Create a coordinator directly from a parsed [`SagaFlowDef`].
    pub fn from_flow_def(def: &SagaFlowDef, compensator: Box<dyn Compensator>) -> Self {
        Self::new(def.to_steps(), compensator)
    }

    /// Access all steps in this flow.
    pub fn steps(&self) -> &[SagaStep] {
        &self.steps
    }

    /// Access completed step identifiers in execution order.
    pub fn completed(&self) -> &[String] {
        &self.completed
    }

    /// Retrieve the next runnable step whose dependencies are satisfied.
    pub fn next_runnable_step(&self) -> Option<&SagaStep> {
        self.steps.iter().find(|step| {
            !self.completed.contains(&step.id)
                && step.deps.iter().all(|dep| self.completed.contains(dep))
        })
    }

    /// Mark a step completed.
    pub fn mark_completed(&mut self, step_id: &str) {
        if !self.completed.contains(&step_id.to_string()) {
            self.completed.push(step_id.to_string());
        }
    }

    /// Check if all steps in the flow are completed.
    pub fn is_finished(&self) -> bool {
        self.completed.len() == self.steps.len()
    }

    /// Deprecated: moves to saga (spec 0049); removed with the CPM surface.
    ///
    /// Compute the Critical Path Method (CPM) schedule for this flow.
    pub fn cpm_schedule(&self) -> Result<crate::CpmSchedule, EngineError> {
        crate::CpmSchedule::compute(&self.steps)
    }

    /// Deprecated: moves to saga (spec 0049); removed with the wave surface.
    ///
    /// Plan concurrent execution waves using CPM critical path priority.
    pub fn plan_waves(&self, max_concurrency: usize) -> Result<crate::WavePlan, EngineError> {
        crate::WavePlan::plan(&self.steps, max_concurrency)
    }

    /// Trigger compensations for all completed steps in reverse order.
    pub fn rollback(
        &self,
        failed_step: &str,
        reason: &str,
    ) -> Result<Vec<CompensationReceipt>, EngineError> {
        tracing::warn!(failed_step, reason, "triggering saga compensations");
        let mut all_receipts = Vec::new();
        for step_id in self.completed.iter().rev() {
            if let Some(step) = self.steps.iter().find(|s| &s.id == step_id) {
                let activity = Activity::new(&step.id, &step.title, step.kind);
                let result = self.compensator.compensate(&activity, reason);
                all_receipts.extend(receipts_or_failure(
                    self.compensator.name(),
                    &activity,
                    result,
                ));
            }
        }
        finish_rollback(all_receipts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensator::{FailOnCompensator, RecordingCompensator};

    #[test]
    fn flow_dependency_ordering() {
        let rec = RecordingCompensator::new();
        let steps = vec![
            SagaStep::new("custody", "Check custody", ActivityKind::Verify, vec![]),
            SagaStep::new(
                "spec",
                "Spec check",
                ActivityKind::Verify,
                vec!["custody".into()],
            ),
            SagaStep::new(
                "build",
                "Implement",
                ActivityKind::Impl,
                vec!["spec".into()],
            ),
        ];

        let mut coord = SagaFlowCoordinator::new(steps, Box::new(rec.clone()));

        assert_eq!(coord.next_runnable_step().unwrap().id, "custody");
        coord.mark_completed("custody");

        assert_eq!(coord.next_runnable_step().unwrap().id, "spec");
        coord.mark_completed("spec");

        assert_eq!(coord.next_runnable_step().unwrap().id, "build");
        coord.mark_completed("build");

        assert!(coord.is_finished());
        assert!(coord.next_runnable_step().is_none());
    }

    #[test]
    fn rollback_compensates_in_reverse_order() {
        let rec = RecordingCompensator::new();
        let steps = vec![
            SagaStep::new("step-1", "Step 1", ActivityKind::Impl, vec![]),
            SagaStep::new(
                "step-2",
                "Step 2",
                ActivityKind::Impl,
                vec!["step-1".into()],
            ),
        ];

        let mut coord = SagaFlowCoordinator::new(steps, Box::new(rec.clone()));
        coord.mark_completed("step-1");
        coord.mark_completed("step-2");

        let receipts = coord.rollback("step-3", "compilation failed").unwrap();

        let recorded = rec.recorded();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].0, "step-2");
        assert_eq!(recorded[1].0, "step-1");

        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].activity_id, "step-2");
        assert_eq!(receipts[1].activity_id, "step-1");
    }

    #[test]
    fn parse_flow_from_toml() {
        let toml_str = r#"
[flow]
name = "test-flow"
description = "A test workflow"

[[step]]
id = "step-a"
title = "Step A"
kind = "verify"
verify_cmd = ["echo", "a"]

[[step]]
id = "step-b"
title = "Step B"
kind = "impl"
deps = ["step-a"]
verify_cmd = ["echo", "b"]
"#;

        let flow_def = SagaFlowDef::from_toml(toml_str).unwrap();
        assert_eq!(flow_def.flow.name, "test-flow");
        assert_eq!(flow_def.steps.len(), 2);

        let steps = flow_def.to_steps();
        assert_eq!(steps[0].id, "step-a");
        assert_eq!(steps[0].kind, ActivityKind::Verify);
        assert_eq!(steps[1].id, "step-b");
        assert_eq!(steps[1].kind, ActivityKind::Impl);
        assert_eq!(steps[1].deps, vec!["step-a"]);
    }

    #[test]
    fn saga_rollback_continues_after_compensation_failure() {
        let steps = vec![
            SagaStep::new("step-1", "Step 1", ActivityKind::Verify, vec![]),
            SagaStep::new("step-2", "Step 2", ActivityKind::Verify, vec![]),
            SagaStep::new("step-3", "Step 3", ActivityKind::Impl, vec![]),
        ];
        let fc = FailOnCompensator::new("step-2");
        let mut coord = SagaFlowCoordinator::new(steps, Box::new(fc.clone()));
        for id in ["step-1", "step-2", "step-3"] {
            coord.mark_completed(id);
        }
        let err = coord.rollback("step-3", "compilation failed").unwrap_err();
        assert_eq!(fc.calls(), vec!["step-3", "step-2", "step-1"]);
        assert!(matches!(
            err,
            EngineError::CompensationIncomplete { failed: 1, .. }
        ));
    }
}
