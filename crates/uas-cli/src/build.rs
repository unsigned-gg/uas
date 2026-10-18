// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `uas build -- <command>`: run a command while holding one host slot (spec 0059).
//!
//! The slot comes from the spec 0036 arbiter. It is a count only: this module writes no limit
//! file and sets no resource limit (D1).

use std::path::PathBuf;
use std::process::Command as Process;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use uas_domain::{HostCapacity, SlotAcquirePolicy};
use uas_pool::slot::arbitration::HostSlotArbiter;
use uas_pool::PoolError;

/// Exit status when no slot frees in time (`EX_TEMPFAIL`, REQ-0059-04).
pub const EXIT_NO_SLOT: i32 = 75;
/// Value given to a child that has no `CARGO_BUILD_JOBS` (D4, spec 0020).
const DEFAULT_JOBS: &str = "2";

/// Arguments of `uas build`.
#[derive(Debug, clap::Args)]
pub struct BuildOpts {
    /// Directory that holds the slot lock files.
    #[arg(long, env = "UAS_SLOT_DIR", default_value = "/tmp/uas-slots")]
    slot_dir: PathBuf,
    /// Host ceiling: how many builds may hold a slot at once.
    #[arg(long, default_value_t = HostCapacity::DEFAULT.get())]
    host_slots: usize,
    /// Longest wait for a slot, in seconds.
    #[arg(long, default_value_t = 600, conflicts_with = "try_only")]
    wait: u64,
    /// Do not wait for a slot.
    #[arg(long = "try", id = "try_only")]
    try_only: bool,
    /// The command and its arguments, after `--`.
    #[arg(last = true, required = true)]
    command: Vec<String>,
}

/// Map a child's end to the `uas build` exit status (D5).
fn exit_code(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .or_else(|| status.signal().map(|s| 128 + s))
        .unwrap_or(1)
}

/// Take a slot, run the command, drop the slot, and return the exit status to use.
pub fn run(opts: &BuildOpts) -> Result<i32> {
    let capacity = HostCapacity::new(opts.host_slots).context("invalid --host-slots")?;
    let arbiter = HostSlotArbiter::new(&opts.slot_dir, capacity);
    let (policy, timeout) = if opts.try_only {
        (SlotAcquirePolicy::Try, Duration::ZERO)
    } else {
        (
            SlotAcquirePolicy::WaitBounded,
            Duration::from_secs(opts.wait),
        )
    };
    let start = Instant::now();
    let lease = match arbiter.acquire(policy, timeout) {
        Ok(lease) => lease,
        Err(PoolError::HostCapacityExceeded(h)) => {
            eprintln!(
                "uas build: no host slot free ({h} of {h} held) after {:.1}s; command not started",
                start.elapsed().as_secs_f64()
            );
            return Ok(EXIT_NO_SLOT);
        }
        Err(e) => bail!("cannot take a host slot: {e}"),
    };
    eprintln!(
        "uas build: holding host slot {} (waited {:.1}s)",
        lease.index().get(),
        start.elapsed().as_secs_f64()
    );
    let (program, args) = opts
        .command
        .split_first()
        .context("no command given after --")?;
    let mut child = Process::new(program);
    child.args(args);
    if std::env::var_os("CARGO_BUILD_JOBS").is_none() {
        child.env("CARGO_BUILD_JOBS", DEFAULT_JOBS);
    }
    let status = child
        .status()
        .with_context(|| format!("cannot run {program}"))?;
    drop(lease);
    Ok(exit_code(status))
}
