//! Landlock rulesets, namespace isolation and path canonicalisation for harness processes.
//!
//! Spec 0010 gives this crate its first code: a probe for the kernel's
//! Landlock ABI, a builder for the default-deny filesystem ruleset that
//! confines a worker, and the launcher that spawns a worker inside it.
//! See `specs/0010-landlock-ruleset-and-abi-probe.md`.
//!
//! `unsafe` is denied, not forbidden (AGENTS.md rule 14). The blocks that opt
//! in are `launcher::jail` (`pre_exec`) and `netns` (`pre_exec`, `unshare`,
//! two `ioctl`s), each with a SAFETY comment.
#![deny(unsafe_code)]

pub mod abi;
pub mod canon;
pub mod error;
pub mod ipc;
pub mod launcher;
pub mod netns;
pub mod ruleset;
pub mod seccomp;

pub use abi::{probe, AbiReport, AbiStatus};
pub use canon::canonical;
pub use error::SandboxError;
pub use launcher::{jail, launch, JAIL_REFUSED_ERRNO};
pub use netns::{isolate, NETNS_REFUSED_ERRNO};
pub use ruleset::{LandlockRulesetBuilder, PathAccess, PlannedRule, RulesetPlan};
