// SPDX-License-Identifier: Apache-2.0 OR MIT
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
    /// The child could not enter the jail, so it never ran. Spec 0010, "Launcher".
    #[error("the worker could not enter the jail (EPERM from pre_exec); it did not run")]
    JailRefused,
    /// The child could not get a private network namespace or bring `lo` up. Spec 0011.
    #[error("the worker could not enter a private network namespace (ENONET from pre_exec); it did not run")]
    NetnsRefused,
    /// `spawn` failed for a reason other than the jail.
    #[error("cannot spawn the worker: {0}")]
    Spawn(#[source] std::io::Error),
}
