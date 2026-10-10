//! Agent Sprint Wave Strategy for UAS orchestration.
//!
//! Partitions DAG steps into sequential execution waves bounded by a concurrency cap
//! (default 3, matching estate host-throttling limits). Prioritizes zero-float
//! critical path steps, and assigns agent roles, model tiers, and workspace isolation modes.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::compensator::{finish_rollback, receipts_or_failure};
use crate::{
    Activity, ActivityKind, CompensationReceipt, Compensator, CpmSchedule, CpmStepSchedule,
    EngineError, SagaStep,
};

/// Default maximum concurrent subagents per wave (per estate Ceres host-throttling rule).
pub const DEFAULT_MAX_WAVE_CONCURRENCY: usize = 3;

/// Maximum allowed lines in an agent subagent verdict (per estate rules).
pub const MAX_VERDICT_LINES: usize = 15;

/// Assigned agent role according to estate guidelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    /// Read-only mechanical checks, probes, and verification.
    Probe,
    /// Bounded builder executing implementation tasks in virtual workspace.
    Builder,
    /// Adversarial critic reviewing builder outputs.
    Critic,
    /// Main session operator gate (landing, secrets, approval). Subagents never execute this.
    MainSession,
}

/// Model capability tier assigned to an agent role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelTier {
    /// Lightweight/fast model for probes and mechanical checks.
    Fast,
    /// Capable/pro model for bounded builders and critics.
    Pro,
    /// Main session interactive or operator context.
    Main,
}

/// Workspace isolation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceMode {
    /// Read-only shared repository surface (no physical disk duplication).
    ReadOnly,
    /// Virtual DeltaDB or shared worktree with delta tracking.
    VirtualShare,
    /// Main session host workspace for gated and landing actions.
    MainHost,
}

/// An individual step assigned to an execution wave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveStep {
    /// Original saga step definition.
    pub step: SagaStep,
    /// CPM schedule analysis for this step.
    pub cpm: CpmStepSchedule,
    /// Assigned agent role.
    pub role: AgentRole,
    /// Assigned model tier.
    pub model_tier: ModelTier,
    /// Assigned workspace isolation mode.
    pub workspace_mode: WorkspaceMode,
}

/// A concurrent execution wave of up to `max_concurrency` steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionWave {
    /// 0-indexed wave number.
    pub wave_index: usize,
    /// Steps assigned to run concurrently in this wave.
    pub steps: Vec<WaveStep>,
}

impl ExecutionWave {
    /// Check whether any step in this wave requires main-session operator gate.
    pub fn is_gated(&self) -> bool {
        self.steps
            .iter()
            .any(|s| s.role == AgentRole::MainSession || s.step.kind == ActivityKind::Gate)
    }

    /// List all step IDs in this wave.
    pub fn step_ids(&self) -> Vec<&str> {
        self.steps.iter().map(|s| s.step.id.as_str()).collect()
    }
}

/// A complete sprint wave execution plan for a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WavePlan {
    /// Ordered sequence of execution waves.
    pub waves: Vec<ExecutionWave>,
    /// Underlying CPM schedule.
    pub cpm: CpmSchedule,
    /// Max concurrency constraint used for planning.
    pub max_concurrency: usize,
}

impl WavePlan {
    /// Generate an optimal sprint wave execution plan using CPM critical path priority.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::InvalidFlow`] if the flow contains cycles or unresolved dependencies.
    pub fn plan(steps: &[SagaStep], max_concurrency: usize) -> Result<Self, EngineError> {
        let max_concurrency = if max_concurrency == 0 {
            DEFAULT_MAX_WAVE_CONCURRENCY
        } else {
            max_concurrency
        };

        let cpm = CpmSchedule::compute(steps)?;

        if steps.is_empty() {
            return Ok(Self {
                waves: Vec::new(),
                cpm,
                max_concurrency,
            });
        }

        let mut assigned: BTreeSet<String> = BTreeSet::new();
        let mut waves: Vec<ExecutionWave> = Vec::new();
        let total_steps = steps.len();

        while assigned.len() < total_steps {
            // Find all unassigned steps whose dependencies have all been assigned in previous waves.
            let mut eligible: Vec<SagaStep> = steps
                .iter()
                .filter(|s| !assigned.contains(&s.id))
                .filter(|s| s.deps.iter().all(|dep| assigned.contains(dep)))
                .cloned()
                .collect();

            if eligible.is_empty() {
                return Err(EngineError::InvalidFlow {
                    reason:
                        "deadlock in wave scheduling: unassigned steps exist but none are eligible"
                            .into(),
                });
            }

            // Sort eligible steps by Critical Path Priority:
            // 1. Critical path steps first (is_critical == true)
            // 2. Smallest slack (tightest float)
            // 3. Lowest Earliest Start (ES)
            // 4. Deterministic step ID
            eligible.sort_by(|a, b| {
                let a_sched = &cpm.steps[&a.id];
                let b_sched = &cpm.steps[&b.id];

                b_sched
                    .is_critical
                    .cmp(&a_sched.is_critical)
                    .then_with(|| a_sched.slack.cmp(&b_sched.slack))
                    .then_with(|| a_sched.es.cmp(&b_sched.es))
                    .then_with(|| a.id.cmp(&b.id))
            });

            // Select up to `max_concurrency` steps for the current wave
            let wave_steps_raw: Vec<SagaStep> =
                eligible.into_iter().take(max_concurrency).collect();

            let wave_steps: Vec<WaveStep> = wave_steps_raw
                .into_iter()
                .map(|step| {
                    let sched = cpm.steps[&step.id].clone();
                    let (role, model_tier, workspace_mode) = classify_step(&step);
                    WaveStep {
                        step,
                        cpm: sched,
                        role,
                        model_tier,
                        workspace_mode,
                    }
                })
                .collect();

            for ws in &wave_steps {
                assigned.insert(ws.step.id.clone());
            }

            let wave_index = waves.len();
            waves.push(ExecutionWave {
                wave_index,
                steps: wave_steps,
            });
        }

        // Sanity check
        let total_assigned: usize = waves.iter().map(|w| w.steps.len()).sum();
        if total_assigned != total_steps {
            return Err(EngineError::InvalidFlow {
                reason: format!(
                    "wave planning incomplete: {} assigned of {} steps",
                    total_assigned, total_steps
                ),
            });
        }

        Ok(Self {
            waves,
            cpm,
            max_concurrency,
        })
    }

    /// Total number of waves in this plan.
    pub fn total_waves(&self) -> usize {
        self.waves.len()
    }

    /// Total number of steps across all waves.
    pub fn step_count(&self) -> usize {
        self.waves.iter().map(|w| w.steps.len()).sum()
    }
}

/// Classify a step into its agent role, model tier, and workspace mode.
pub fn classify_step(step: &SagaStep) -> (AgentRole, ModelTier, WorkspaceMode) {
    if step.kind == ActivityKind::Gate || step.gate.is_some() {
        return (
            AgentRole::MainSession,
            ModelTier::Main,
            WorkspaceMode::MainHost,
        );
    }

    if let Some(agent) = &step.agent {
        match agent.to_ascii_lowercase().as_str() {
            "probe" | "fast" | "lint" => {
                return (AgentRole::Probe, ModelTier::Fast, WorkspaceMode::ReadOnly)
            }
            "critic" | "review" => {
                return (AgentRole::Critic, ModelTier::Pro, WorkspaceMode::ReadOnly)
            }
            "builder" | "impl" => {
                return (
                    AgentRole::Builder,
                    ModelTier::Pro,
                    WorkspaceMode::VirtualShare,
                )
            }
            "main" | "operator" => {
                return (
                    AgentRole::MainSession,
                    ModelTier::Main,
                    WorkspaceMode::MainHost,
                )
            }
            _ => {}
        }
    }

    match step.kind {
        ActivityKind::Verify => {
            let id_lower = step.id.to_ascii_lowercase();
            let title_lower = step.title.to_ascii_lowercase();
            if id_lower.contains("review")
                || id_lower.contains("critic")
                || title_lower.contains("review")
                || title_lower.contains("critic")
            {
                (AgentRole::Critic, ModelTier::Pro, WorkspaceMode::ReadOnly)
            } else {
                (AgentRole::Probe, ModelTier::Fast, WorkspaceMode::ReadOnly)
            }
        }
        ActivityKind::Impl => (
            AgentRole::Builder,
            ModelTier::Pro,
            WorkspaceMode::VirtualShare,
        ),
        ActivityKind::Gate => (
            AgentRole::MainSession,
            ModelTier::Main,
            WorkspaceMode::MainHost,
        ),
    }
}

/// Validate that a subagent's verdict complies with the <= 15 lines rule.
pub fn validate_verdict(verdict: &str) -> Result<(), EngineError> {
    let line_count = verdict.lines().count();
    if line_count > MAX_VERDICT_LINES {
        return Err(EngineError::InvalidFlow {
            reason: format!(
                "subagent verdict exceeded max {} lines (got {} lines)",
                MAX_VERDICT_LINES, line_count
            ),
        });
    }
    Ok(())
}

/// Coordinator running an orchestrated wave plan with compensator rollback.
pub struct WaveCoordinator {
    plan: WavePlan,
    current_wave_idx: usize,
    completed_steps: Vec<String>,
    step_verdicts: BTreeMap<String, String>,
    compensator: Box<dyn Compensator>,
}

impl WaveCoordinator {
    /// Create a new coordinator for a wave plan.
    pub fn new(plan: WavePlan, compensator: Box<dyn Compensator>) -> Self {
        Self {
            plan,
            current_wave_idx: 0,
            completed_steps: Vec::new(),
            step_verdicts: BTreeMap::new(),
            compensator,
        }
    }

    /// Reference to the underlying wave plan.
    pub fn plan(&self) -> &WavePlan {
        &self.plan
    }

    /// Current wave index.
    pub fn current_wave_idx(&self) -> usize {
        self.current_wave_idx
    }

    /// Reference to the active wave being executed, or None if finished.
    pub fn current_wave(&self) -> Option<&ExecutionWave> {
        self.plan.waves.get(self.current_wave_idx)
    }

    /// Check if all steps in the current wave have been marked completed.
    pub fn is_current_wave_completed(&self) -> bool {
        let Some(wave) = self.current_wave() else {
            return true;
        };
        wave.steps
            .iter()
            .all(|ws| self.completed_steps.contains(&ws.step.id))
    }

    /// Mark a step in the current wave as completed with an optional verdict.
    pub fn mark_step_completed(
        &mut self,
        step_id: &str,
        verdict: Option<String>,
    ) -> Result<(), EngineError> {
        if let Some(v) = &verdict {
            validate_verdict(v)?;
            self.step_verdicts.insert(step_id.to_string(), v.clone());
        }

        if !self.completed_steps.contains(&step_id.to_string()) {
            self.completed_steps.push(step_id.to_string());
        }

        Ok(())
    }

    /// Advance to the next wave once the current wave has completed.
    pub fn advance_wave(&mut self) -> Result<Option<&ExecutionWave>, EngineError> {
        if !self.is_current_wave_completed() {
            let pending: Vec<&str> = self
                .current_wave()
                .map(|w| {
                    w.steps
                        .iter()
                        .filter(|ws| !self.completed_steps.contains(&ws.step.id))
                        .map(|ws| ws.step.id.as_str())
                        .collect()
                })
                .unwrap_or_default();

            return Err(EngineError::InvalidFlow {
                reason: format!(
                    "cannot advance wave {}: steps still pending: {:?}",
                    self.current_wave_idx, pending
                ),
            });
        }

        self.current_wave_idx += 1;
        Ok(self.current_wave())
    }

    /// Check if all waves in the plan have finished.
    pub fn is_finished(&self) -> bool {
        self.current_wave_idx >= self.plan.waves.len()
    }

    /// Roll back all completed steps in reverse order upon a step failure.
    pub fn rollback(
        &self,
        failed_step: &str,
        reason: &str,
    ) -> Result<Vec<CompensationReceipt>, EngineError> {
        tracing::warn!(
            failed_step,
            reason,
            "triggering wave rollback compensations"
        );

        let mut all_receipts = Vec::new();
        // Collect step definitions lookup
        let step_map: BTreeMap<String, &SagaStep> = self
            .plan
            .waves
            .iter()
            .flat_map(|w| w.steps.iter().map(|ws| (ws.step.id.clone(), &ws.step)))
            .collect();

        for step_id in self.completed_steps.iter().rev() {
            if let Some(step) = step_map.get(step_id) {
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

    /// Access the list of completed step IDs.
    pub fn completed_steps(&self) -> &[String] {
        &self.completed_steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensator::{FailOnCompensator, RecordingCompensator};

    #[test]
    fn wave_planning_bounded_concurrency_and_cpm_priority() {
        // Step graph:
        // custody (dur=5) -> spec (dur=5)
        // Independent unblocked tasks:
        // task_crit (dur=40, on critical path to end)
        // task_side1 (dur=5)
        // task_side2 (dur=5)
        // task_side3 (dur=5)
        // task_side4 (dur=5)
        let steps = vec![
            SagaStep::new("crit1", "Critical 1", ActivityKind::Impl, vec![]).with_duration(50),
            SagaStep::new(
                "crit2",
                "Critical 2",
                ActivityKind::Impl,
                vec!["crit1".into()],
            )
            .with_duration(50),
            SagaStep::new("side1", "Side 1", ActivityKind::Verify, vec![]).with_duration(5),
            SagaStep::new("side2", "Side 2", ActivityKind::Verify, vec![]).with_duration(5),
            SagaStep::new("side3", "Side 3", ActivityKind::Verify, vec![]).with_duration(5),
            SagaStep::new("side4", "Side 4", ActivityKind::Verify, vec![]).with_duration(5),
        ];

        let plan = WavePlan::plan(&steps, 3).unwrap();

        // 6 steps total, max concurrency 3 -> requires at least 2 waves.
        assert_eq!(plan.max_concurrency, 3);
        assert_eq!(plan.step_count(), 6);

        // Wave 0 must include `crit1` because it is on the critical path (slack=0)!
        let wave0_ids = plan.waves[0].step_ids();
        assert_eq!(wave0_ids.len(), 3);
        assert!(
            wave0_ids.contains(&"crit1"),
            "Wave 0 must prioritize critical step"
        );

        // Step roles
        let crit1_step = plan.waves[0]
            .steps
            .iter()
            .find(|s| s.step.id == "crit1")
            .unwrap();
        assert_eq!(crit1_step.role, AgentRole::Builder);
        assert_eq!(crit1_step.model_tier, ModelTier::Pro);
        assert_eq!(crit1_step.workspace_mode, WorkspaceMode::VirtualShare);
    }

    #[test]
    fn wave_coordinator_advancement_and_rollback() {
        let rec = RecordingCompensator::new();
        let steps = vec![
            SagaStep::new("s1", "Step 1", ActivityKind::Verify, vec![]),
            SagaStep::new("s2", "Step 2", ActivityKind::Verify, vec![]),
            SagaStep::new(
                "s3",
                "Step 3",
                ActivityKind::Impl,
                vec!["s1".into(), "s2".into()],
            ),
        ];

        let plan = WavePlan::plan(&steps, 2).unwrap();
        assert_eq!(plan.waves.len(), 2);
        assert_eq!(plan.waves[0].step_ids(), vec!["s1", "s2"]);
        assert_eq!(plan.waves[1].step_ids(), vec!["s3"]);

        let mut coord = WaveCoordinator::new(plan, Box::new(rec.clone()));
        assert_eq!(coord.current_wave_idx(), 0);

        coord
            .mark_step_completed("s1", Some("verdict: clean".into()))
            .unwrap();
        // Trying to advance before wave is completed should fail
        assert!(coord.advance_wave().is_err());

        coord.mark_step_completed("s2", None).unwrap();
        assert!(coord.is_current_wave_completed());

        let next_wave = coord.advance_wave().unwrap();
        assert!(next_wave.is_some());
        assert_eq!(coord.current_wave_idx(), 1);

        // Now simulate failure in wave 1 step 3 -> rollback!
        let receipts = coord.rollback("s3", "build failed").unwrap();
        assert_eq!(receipts.len(), 2);
        // Completed were s1 then s2 -> rolled back in reverse order: s2 then s1
        assert_eq!(receipts[0].activity_id, "s2");
        assert_eq!(receipts[1].activity_id, "s1");
    }

    #[test]
    fn verdict_line_limit_enforced() {
        let short_verdict = "line 1\nline 2\nline 3\n";
        assert!(validate_verdict(short_verdict).is_ok());

        let long_verdict = (0..20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = validate_verdict(&long_verdict).unwrap_err();
        assert!(matches!(err, EngineError::InvalidFlow { .. }));
    }

    fn three_independent_steps() -> Vec<SagaStep> {
        vec![
            SagaStep::new("s1", "Step 1", ActivityKind::Verify, vec![]),
            SagaStep::new("s2", "Step 2", ActivityKind::Verify, vec![]),
            SagaStep::new("s3", "Step 3", ActivityKind::Impl, vec![]),
        ]
    }

    fn completed_wave_coordinator(fail_on: &str) -> (WaveCoordinator, FailOnCompensator) {
        let steps = three_independent_steps();
        let fc = FailOnCompensator::new(fail_on);
        let plan = WavePlan::plan(&steps, 3).unwrap();
        let mut coord = WaveCoordinator::new(plan, Box::new(fc.clone()));
        for id in ["s1", "s2", "s3"] {
            coord.mark_step_completed(id, None).unwrap();
        }
        (coord, fc)
    }

    #[test]
    fn rollback_all_compensations_succeed() {
        let (coord, fc) = completed_wave_coordinator("none");
        let receipts = coord.rollback("s3", "build failed").unwrap();
        assert_eq!(fc.calls(), vec!["s3", "s2", "s1"]);
        assert_eq!(receipts.len(), 3);
        assert!(receipts.iter().all(|r| r.success));
    }

    #[test]
    fn rollback_continues_after_middle_compensation_failure() {
        let (coord, fc) = completed_wave_coordinator("s2");
        let err = coord.rollback("s3", "build failed").unwrap_err();
        assert_eq!(fc.calls(), vec!["s3", "s2", "s1"]);
        match err {
            EngineError::CompensationIncomplete { failed, receipts } => {
                assert_eq!(failed, 1);
                assert_eq!(receipts.len(), 3);
                assert_eq!(receipts[1].activity_id, "s2");
                assert!(!receipts[1].success);
                assert_eq!(receipts[1].handler_name, "fail_on");
            }
            other => panic!("expected CompensationIncomplete, got {other:?}"),
        }
    }

    #[test]
    fn rollback_continues_after_first_compensation_failure() {
        let (coord, fc) = completed_wave_coordinator("s3");
        let err = coord.rollback("s3", "build failed").unwrap_err();
        assert_eq!(fc.calls(), vec!["s3", "s2", "s1"]);
        match err {
            EngineError::CompensationIncomplete { failed, receipts } => {
                assert_eq!(failed, 1);
                assert_eq!(receipts.len(), 3);
                assert!(!receipts[0].success);
            }
            other => panic!("expected CompensationIncomplete, got {other:?}"),
        }
    }
}
