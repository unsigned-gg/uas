// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Agent harness provider definitions (spec 0058).
//!
//! A [`Provider`] identifies the agent harness CLI and execution runtime that drives
//! a builder, critic, or probe session within an isolated worker slot.

use core::fmt;
use core::str::FromStr;
use serde::{Deserialize, Serialize};

/// Known error returned when parsing an unrecognized provider name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown agent provider: {0}")]
pub struct ParseProviderError(pub String);

/// The agent execution harness provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// Anthropic Claude Code agent harness.
    Claude,
    /// Google DeepMind Antigravity agent CLI.
    Agy,
    /// OpenMultiPlatform (omp) agent harness.
    Omp,
    /// OpenAI Codex CLI agent harness.
    Codex,
    /// Direct LiteLLM cluster gateway runner.
    Gateway,
    /// Deterministic in-process mock harness for offline testing.
    Mock,
}

impl Provider {
    /// Returns the string representation of the provider.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Agy => "agy",
            Self::Omp => "omp",
            Self::Codex => "codex",
            Self::Gateway => "gateway",
            Self::Mock => "mock",
        }
    }

    /// Returns the canonical default model for this provider.
    #[must_use]
    pub const fn default_model(&self) -> &'static str {
        match self {
            Self::Claude => "sonnet",
            Self::Agy => "gemini-3.5-pro",
            Self::Omp => "deepseek-v4.1",
            Self::Codex => "gpt-5.5",
            Self::Gateway => "glm-5.2",
            Self::Mock => "mock-v1",
        }
    }

    /// Whether this provider is natively supported directly by the blackwall runner.
    #[must_use]
    pub const fn is_blackwall_native(&self) -> bool {
        matches!(self, Self::Claude | Self::Omp | Self::Gateway)
    }

    /// Whether this provider is a synthetic mock used for offline testing.
    #[must_use]
    pub const fn is_mock(&self) -> bool {
        matches!(self, Self::Mock)
    }

    /// Whether this provider requires an outbound LLM gateway or network access.
    #[must_use]
    pub const fn requires_network(&self) -> bool {
        !self.is_mock()
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Provider {
    type Err = ParseProviderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(Self::Claude),
            "agy" => Ok(Self::Agy),
            "omp" => Ok(Self::Omp),
            "codex" => Ok(Self::Codex),
            "gateway" => Ok(Self::Gateway),
            "mock" => Ok(Self::Mock),
            other => Err(ParseProviderError(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_roundtrip_and_display() {
        let providers = [
            (Provider::Claude, "claude", "sonnet", true),
            (Provider::Agy, "agy", "gemini-3.5-pro", true),
            (Provider::Omp, "omp", "deepseek-v4.1", true),
            (Provider::Codex, "codex", "gpt-5.5", true),
            (Provider::Gateway, "gateway", "glm-5.2", true),
            (Provider::Mock, "mock", "mock-v1", false),
        ];

        for (p, expected_str, expected_model, needs_net) in providers {
            assert_eq!(p.as_str(), expected_str);
            assert_eq!(p.to_string(), expected_str);
            assert_eq!(Provider::from_str(expected_str).unwrap(), p);
            assert_eq!(p.default_model(), expected_model);
            assert_eq!(p.requires_network(), needs_net);
            assert_eq!(p.is_mock(), !needs_net);

            // Serde JSON roundtrip
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(json, format!("\"{expected_str}\""));
            let back: Provider = serde_json::from_str(&json).unwrap();
            assert_eq!(back, p);
        }

        assert!(Provider::Claude.is_blackwall_native());
        assert!(Provider::Omp.is_blackwall_native());
        assert!(Provider::Gateway.is_blackwall_native());
        assert!(!Provider::Agy.is_blackwall_native());
        assert!(!Provider::Mock.is_blackwall_native());
    }

    #[test]
    fn test_unknown_provider_fails() {
        let err = Provider::from_str("unknown-harness").unwrap_err();
        assert_eq!(err, ParseProviderError("unknown-harness".into()));
        assert_eq!(err.to_string(), "unknown agent provider: unknown-harness");
    }
}
