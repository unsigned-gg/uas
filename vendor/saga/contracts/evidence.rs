//! Evidence tiers.

use serde::{Deserialize, Serialize};

/// Tier of confidence for a terminal outcome.
///
/// Normative mapping:
/// - `Verified` ⇔ a `verify_cmd` receipt exists and exited 0.
/// - `Reported` — actor asserted via a typed envelope.
/// - `Heuristic` — inferred from pane observation (non-empty window).
/// - `Asserted` — bare claim, no observable support.
///
/// Pane observation NEVER yields `Verified`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceTier {
    Verified,
    Reported,
    Heuristic,
    Asserted,
}
