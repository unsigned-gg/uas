//! Pure domain types for Best-of-N candidate tournaments (specs 0011, 0015, 0031).
//!
//! No I/O, no suite dependencies. Inner hexagon.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Scorecard and metrics for a single tournament candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentCandidateSummary {
    pub candidate: usize,
    pub score: u64,
    pub verified: bool,
    pub warnings: u32,
    pub diff_lines: u64,
    pub duration_ms: u64,
}

/// Recorded outcome of a Best-of-N candidate tournament.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentSummary {
    pub run_id: String,
    #[serde(default)]
    pub flow: Option<String>,
    pub task_id: String,
    pub at: DateTime<Utc>,
    pub winner_index: usize,
    pub winner_score: u64,
    pub rollout_n: usize,
    pub detail: String,
    pub ranking: Vec<TournamentCandidateSummary>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_tournament_summary() {
        let summary = TournamentSummary {
            run_id: "test-run".into(),
            flow: Some("ship-fix".into()),
            task_id: "step-1".into(),
            at: Utc::now(),
            winner_index: 0,
            winner_score: 18446744073709551615,
            rollout_n: 2,
            detail: "candidate 0 won".into(),
            ranking: vec![
                TournamentCandidateSummary {
                    candidate: 0,
                    score: 18446744073709551615,
                    verified: true,
                    warnings: 0,
                    diff_lines: 5,
                    duration_ms: 1200,
                },
                TournamentCandidateSummary {
                    candidate: 1,
                    score: 0,
                    verified: false,
                    warnings: 2,
                    diff_lines: 10,
                    duration_ms: 1500,
                },
            ],
        };

        let json = serde_json::to_string(&summary).unwrap();
        let parsed: TournamentSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, summary);
    }
}
