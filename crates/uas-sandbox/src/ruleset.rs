// SPDX-License-Identifier: Apache-2.0 OR MIT
//! The default-deny filesystem ruleset for a worker.
//!
//! Spec 0010 D5 and D6: every access the kernel offers is handled, so anything not
//! listed is denied. ABI v1 accesses are a hard requirement; newer ones are best
//! effort.

use std::path::{Path, PathBuf};

use landlock::{
    Access, AccessFs, BitFlags, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
    RulesetCreated, RulesetCreatedAttr, ABI,
};

use crate::canon::canonical;
use crate::error::SandboxError;

/// The newest ABI whose accesses the builder handles on a best-effort basis.
const BEST_EFFORT_ABI: ABI = ABI::V5;

/// What a rule grants beneath its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathAccess {
    /// Read files and list directories.
    Read,
    /// `Read` plus execute.
    ReadExec,
    /// Every access except execute. Spec 0010 D7: scratch and device paths.
    Write,
    /// Every filesystem access the effective ABI offers.
    ReadWrite,
}

impl PathAccess {
    fn flags(self) -> BitFlags<AccessFs> {
        match self {
            PathAccess::Read => AccessFs::ReadFile | AccessFs::ReadDir,
            PathAccess::ReadExec => AccessFs::ReadFile | AccessFs::ReadDir | AccessFs::Execute,
            PathAccess::Write => AccessFs::from_all(BEST_EFFORT_ABI) & !AccessFs::Execute,
            PathAccess::ReadWrite => AccessFs::from_all(BEST_EFFORT_ABI),
        }
    }
}

/// One rule after canonicalisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedRule {
    /// The canonical path the rule is attached to.
    pub path: PathBuf,
    /// What the rule grants.
    pub access: PathAccess,
}

/// The rules a builder will install, for tests and logs. It does no I/O of its own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RulesetPlan {
    /// Rules in the order they were requested.
    pub rules: Vec<PlannedRule>,
    /// Optional paths that were absent and therefore left out.
    pub skipped: Vec<PathBuf>,
}

/// Collects path rules and produces the ruleset the launcher applies.
#[derive(Debug, Default)]
pub struct LandlockRulesetBuilder {
    requested: Vec<(PathBuf, PathAccess)>,
    skipped: Vec<PathBuf>,
}

impl LandlockRulesetBuilder {
    /// An empty builder. With no rules, every handled access is denied.
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant read beneath `path`. The path must exist at `build` time.
    pub fn allow_read(self, path: impl AsRef<Path>) -> Self {
        self.allow(path, PathAccess::Read)
    }

    /// Grant read and execute beneath `path`.
    pub fn allow_read_exec(self, path: impl AsRef<Path>) -> Self {
        self.allow(path, PathAccess::ReadExec)
    }

    /// Grant every access except execute beneath `path`.
    pub fn allow_write(self, path: impl AsRef<Path>) -> Self {
        self.allow(path, PathAccess::Write)
    }

    /// Grant every filesystem access beneath `path`.
    pub fn allow_read_write(self, path: impl AsRef<Path>) -> Self {
        self.allow(path, PathAccess::ReadWrite)
    }

    /// Add a rule for `path` with `access`.
    pub fn allow(mut self, path: impl AsRef<Path>, access: PathAccess) -> Self {
        self.requested.push((path.as_ref().to_path_buf(), access));
        self
    }

    /// Add a rule only if `path` exists now. An absent path is recorded in the plan.
    pub fn allow_optional(mut self, path: impl AsRef<Path>, access: PathAccess) -> Self {
        let path = path.as_ref();
        if path.exists() {
            self.allow(path, access)
        } else {
            self.skipped.push(path.to_path_buf());
            self
        }
    }

    /// The preset for a UAS worker. Spec 0010, "Worker preset".
    ///
    /// `workspace` gets every access. `/usr`, `/lib`, `/bin` and each `toolchain`
    /// path get read and execute. `/lib64` and `/sbin` are optional. `/etc` is
    /// read-only. `/dev/null`, `/dev/zero`, `/dev/urandom` and `/tmp` get write
    /// without execute. Nothing else is reachable. Pass toolchain paths with
    /// care: `~/.cargo` whole exposes `credentials.toml`.
    pub fn worker<I, P>(workspace: impl AsRef<Path>, toolchain: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut builder = Self::new().allow_read_write(workspace);
        for system in ["/usr", "/lib", "/bin"] {
            builder = builder.allow_read_exec(system);
        }
        for optional in ["/lib64", "/sbin"] {
            builder = builder.allow_optional(optional, PathAccess::ReadExec);
        }
        for path in toolchain {
            builder = builder.allow_read_exec(path);
        }
        builder = builder.allow_read("/etc");
        for scratch in ["/dev/null", "/dev/zero", "/dev/urandom", "/tmp"] {
            builder = builder.allow_write(scratch);
        }
        builder
    }

    /// Canonicalise every requested path. Fails on the first missing one.
    pub fn plan(&self) -> Result<RulesetPlan, SandboxError> {
        let mut rules = Vec::with_capacity(self.requested.len());
        for (path, access) in &self.requested {
            rules.push(PlannedRule {
                path: canonical(path)?,
                access: *access,
            });
        }
        Ok(RulesetPlan {
            rules,
            skipped: self.skipped.clone(),
        })
    }

    /// Build the ruleset. The caller applies it with `restrict_self` on the worker.
    ///
    /// Fails when a path is missing, or when the kernel cannot enforce the ABI v1
    /// accesses (spec 0010 D5: fail closed, never a silent `NotEnforced`). A rule
    /// on a regular file keeps only the accesses that apply to files.
    pub fn build(self) -> Result<(RulesetCreated, RulesetPlan), SandboxError> {
        let plan = self.plan()?;
        let mut ruleset = Ruleset::default()
            .set_compatibility(CompatLevel::HardRequirement)
            .handle_access(AccessFs::from_all(ABI::V1))?
            .set_compatibility(CompatLevel::BestEffort)
            .handle_access(AccessFs::from_all(BEST_EFFORT_ABI))?
            .create()?
            .set_compatibility(CompatLevel::BestEffort);
        for rule in &plan.rules {
            // PathFdError is non-exhaustive upstream, so it is carried as an io::Error.
            let fd = PathFd::new(&rule.path).map_err(|err| {
                SandboxError::PathOpen(rule.path.clone(), std::io::Error::other(err))
            })?;
            let mut access = rule.access.flags();
            if rule.path.is_file() {
                access &= AccessFs::from_file(BEST_EFFORT_ABI);
            }
            ruleset = ruleset.add_rule(PathBeneath::new(fd, access))?;
        }
        Ok((ruleset, plan))
    }
}
