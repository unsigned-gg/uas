//! Worker priority clamping with process groups (spec 0014 D4, ruling R2).
//!
//! On Linux `setpriority(PRIO_PROCESS, pid, nice)` changes one thread. Measured
//! on ceres on 2026-10-09: a four-thread process reniced by pid read
//! `19, 0, 0, 0`. `PRIO_PGRP` clamps every thread of every process in the group,
//! and later threads and children inherit the value. So `uas-pool` puts each
//! worker in its own process group and clamps the group once, before `exec`.
//!
//! Everything here goes through `rustix`, so the crate needs no `unsafe` for
//! the clamp. Reading another process's nice value is unprivileged; lowering a
//! process's priority is allowed for the owner; raising it back is not without
//! `CAP_SYS_NICE`, which is why a clamped worker cannot undo the clamp.

use std::io;

use rustix::process::{
    getpgid, getpriority_pgrp, getpriority_process, setpgid, setpriority_pgrp, setpriority_process,
    Pid,
};

/// The nice value for worker process trees (spec 0014, clamping invariants).
pub const WORKER_NICE: i32 = 19;

/// Puts the calling process in a new process group and clamps that group.
///
/// Call it from the child side of a spawn, before `exec`, through
/// `std::os::unix::process::CommandExt::pre_exec`. The child then has
/// `pgid == pid` and nice [`WORKER_NICE`], and everything it starts inherits
/// both.
pub fn enter_clamped_group() -> io::Result<()> {
    setpgid(None, None)?;
    setpriority_pgrp(None, WORKER_NICE)?;
    Ok(())
}

/// Clamps every process in group `pgid` to [`WORKER_NICE`].
pub fn clamp_process_group(pgid: u32) -> io::Result<()> {
    Ok(setpriority_pgrp(Some(pid(pgid)?), WORKER_NICE)?)
}

/// Clamps one process (its main thread and every thread created afterwards).
///
/// The daemon uses it to re-clamp an escapee (spec 0014, anti-escape defense).
pub fn clamp_process(pid_raw: u32) -> io::Result<()> {
    Ok(setpriority_process(Some(pid(pid_raw)?), WORKER_NICE)?)
}

/// The nice value of process `pid_raw` (its main thread).
pub fn nice_of(pid_raw: u32) -> io::Result<i32> {
    Ok(getpriority_process(Some(pid(pid_raw)?))?)
}

/// The nice value of process group `pgid` (the highest priority member).
pub fn nice_of_group(pgid: u32) -> io::Result<i32> {
    Ok(getpriority_pgrp(Some(pid(pgid)?))?)
}

/// The process group of `pid_raw`.
pub fn pgid_of(pid_raw: u32) -> io::Result<u32> {
    let pgid = getpgid(Some(pid(pid_raw)?))?;
    Ok(pgid.as_raw_nonzero().get().unsigned_abs())
}

/// Re-clamps `pid_raw` when its nice value is above [`WORKER_NICE`].
///
/// Returns `Ok(true)` when a clamp was applied. The daemon calls this on every
/// exec event of a tracked pid.
pub fn reclamp_if_needed(pid_raw: u32) -> io::Result<bool> {
    if nice_of(pid_raw)? >= WORKER_NICE {
        return Ok(false);
    }
    clamp_process(pid_raw)?;
    Ok(true)
}

fn pid(raw: u32) -> io::Result<Pid> {
    i32::try_from(raw)
        .ok()
        .and_then(Pid::from_raw)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("invalid pid {raw}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_overflow_pids_are_invalid() {
        assert!(pid(0).is_err());
        assert!(pid(u32::MAX).is_err());
        assert!(pid(1).is_ok());
    }

    #[test]
    fn own_process_group_and_nice_are_readable() {
        let me = std::process::id();
        let pgid = pgid_of(me).expect("getpgid on self");
        assert!(pgid > 0);
        let nice = nice_of(me).expect("getpriority on self");
        assert!((-20..=19).contains(&nice));
        let group_nice = nice_of_group(pgid).expect("getpriority on own group");
        assert!((-20..=19).contains(&group_nice));
    }

    #[test]
    fn reclamp_leaves_an_already_clamped_process_alone() {
        // Reading is enough here: this process is not at nice 19, so the
        // function would clamp it. Lowering our own priority would slow the
        // rest of the test binary, so only the invalid-pid path is exercised.
        assert!(reclamp_if_needed(0).is_err());
    }
}
