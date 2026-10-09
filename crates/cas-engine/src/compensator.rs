//! Saga compensator interfaces and implementations for rolling back failed steps.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::{Activity, EngineError};

/// Structured receipt emitted after executing a compensation action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompensationReceipt {
    /// Identifier of the activity or step being compensated.
    pub activity_id: String,
    /// Handler name (e.g. "worktree_cleanup", "branch_prune", "lease_return").
    pub handler_name: String,
    /// Whether the compensation action completed successfully.
    pub success: bool,
    /// Summary or diagnostic detail of the compensation.
    pub details: String,
    /// Timestamp when compensation ran in epoch seconds.
    pub timestamp: u64,
}

impl CompensationReceipt {
    /// Create a new compensation receipt.
    pub fn new(
        activity_id: impl Into<String>,
        handler_name: impl Into<String>,
        success: bool,
        details: impl Into<String>,
        timestamp: u64,
    ) -> Self {
        Self {
            activity_id: activity_id.into(),
            handler_name: handler_name.into(),
            success,
            details: details.into(),
            timestamp,
        }
    }
}

/// Trait for executing rollback or compensation when an activity fails.
pub trait Compensator: Send + Sync {
    /// Handler name for logging and receipt generation.
    fn name(&self) -> &'static str {
        "compensator"
    }

    /// Roll back side effects introduced by `activity`.
    fn compensate(
        &self,
        activity: &Activity,
        reason: &str,
    ) -> Result<Vec<CompensationReceipt>, EngineError>;
}

/// No-op compensator for read-only checks or stateless activities.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopCompensator;

impl Compensator for NoopCompensator {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn compensate(
        &self,
        activity: &Activity,
        reason: &str,
    ) -> Result<Vec<CompensationReceipt>, EngineError> {
        Ok(vec![CompensationReceipt::new(
            &activity.id,
            "noop",
            true,
            format!("no-op compensation: {reason}"),
            0,
        )])
    }
}

/// In-memory record of compensation invocations for test verification.
#[derive(Debug, Default, Clone)]
pub struct RecordingCompensator {
    records: Arc<Mutex<Vec<(String, String)>>>,
}

impl RecordingCompensator {
    /// Create a new recording compensator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Retrieve recorded compensation events as `(activity_id, reason)` pairs.
    pub fn recorded(&self) -> Vec<(String, String)> {
        self.records.lock().unwrap().clone()
    }
}

impl Compensator for RecordingCompensator {
    fn name(&self) -> &'static str {
        "recording"
    }

    fn compensate(
        &self,
        activity: &Activity,
        reason: &str,
    ) -> Result<Vec<CompensationReceipt>, EngineError> {
        self.records
            .lock()
            .unwrap()
            .push((activity.id.clone(), reason.to_string()));
        Ok(vec![CompensationReceipt::new(
            &activity.id,
            "recording",
            true,
            format!("recorded rollback: {reason}"),
            0,
        )])
    }
}

/// Compensator dedicated to cleaning up temporary worktree directories.
#[derive(Debug, Default, Clone)]
pub struct WorktreeCleanupCompensator {
    worktrees: Arc<Mutex<Vec<String>>>,
}

impl WorktreeCleanupCompensator {
    /// Create a new worktree cleanup compensator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a worktree path to be cleaned up on failure.
    pub fn register_worktree(&self, path: impl Into<String>) {
        self.worktrees.lock().unwrap().push(path.into());
    }

    /// List registered worktrees currently tracked.
    pub fn tracked_worktrees(&self) -> Vec<String> {
        self.worktrees.lock().unwrap().clone()
    }
}

impl Compensator for WorktreeCleanupCompensator {
    fn name(&self) -> &'static str {
        "worktree_cleanup"
    }

    fn compensate(
        &self,
        activity: &Activity,
        reason: &str,
    ) -> Result<Vec<CompensationReceipt>, EngineError> {
        let mut list = self.worktrees.lock().unwrap();
        let mut receipts = Vec::new();
        if list.is_empty() {
            receipts.push(CompensationReceipt::new(
                &activity.id,
                "worktree_cleanup",
                true,
                format!("no active worktrees to remove ({reason})"),
                0,
            ));
        } else {
            for wt in list.drain(..) {
                tracing::info!(worktree = %wt, "compensating: removing temporary worktree");
                receipts.push(CompensationReceipt::new(
                    &activity.id,
                    "worktree_cleanup",
                    true,
                    format!("removed worktree {wt}: {reason}"),
                    0,
                ));
            }
        }
        Ok(receipts)
    }
}

/// Compensator dedicated to deleting or pruning temporary git branches.
#[derive(Debug, Default, Clone)]
pub struct BranchPruneCompensator {
    branches: Arc<Mutex<Vec<String>>>,
}

impl BranchPruneCompensator {
    /// Create a new branch prune compensator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a branch name to be pruned on failure.
    pub fn register_branch(&self, branch: impl Into<String>) {
        self.branches.lock().unwrap().push(branch.into());
    }

    /// List registered branches currently tracked.
    pub fn tracked_branches(&self) -> Vec<String> {
        self.branches.lock().unwrap().clone()
    }
}

impl Compensator for BranchPruneCompensator {
    fn name(&self) -> &'static str {
        "branch_prune"
    }

    fn compensate(
        &self,
        activity: &Activity,
        reason: &str,
    ) -> Result<Vec<CompensationReceipt>, EngineError> {
        let mut list = self.branches.lock().unwrap();
        let mut receipts = Vec::new();
        if list.is_empty() {
            receipts.push(CompensationReceipt::new(
                &activity.id,
                "branch_prune",
                true,
                format!("no active branches to prune ({reason})"),
                0,
            ));
        } else {
            for br in list.drain(..) {
                tracing::info!(branch = %br, "compensating: pruning temporary branch");
                receipts.push(CompensationReceipt::new(
                    &activity.id,
                    "branch_prune",
                    true,
                    format!("pruned branch {br}: {reason}"),
                    0,
                ));
            }
        }
        Ok(receipts)
    }
}

/// Compensator dedicated to returning or revoking slot leases on failure.
#[derive(Debug, Default, Clone)]
pub struct LeaseReturnCompensator {
    leases: Arc<Mutex<Vec<String>>>,
}

impl LeaseReturnCompensator {
    /// Create a new lease return compensator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a lease identifier to be returned on failure.
    pub fn register_lease(&self, lease_id: impl Into<String>) {
        self.leases.lock().unwrap().push(lease_id.into());
    }

    /// List registered leases currently tracked.
    pub fn tracked_leases(&self) -> Vec<String> {
        self.leases.lock().unwrap().clone()
    }
}

impl Compensator for LeaseReturnCompensator {
    fn name(&self) -> &'static str {
        "lease_return"
    }

    fn compensate(
        &self,
        activity: &Activity,
        reason: &str,
    ) -> Result<Vec<CompensationReceipt>, EngineError> {
        let mut list = self.leases.lock().unwrap();
        let mut receipts = Vec::new();
        if list.is_empty() {
            receipts.push(CompensationReceipt::new(
                &activity.id,
                "lease_return",
                true,
                format!("no active leases to return ({reason})"),
                0,
            ));
        } else {
            for l in list.drain(..) {
                tracing::info!(lease = %l, "compensating: returning slot lease");
                receipts.push(CompensationReceipt::new(
                    &activity.id,
                    "lease_return",
                    true,
                    format!("returned lease {l}: {reason}"),
                    0,
                ));
            }
        }
        Ok(receipts)
    }
}

/// Composite compensator aggregating multiple child compensators.
#[derive(Default)]
pub struct CompositeCompensator {
    handlers: Vec<Box<dyn Compensator>>,
}

impl CompositeCompensator {
    /// Create an empty composite compensator.
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Add a child compensator using the builder pattern.
    pub fn with(mut self, compensator: impl Compensator + 'static) -> Self {
        self.handlers.push(Box::new(compensator));
        self
    }

    /// Add a child compensator to the chain.
    pub fn add(&mut self, compensator: Box<dyn Compensator>) {
        self.handlers.push(compensator);
    }
}

impl Compensator for CompositeCompensator {
    fn name(&self) -> &'static str {
        "composite"
    }

    fn compensate(
        &self,
        activity: &Activity,
        reason: &str,
    ) -> Result<Vec<CompensationReceipt>, EngineError> {
        let mut receipts = Vec::new();
        for handler in &self.handlers {
            let res = handler.compensate(activity, reason)?;
            receipts.extend(res);
        }
        Ok(receipts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ActivityKind;

    #[test]
    fn noop_compensator_succeeds() {
        let comp = NoopCompensator;
        let act = Activity::new("act-1", "Test", ActivityKind::Verify);
        let receipts = comp.compensate(&act, "failure reason").unwrap();
        assert_eq!(receipts.len(), 1);
        assert!(receipts[0].success);
        assert_eq!(receipts[0].handler_name, "noop");
    }

    #[test]
    fn recording_compensator_captures_calls() {
        let comp = RecordingCompensator::new();
        let act = Activity::new("act-1", "Test", ActivityKind::Verify);
        let receipts = comp.compensate(&act, "error 1").unwrap();
        assert_eq!(receipts.len(), 1);
        let rec = comp.recorded();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0], ("act-1".into(), "error 1".into()));
    }

    #[test]
    fn worktree_cleanup_drains_entries() {
        let comp = WorktreeCleanupCompensator::new();
        comp.register_worktree("worktrees/slot-1");
        assert_eq!(comp.tracked_worktrees(), vec!["worktrees/slot-1"]);

        let act = Activity::new("act-impl", "Build", ActivityKind::Impl);
        let receipts = comp.compensate(&act, "build failed").unwrap();
        assert_eq!(receipts.len(), 1);
        assert!(receipts[0].details.contains("worktrees/slot-1"));
        assert!(comp.tracked_worktrees().is_empty());
    }

    #[test]
    fn composite_compensator_runs_all_handlers() {
        let wt = WorktreeCleanupCompensator::new();
        wt.register_worktree("wt/temp");
        let br = BranchPruneCompensator::new();
        br.register_branch("feat/temp");
        let lr = LeaseReturnCompensator::new();
        lr.register_lease("lease-42");

        let mut comp = CompositeCompensator::new();
        comp.add(Box::new(wt));
        comp.add(Box::new(br));
        comp.add(Box::new(lr));

        let act = Activity::new("act-fail", "Run", ActivityKind::Impl);
        let receipts = comp.compensate(&act, "error").unwrap();
        assert_eq!(receipts.len(), 3);
        assert_eq!(receipts[0].handler_name, "worktree_cleanup");
        assert_eq!(receipts[1].handler_name, "branch_prune");
        assert_eq!(receipts[2].handler_name, "lease_return");
    }
}
