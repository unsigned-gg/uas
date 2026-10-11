// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Kernel Landlock ABI probe.
//!
//! The `landlock` crate keeps its probe private and reports the kernel status only
//! from `restrict_self()`. Spec 0010 D2: apply a minimal ruleset on a scratch thread
//! and read the status from there. Landlock confines the calling thread only, so the
//! process stays unrestricted.

use landlock::{
    Access, AccessFs, CompatLevel, Compatible, LandlockStatus, Ruleset, RulesetAttr, ABI,
};

use crate::error::SandboxError;

/// Whether the running kernel offers Landlock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiStatus {
    /// Landlock is not built into the kernel (`ENOSYS`).
    Unsupported,
    /// Landlock is built in but not enabled at boot (`EOPNOTSUPP`).
    NotEnabled,
    /// Landlock works. `AbiReport::effective_abi` says which version.
    Available,
}

/// What the probe found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiReport {
    /// Kernel support.
    pub status: AbiStatus,
    /// The highest ABI version both the kernel and the `landlock` crate understand.
    /// Zero when `status` is not `Available`.
    pub effective_abi: u32,
    /// The raw kernel ABI when it is newer than the crate knows. `None` otherwise.
    pub kernel_abi: Option<u32>,
}

impl AbiReport {
    /// True when rules can be enforced on this kernel.
    pub fn is_available(&self) -> bool {
        self.status == AbiStatus::Available
    }
}

impl From<LandlockStatus> for AbiReport {
    fn from(status: LandlockStatus) -> Self {
        match status {
            LandlockStatus::NotImplemented => AbiReport {
                status: AbiStatus::Unsupported,
                effective_abi: 0,
                kernel_abi: None,
            },
            LandlockStatus::NotEnabled => AbiReport {
                status: AbiStatus::NotEnabled,
                effective_abi: 0,
                kernel_abi: None,
            },
            LandlockStatus::Available {
                effective_abi,
                kernel_abi,
            } => AbiReport {
                status: AbiStatus::Available,
                effective_abi: effective_abi as u32,
                kernel_abi: kernel_abi.and_then(|raw| u32::try_from(raw).ok()),
            },
        }
    }
}

/// Probe the kernel on a scratch thread. The calling thread is never restricted.
pub fn probe() -> Result<AbiReport, SandboxError> {
    let handle = std::thread::Builder::new()
        .name("uas-landlock-probe".into())
        .spawn(probe_on_this_thread)
        .map_err(|err| SandboxError::ProbeThread(err.to_string()))?;
    handle
        .join()
        .map_err(|_| SandboxError::ProbeThread("probe thread panicked".into()))?
}

/// Restrict the current thread with an empty ruleset and report the kernel status.
///
/// Best effort on purpose: on a kernel with no Landlock the crate makes no syscall
/// and reports `NotImplemented` or `NotEnabled` instead of failing.
fn probe_on_this_thread() -> Result<AbiReport, SandboxError> {
    let status = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_all(ABI::V1))?
        .create()?
        .restrict_self()?;
    Ok(AbiReport::from(status.landlock))
}
