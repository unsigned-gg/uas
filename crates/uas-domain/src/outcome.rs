//! How a worker ended, and the cgroup limit counters behind it.

use serde::{Deserialize, Serialize};

/// Why the kernel killed a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KillReason {
    /// The slot cgroup hit `memory.max` and the OOM killer fired.
    Oom,
}

/// How a worker ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerOutcome {
    /// The worker exited by itself with this code.
    Exited(i32),
    /// The kernel killed the worker.
    Killed(KillReason),
}

/// Slot cgroup limit counters read at `Reclaiming`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitEvents {
    /// Count of `oom_kill` in `memory.events`.
    pub oom_kill: u64,
    /// Count of `max` in `pids.events`: forks refused at `pids.max`.
    pub pids_max_hits: u64,
}

impl WorkerOutcome {
    /// Classify an exit. A non-zero `oom_kill` is `Killed(Oom)`. Every other
    /// case keeps the exit code. `pids_max_hits` never changes the result,
    /// because `pids.max` refuses forks and kills nothing.
    #[must_use]
    pub fn classify(exit_code: i32, events: &LimitEvents) -> Self {
        if events.oom_kill > 0 {
            Self::Killed(KillReason::Oom)
        } else {
            Self::Exited(exit_code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_oom_kill_is_killed_oom() {
        let ev = LimitEvents {
            oom_kill: 1,
            pids_max_hits: 0,
        };
        assert_eq!(
            WorkerOutcome::classify(137, &ev),
            WorkerOutcome::Killed(KillReason::Oom)
        );
    }

    #[test]
    fn outcome_zero_oom_keeps_exit_code() {
        assert_eq!(
            WorkerOutcome::classify(137, &LimitEvents::default()),
            WorkerOutcome::Exited(137)
        );
    }

    #[test]
    fn outcome_pids_hits_change_nothing() {
        let ev = LimitEvents {
            oom_kill: 0,
            pids_max_hits: 5,
        };
        assert_eq!(WorkerOutcome::classify(1, &ev), WorkerOutcome::Exited(1));
    }

    #[test]
    fn outcome_serde_round_trip() {
        for o in [
            WorkerOutcome::Exited(3),
            WorkerOutcome::Killed(KillReason::Oom),
        ] {
            let s = serde_json::to_string(&o).unwrap();
            assert_eq!(serde_json::from_str::<WorkerOutcome>(&s).unwrap(), o);
        }
        let ev = LimitEvents {
            oom_kill: 2,
            pids_max_hits: 7,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(serde_json::from_str::<LimitEvents>(&s).unwrap(), ev);
    }
}
