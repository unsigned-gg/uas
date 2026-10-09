//! Errors of the sandbox crate.

use std::path::PathBuf;

/// What can go wrong while probing the kernel or building a ruleset.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// A path named in a rule does not exist. Spec 0010 D4: never skip it silently.
    #[error("path does not exist: {0}")]
    PathMissing(PathBuf),
    /// A path exists but could not be canonicalised or opened.
    #[error("cannot open path {0}: {1}")]
    PathOpen(PathBuf, #[source] std::io::Error),
    /// The `landlock` crate refused the ruleset or a rule.
    #[error("landlock ruleset error: {0}")]
    Ruleset(#[from] landlock::RulesetError),
    /// The probe thread could not start or did not finish.
    #[error("the landlock probe thread failed: {0}")]
    ProbeThread(String),
}
