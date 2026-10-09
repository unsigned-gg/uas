//! RunId and StepId newtypes.

use std::fmt;
use std::str::FromStr;

/// Run identifier: `<flow>-<yyyymmdd-hhmmss>-<hex4>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RunId(pub String);

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for RunId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("RunId cannot be empty".into());
        }
        Ok(RunId(s.into()))
    }
}

/// Step identifier: kebab-case, non-empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct StepId(pub String);

impl fmt::Display for StepId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for StepId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("StepId cannot be empty".into());
        }
        let bytes = s.as_bytes();
        if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
            return Err("StepId must start with a lowercase letter or digit".into());
        }
        for &b in bytes {
            if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
                return Err("StepId must be kebab-case (lowercase, digits, hyphens only)".into());
            }
        }
        Ok(StepId(s.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_id_round_trips() {
        let id = RunId("review-loop-20261007-120000-a1b2".into());
        assert_eq!(id.to_string(), "review-loop-20261007-120000-a1b2");
        assert_eq!(
            RunId::from_str("review-loop-20261007-120000-a1b2")
                .unwrap()
                .0,
            "review-loop-20261007-120000-a1b2"
        );
    }

    #[test]
    fn step_id_rejects_empty_and_uppercase() {
        assert!(StepId::from_str("").is_err());
        assert!(StepId::from_str("Hello").is_err());
        assert!(StepId::from_str("hello-world").is_ok());
        assert!(StepId::from_str("hello_world").is_err());
        assert!(StepId::from_str("-hello").is_err());
    }
}
