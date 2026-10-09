//! Pure domain types for run catalog indexing and choice menus (specs 0023, 0031).
//!
//! No I/O, no suite dependencies. Inner hexagon.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Catalog item representing a recorded run for interactive choice and inspection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunCatalogEntry {
    pub run_id: String,
    #[serde(default)]
    pub flow: Option<String>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub task_states: BTreeMap<String, usize>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub has_tournaments: bool,
    #[serde(default)]
    pub tokens: Option<u64>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

impl RunCatalogEntry {
    /// Formatted status summary (e.g. "done: 4, working: 1").
    pub fn status_summary(&self) -> String {
        if self.task_states.is_empty() {
            return "empty".into();
        }
        self.task_states
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_run_catalog_entry() {
        let mut task_states = BTreeMap::new();
        task_states.insert("done".into(), 3);
        let entry = RunCatalogEntry {
            run_id: "test-run".into(),
            flow: Some("ship-fix".into()),
            started_at: Some(Utc::now()),
            ended_at: None,
            task_states,
            last_error: None,
            has_tournaments: true,
            tokens: Some(42000),
            cost_usd: Some(0.12),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: RunCatalogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, entry);
        assert_eq!(entry.status_summary(), "done:3");
    }
}
