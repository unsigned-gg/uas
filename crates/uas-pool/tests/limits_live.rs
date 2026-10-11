// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Live OOM and pids proof on a real cgroup v2 tree (spec 0041 REQ-05 to REQ-08, plan P7).
//!
//! The setup builds a delegated tree under this process's own cgroup: it moves the test
//! process to a leaf, enables `+memory +pids` on the parent and makes one slot cgroup per
//! case. When any step fails the case prints `skipped: ...` and passes, so hosts without
//! delegation stay green. Run it live with:
//! `systemd-run --user --scope -q -p Delegate=yes <test binary> --nocapture --test-threads=1`.
#![allow(missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use uas_domain::{KillReason, LimitEvents, WorkerOutcome};
use uas_pool::{spawn_worker, LaunchConfig, SlotCgroup};

const GIB: u64 = 1024 * 1024 * 1024;

/// This process's cgroup directory, from `/proc/self/cgroup`.
fn own_cgroup() -> Option<PathBuf> {
    let text = fs::read_to_string("/proc/self/cgroup").ok()?;
    let rel = text.lines().find_map(|l| l.strip_prefix("0::"))?;
    Some(PathBuf::from("/sys/fs/cgroup").join(rel.trim_start_matches('/')))
}

/// Builds the delegated tree of plan 0041 finding 11 and returns a fresh slot cgroup
/// with `memory.max`, `memory.swap.max` and `pids.max` files, or `None` on any failure.
fn make_slot(name: &str) -> Option<PathBuf> {
    let parent = own_cgroup()?;
    let attempt = || -> std::io::Result<PathBuf> {
        let leaf = parent.join("leaf");
        fs::create_dir_all(&leaf)?;
        fs::write(leaf.join("cgroup.procs"), std::process::id().to_string())?;
        fs::write(parent.join("cgroup.subtree_control"), "+memory +pids")?;
        let slot = parent.join(format!("{name}-{}", std::process::id()));
        fs::create_dir(&slot)?;
        for file in ["memory.max", "memory.swap.max", "pids.max", "memory.events"] {
            if !slot.join(file).exists() {
                return Err(std::io::Error::other(format!("{file} absent")));
            }
        }
        Ok(slot)
    };
    match attempt() {
        Ok(slot) => Some(slot),
        Err(err) => {
            println!(
                "skipped: cannot make a memory+pids cgroup under {} ({err})",
                parent.display()
            );
            None
        }
    }
}

/// Kills whatever is left in the slot and removes it, best effort.
fn cleanup(slot: &Path) {
    let _ = fs::write(slot.join("cgroup.kill"), "1");
    for _ in 0..50 {
        if fs::remove_dir(slot).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn run(slot: &Path, script: &str) -> (std::process::ExitStatus, uas_pool::WorkerAudit) {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", script]);
    let cgroup = SlotCgroup {
        path: slot.to_path_buf(),
        baseline: LimitEvents::default(),
    };
    spawn_worker(
        cmd,
        &LaunchConfig::default(),
        "uncapped".into(),
        Some(cgroup),
    )
    .expect("spawn")
    .finish()
    .expect("finish")
}

#[test]
fn limits_live_oom_kill_gives_killed_oom() {
    let Some(slot) = make_slot("oom") else { return };
    fs::write(slot.join("memory.max"), (2 * GIB).to_string()).expect("memory.max");
    fs::write(slot.join("memory.swap.max"), "0").expect("memory.swap.max");
    let (_, audit) = run(&slot, "exec python3 -c 'x=bytearray(2**32)'");
    cleanup(&slot);
    assert_eq!(audit.limit_events.oom_kill, 1, "{audit:?}");
    assert_eq!(audit.outcome, WorkerOutcome::Killed(KillReason::Oom));
}

#[test]
fn limits_live_pids_max_is_recorded_and_changes_no_outcome() {
    let Some(slot) = make_slot("pids") else {
        return;
    };
    fs::write(slot.join("pids.max"), "64").expect("pids.max");
    let (_, audit) = run(
        &slot,
        "for i in $(seq 100); do sleep 3 & done; wait; exit 0",
    );
    cleanup(&slot);
    assert!(audit.limit_events.pids_max_hits > 0, "{audit:?}");
    assert!(
        matches!(audit.outcome, WorkerOutcome::Exited(_)),
        "{audit:?}"
    );
}
