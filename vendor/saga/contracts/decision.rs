//! Decision audit record from völva.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A typed decision recorded into run events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub question: String,
    pub answer: String,
    pub confidence: f64,
    pub probabilities: BTreeMap<String, f64>,
    pub at: DateTime<Utc>,
}

use std::collections::BTreeMap;
