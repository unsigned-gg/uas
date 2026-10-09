//! Spawn a worker process inside the jail.
//!
//! Spec 0010, "Launcher". The child enters the jail between `fork` and `exec`:
//! `restrict_self` sets `PR_SET_NO_NEW_PRIVS` and calls `landlock_restrict_self`.
//! The parent never restricts itself.

use std::error::Error;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use std::sync::Mutex;

use landlock::{RulesetCreated, RulesetError, RulesetStatus};

use crate::error::SandboxError;
use crate::netns::NETNS_REFUSED_ERRNO;

/// `ENOTRECOVERABLE`. The child reports a refused jail with this errno. std forwards
/// only a raw OS error from `pre_exec` to the parent and turns anything else into
/// `EINVAL`, so the errno is the whole message. `exec`, seccomp and LSM denials
/// produce `EPERM`, `EACCES` or `ENOENT`, never this value, so the parent can tell
/// a refused jail from every other spawn failure.
pub const JAIL_REFUSED_ERRNO: i32 = 131;

/// Install the jail on `command`. Every later `spawn` of this command restricts the
/// child before `exec`. The ruleset stays in the parent's closure; each child takes
/// its own copy after fork, so one jailed command can spawn any number of workers.
pub fn jail(command: &mut Command, ruleset: RulesetCreated) -> &mut Command {
    let slot = Mutex::new(Some(ruleset));
    // SAFETY: the closure runs in the child after fork and before exec. It locks
    // a mutex that only this child can hold (futex), takes the ruleset out of
    // it, makes the prctl and landlock syscalls through the landlock crate, and
    // closes the ruleset fd when the ruleset drops. Every error it returns is
    // built from a raw errno. Nothing on the path allocates, so it is safe in a
    // child forked from a multi-threaded parent.
    #[allow(unsafe_code)]
    unsafe {
        command.pre_exec(move || restrict_child(&slot))
    }
}

/// Spawn `command` inside `ruleset`. Fails closed: a child that cannot enter the
/// jail never runs, and the error says so.
///
/// `JailRefused` is the launcher's own verdict: `no_new_privs` was not set, the
/// ruleset was not enforced, or the kernel refused without an errno. `NetnsRefused`
/// comes from a `netns::isolate` hook on the same command. A kernel errno from
/// `landlock_restrict_self` or `prctl` arrives unchanged in `Spawn`.
pub fn launch(ruleset: RulesetCreated, command: &mut Command) -> Result<Child, SandboxError> {
    jail(command, ruleset).spawn().map_err(classify_spawn_error)
}

/// Turn a `spawn` error into the sandbox verdict. The reserved errnos come from
/// the hooks; everything else is a plain spawn failure with its own errno.
fn classify_spawn_error(err: io::Error) -> SandboxError {
    match err.raw_os_error() {
        Some(JAIL_REFUSED_ERRNO) => SandboxError::JailRefused,
        Some(NETNS_REFUSED_ERRNO) => SandboxError::NetnsRefused,
        _ => SandboxError::Spawn(err),
    }
}

/// Runs in the child. Refuses to continue unless the ruleset is enforced and
/// `no_new_privs` is set. `PartiallyEnforced` is accepted: ABI v1 accesses are a hard
/// requirement at build time, so a partial status only means a newer access was
/// dropped on an older kernel.
fn restrict_child(slot: &Mutex<Option<RulesetCreated>>) -> io::Result<()> {
    let refused = || io::Error::from_raw_os_error(JAIL_REFUSED_ERRNO);
    // After fork this is the child's private copy of the slot, so `take` never
    // affects the parent or a sibling. An empty slot cannot happen; it is refused
    // rather than unwrapped.
    let ruleset = slot
        .lock()
        .map_err(|_| refused())?
        .take()
        .ok_or_else(refused)?;
    let status = ruleset.restrict_self().map_err(|err| {
        kernel_errno(&err)
            .map(io::Error::from_raw_os_error)
            .unwrap_or_else(refused)
    })?;
    if !status.no_new_privs {
        return Err(refused());
    }
    match status.ruleset {
        RulesetStatus::FullyEnforced | RulesetStatus::PartiallyEnforced => Ok(()),
        RulesetStatus::NotEnforced => Err(refused()),
    }
}

/// The raw errno behind a `RulesetError`, if a syscall produced one. Walks the
/// error chain by reference; no allocation.
fn kernel_errno(err: &RulesetError) -> Option<i32> {
    let mut source: Option<&(dyn Error + 'static)> = err.source();
    while let Some(inner) = source {
        if let Some(io_err) = inner.downcast_ref::<io::Error>() {
            return io_err.raw_os_error();
        }
        source = inner.source();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_errnos_become_verdicts() {
        let jail = classify_spawn_error(io::Error::from_raw_os_error(JAIL_REFUSED_ERRNO));
        assert!(matches!(jail, SandboxError::JailRefused));
        let netns = classify_spawn_error(io::Error::from_raw_os_error(NETNS_REFUSED_ERRNO));
        assert!(matches!(netns, SandboxError::NetnsRefused));
    }

    #[test]
    fn other_errnos_stay_spawn_errors() {
        let eperm = classify_spawn_error(io::Error::from_raw_os_error(1));
        assert!(matches!(eperm, SandboxError::Spawn(ref e) if e.raw_os_error() == Some(1)));
        let other = classify_spawn_error(io::Error::other("no errno"));
        assert!(matches!(other, SandboxError::Spawn(ref e) if e.raw_os_error().is_none()));
    }
}
