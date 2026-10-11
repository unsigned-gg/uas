// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Critical Path Method (CPM) scheduler for Saga workflows.
//!
//! Computes earliest/latest start and finish times, calculates total float
//! slack, detects dependency cycles, and identifies critical path bottleneck activities.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{EngineError, SagaStep};

/// Schedule bounds and critical status for an individual workflow step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpmStepSchedule {
    /// Step identifier.
    pub id: String,
    /// Estimated or effective duration units.
    pub duration: u64,
    /// Earliest Start time (ES).
    pub es: u64,
    /// Earliest Finish time (EF = ES + duration).
    pub ef: u64,
    /// Latest Start time (LS = LF - duration).
    pub ls: u64,
    /// Latest Finish time (LF).
    pub lf: u64,
    /// Total float slack (Slack = LS - ES = LF - EF).
    pub slack: u64,
    /// Whether this step is on the critical path (slack == 0).
    pub is_critical: bool,
}

/// Complete Critical Path Method (CPM) schedule for a workflow DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpmSchedule {
    /// Step schedules keyed by step ID.
    pub steps: BTreeMap<String, CpmStepSchedule>,
    /// Total duration along the critical path.
    pub total_duration: u64,
    /// Sequence of step identifiers forming the primary critical path.
    pub critical_path: Vec<String>,
}

impl CpmSchedule {
    /// Compute the CPM schedule across a set of [`SagaStep`]s.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::InvalidFlow`] if:
    /// - Any step ID is empty or duplicated.
    /// - Any step references a nonexistent dependency.
    /// - The dependency graph contains a cycle.
    pub fn compute(steps: &[SagaStep]) -> Result<Self, EngineError> {
        if steps.is_empty() {
            return Ok(Self {
                steps: BTreeMap::new(),
                total_duration: 0,
                critical_path: Vec::new(),
            });
        }

        // 1. Validation: check uniqueness and non-empty IDs.
        let mut step_map: BTreeMap<String, &SagaStep> = BTreeMap::new();
        for step in steps {
            if step.id.trim().is_empty() {
                return Err(EngineError::InvalidFlow {
                    reason: "step ID cannot be empty".into(),
                });
            }
            if step_map.insert(step.id.clone(), step).is_some() {
                return Err(EngineError::InvalidFlow {
                    reason: format!("duplicate step ID: {}", step.id),
                });
            }
        }

        // Check for unknown dependencies and self-loops.
        for step in steps {
            for dep in &step.deps {
                if dep == &step.id {
                    return Err(EngineError::InvalidFlow {
                        reason: format!("step {} cannot depend on itself", step.id),
                    });
                }
                if !step_map.contains_key(dep) {
                    return Err(EngineError::InvalidFlow {
                        reason: format!("step {} references unknown dependency: {}", step.id, dep),
                    });
                }
            }
        }

        // 2. Build adjacency structures.
        // In this DAG: edge u -> v means u must complete before v starts (u is in v.deps).
        let mut preds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut succs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut in_degree: BTreeMap<String, usize> = BTreeMap::new();

        for step in steps {
            preds.insert(step.id.clone(), step.deps.iter().cloned().collect());
            succs.entry(step.id.clone()).or_default();
            in_degree.insert(step.id.clone(), step.deps.len());
        }

        for step in steps {
            for dep in &step.deps {
                succs
                    .entry(dep.clone())
                    .or_default()
                    .insert(step.id.clone());
            }
        }

        // 3. Topological sort using Kahn's algorithm (with deterministic ordering).
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut topo_order: Vec<String> = Vec::with_capacity(steps.len());

        while let Some(u) = queue.pop_front() {
            topo_order.push(u.clone());

            if let Some(next_steps) = succs.get(&u) {
                // Collect and sort to preserve determinism
                let mut sorted_next: Vec<String> = next_steps.iter().cloned().collect();
                sorted_next.sort();

                for v in sorted_next {
                    let deg = in_degree.get_mut(&v).expect("in-degree entry exists");
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(v);
                    }
                }
            }
        }

        if topo_order.len() != steps.len() {
            return Err(EngineError::InvalidFlow {
                reason: "dependency cycle detected in saga steps".into(),
            });
        }

        // 4. Forward Pass: compute Earliest Start (ES) and Earliest Finish (EF).
        let mut es_map: BTreeMap<String, u64> = BTreeMap::new();
        let mut ef_map: BTreeMap<String, u64> = BTreeMap::new();

        for id in &topo_order {
            let step = step_map[id];
            let duration = step.effective_duration();

            let es = preds[id].iter().map(|p| ef_map[p]).max().unwrap_or(0);

            let ef = es + duration;
            es_map.insert(id.clone(), es);
            ef_map.insert(id.clone(), ef);
        }

        let total_duration = ef_map.values().max().copied().unwrap_or(0);

        // 5. Backward Pass: compute Latest Finish (LF) and Latest Start (LS).
        let mut lf_map: BTreeMap<String, u64> = BTreeMap::new();
        let mut ls_map: BTreeMap<String, u64> = BTreeMap::new();

        for id in topo_order.iter().rev() {
            let step = step_map[id];
            let duration = step.effective_duration();

            let lf = succs[id]
                .iter()
                .map(|s| ls_map[s])
                .min()
                .unwrap_or(total_duration);

            let ls = lf.saturating_sub(duration);
            lf_map.insert(id.clone(), lf);
            ls_map.insert(id.clone(), ls);
        }

        // 6. Compute Slack and identify critical steps.
        let mut schedule_steps: BTreeMap<String, CpmStepSchedule> = BTreeMap::new();

        for (id, step) in &step_map {
            let duration = step.effective_duration();
            let es = es_map[id];
            let ef = ef_map[id];
            let ls = ls_map[id];
            let lf = lf_map[id];
            let slack = ls.saturating_sub(es);
            let is_critical = slack == 0;

            schedule_steps.insert(
                id.clone(),
                CpmStepSchedule {
                    id: id.clone(),
                    duration,
                    es,
                    ef,
                    ls,
                    lf,
                    slack,
                    is_critical,
                },
            );
        }

        // 7. Extract Primary Critical Path: trace from entry to exit where is_critical is true.
        let mut critical_path = Vec::new();
        let mut curr_entry: Option<String> = schedule_steps
            .values()
            .filter(|s| s.is_critical && s.es == 0)
            .min_by_key(|s| &s.id)
            .map(|s| s.id.clone());

        while let Some(curr_id) = curr_entry {
            critical_path.push(curr_id.clone());
            let curr_sched = &schedule_steps[&curr_id];

            // Next critical step must have ES == current.EF and be a successor
            curr_entry = succs[&curr_id]
                .iter()
                .filter(|next_id| {
                    let next_sched = &schedule_steps[*next_id];
                    next_sched.is_critical && next_sched.es == curr_sched.ef
                })
                .min()
                .cloned();
        }

        Ok(Self {
            steps: schedule_steps,
            total_duration,
            critical_path,
        })
    }

    /// Retrieve all step schedules marked as critical (zero float slack).
    pub fn critical_steps(&self) -> Vec<&CpmStepSchedule> {
        self.steps.values().filter(|s| s.is_critical).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ActivityKind;

    #[test]
    fn cpm_linear_pipeline() {
        let steps = vec![
            SagaStep::new("custody", "Check custody", ActivityKind::Verify, vec![])
                .with_duration(10),
            SagaStep::new(
                "spec",
                "Spec check",
                ActivityKind::Verify,
                vec!["custody".into()],
            )
            .with_duration(15),
            SagaStep::new(
                "build",
                "Build workspace",
                ActivityKind::Impl,
                vec!["spec".into()],
            )
            .with_duration(25),
        ];

        let sched = CpmSchedule::compute(&steps).unwrap();
        assert_eq!(sched.total_duration, 50);
        assert_eq!(sched.critical_path, vec!["custody", "spec", "build"]);

        let custody = &sched.steps["custody"];
        assert_eq!(custody.es, 0);
        assert_eq!(custody.ef, 10);
        assert_eq!(custody.ls, 0);
        assert_eq!(custody.lf, 10);
        assert_eq!(custody.slack, 0);
        assert!(custody.is_critical);

        let build = &sched.steps["build"];
        assert_eq!(build.es, 25);
        assert_eq!(build.ef, 50);
        assert_eq!(build.slack, 0);
        assert!(build.is_critical);
    }

    #[test]
    fn cpm_parallel_branches_identifies_critical_and_slack() {
        // Graph:
        //        -> branch_fast (dur=5)  ->
        // start (dur=10)                    merge (dur=10)
        //        -> branch_slow (dur=30) ->
        let steps = vec![
            SagaStep::new("start", "Start", ActivityKind::Verify, vec![]).with_duration(10),
            SagaStep::new(
                "fast",
                "Fast branch",
                ActivityKind::Verify,
                vec!["start".into()],
            )
            .with_duration(5),
            SagaStep::new(
                "slow",
                "Slow branch",
                ActivityKind::Impl,
                vec!["start".into()],
            )
            .with_duration(30),
            SagaStep::new(
                "merge",
                "Merge gate",
                ActivityKind::Gate,
                vec!["fast".into(), "slow".into()],
            )
            .with_duration(10),
        ];

        let sched = CpmSchedule::compute(&steps).unwrap();
        assert_eq!(sched.total_duration, 50);
        assert_eq!(sched.critical_path, vec!["start", "slow", "merge"]);

        let fast = &sched.steps["fast"];
        assert_eq!(fast.es, 10);
        assert_eq!(fast.ef, 15);
        assert_eq!(fast.lf, 40);
        assert_eq!(fast.ls, 35);
        assert_eq!(fast.slack, 25);
        assert!(!fast.is_critical);

        let slow = &sched.steps["slow"];
        assert_eq!(slow.es, 10);
        assert_eq!(slow.ef, 40);
        assert_eq!(slow.ls, 10);
        assert_eq!(slow.lf, 40);
        assert_eq!(slow.slack, 0);
        assert!(slow.is_critical);
    }

    #[test]
    fn cpm_detects_cycle() {
        let steps = vec![
            SagaStep::new("a", "A", ActivityKind::Verify, vec!["b".into()]),
            SagaStep::new("b", "B", ActivityKind::Verify, vec!["a".into()]),
        ];

        let err = CpmSchedule::compute(&steps).unwrap_err();
        assert!(matches!(err, EngineError::InvalidFlow { .. }));
    }

    #[test]
    fn cpm_detects_unknown_dependency() {
        let steps = vec![SagaStep::new(
            "a",
            "A",
            ActivityKind::Verify,
            vec!["ghost".into()],
        )];

        let err = CpmSchedule::compute(&steps).unwrap_err();
        assert!(matches!(err, EngineError::InvalidFlow { .. }));
    }
}
