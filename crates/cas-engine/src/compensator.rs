//! Saga compensator interfaces and implementations for rolling back failed steps.

use crate::{Activity, EngineError};

/// Trait for executing rollback or compensation when an activity fails.
pub trait Compensator: Send + Sync {
    /// Roll back side effects introduced by `activity`.
    fn compensate(&self, activity: &Activity, reason: &str) -> Result<(), EngineError>;
}

/// No-op compensator for read-only checks or stateless activities.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopCompensator;

impl Compensator for NoopCompensator {
    fn compensate(&self, _activity: &Activity, _reason: &str) -> Result<(), EngineError> {
        Ok(())
    }
}

/// In-memory record of compensation invocations for test verification.
#[derive(Debug, Default, Clone)]
pub struct RecordingCompensator {
    records: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
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
    fn compensate(&self, activity: &Activity, reason: &str) -> Result<(), EngineError> {
        self.records
            .lock()
            .unwrap()
            .push((activity.id.clone(), reason.to_string()));
        Ok(())
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
        assert!(comp.compensate(&act, "failure reason").is_ok());
    }

    #[test]
    fn recording_compensator_captures_calls() {
        let comp = RecordingCompensator::new();
        let act = Activity::new("act-1", "Test", ActivityKind::Verify);
        comp.compensate(&act, "error 1").unwrap();
        let rec = comp.recorded();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0], ("act-1".into(), "error 1".into()));
    }
}
