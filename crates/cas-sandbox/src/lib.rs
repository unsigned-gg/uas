//! Landlock rulesets, namespace isolation and path canonicalisation for harness processes.
//!
//! Spec 0010 gives this crate its first code: a probe for the kernel's
//! Landlock ABI and a builder for the default-deny filesystem ruleset that
//! confines a worker. Applying the ruleset to a worker process belongs to the
//! launcher (CER-2716). See `specs/0010-landlock-ruleset-and-abi-probe.md`.
#![deny(unsafe_code)]

pub mod abi;
pub mod canon;
pub mod error;
pub mod ruleset;

pub use abi::{probe, AbiReport, AbiStatus};
pub use canon::canonical;
pub use error::SandboxError;
pub use ruleset::{LandlockRulesetBuilder, PathAccess, PlannedRule, RulesetPlan};
