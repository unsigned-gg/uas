//! Saga flow execution coordinator and compensation runner.
//!
//! Bridges `.saga/flows/*.toml` definitions with [`crate::WorkflowSession`] executions,
//! enforcing compensators in reverse order upon step failure.

use crate::{Activity, ActivityKind, Compensator, EngineError};

/// A step definition within a Saga flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SagaStep {
    /// Step identifier (e.g. "custody-check", "spec-check", "operator-gate").
    pub id: String,
    /// Step title.
    pub title: String,
    /// Kind of activity.
    pub kind: ActivityKind,
    /// Prerequisites steps that must complete before this step runs.
    pub deps: Vec<String>,
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

    /// Trigger compensations for all completed steps in reverse order.
    pub fn rollback(&self, failed_step: &str, reason: &str) -> Result<(), EngineError> {
        tracing::warn!(failed_step, reason, "triggering saga compensations");
        for step_id in self.completed.iter().rev() {
            if let Some(step) = self.steps.iter().find(|s| &s.id == step_id) {
                let activity = Activity::new(&step.id, &step.title, step.kind);
                self.compensator.compensate(&activity, reason)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensator::RecordingCompensator;

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

        coord.rollback("step-3", "compilation failed").unwrap();

        let recorded = rec.recorded();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].0, "step-2");
        assert_eq!(recorded[1].0, "step-1");
    }
}
