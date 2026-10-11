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

/// This process's cgroup directory, from `/proc/self/cgroup`.
fn own_cgroup() -> Option<PathBuf> {
    let text = fs::read_to_string("/proc/self/cgroup").ok()?;
    let rel = text.lines().find_map(|l| l.strip_prefix("0::"))?;
    Some(PathBuf::from("/sys/fs/cgroup").join(rel.trim_start_matches('/')))
}

/// The delegated tree of one case. Dropping it removes the slot, moves the test process back to
/// its original cgroup and removes `leaf`, so every exit path (pass, skip, panic) cleans up.
/// Setup moves the whole process, so a lock serialises the cases within one test binary.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Tree {
    _serial: std::sync::MutexGuard<'static, ()>,
    parent: PathBuf,
    leaf: PathBuf,
    slot: PathBuf,
}

impl Tree {
    fn slot(&self) -> &Path {
        &self.slot
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::write(self.slot.join("cgroup.kill"), "1");
        for _ in 0..50 {
            if fs::remove_dir(&self.slot).is_ok() || !self.slot.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // A parent with controllers delegated cannot hold a process, so disable them first.
        let _ = fs::write(self.parent.join("cgroup.subtree_control"), "-memory -pids");
        let _ = fs::write(
            self.parent.join("cgroup.procs"),
            std::process::id().to_string(),
        );
        let _ = fs::remove_dir(&self.leaf);
    }
}

fn skip(parent: &Path, why: &dyn std::fmt::Display) {
    println!(
        "skipped: cannot make a memory+pids cgroup under {} ({why})",
        parent.display()
    );
}

/// Builds the delegated tree of plan 0041 finding 11 with a fresh slot cgroup, writes `limits`
/// into it, and returns `None` after printing a `skipped:` line when any step fails.
fn make_tree(name: &str, limits: &[(&str, &str)]) -> Option<Tree> {
    let parent = own_cgroup()?;
    let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let tree = Tree {
        _serial: serial,
        leaf: parent.join("leaf"),
        slot: parent.join(format!("{name}-{}", std::process::id())),
        parent,
    };
    let attempt = |tree: &Tree| -> std::io::Result<()> {
        fs::create_dir_all(&tree.leaf)?;
        fs::write(
            tree.leaf.join("cgroup.procs"),
            std::process::id().to_string(),
        )?;
        fs::write(tree.parent.join("cgroup.subtree_control"), "+memory +pids")?;
        fs::create_dir(&tree.slot)?;
        for file in ["memory.max", "memory.swap.max", "pids.max", "memory.events"] {
            if !tree.slot.join(file).exists() {
                return Err(std::io::Error::other(format!("{file} absent")));
            }
        }
        for (file, value) in limits {
            fs::write(tree.slot.join(file), value)
                .map_err(|e| std::io::Error::other(format!("{file}: {e}")))?;
        }
        Ok(())
    };
    match attempt(&tree) {
        Ok(()) => Some(tree),
        Err(err) => {
            skip(&tree.parent, &err);
            // `tree` drops here and undoes whatever the failed attempt did.
            None
        }
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

const ALLOC_4GIB: &str = "exec python3 -c 'x=bytearray(2**32)'";

#[test]
fn limits_live_oom_kill_gives_killed_oom() {
    let limits = [("memory.max", "2147483648"), ("memory.swap.max", "0")];
    let Some(tree) = make_tree("oom", &limits) else {
        return;
    };
    let (_, audit) = run(tree.slot(), ALLOC_4GIB);
    assert_eq!(audit.limit_events.oom_kill, 1, "{audit:?}");
    assert_eq!(audit.outcome, WorkerOutcome::Killed(KillReason::Oom));
}

#[test]
fn limits_live_without_memory_max_nothing_is_killed() {
    // Negative control: the same allocation with `memory.max` unset must not be an OOM kill,
    // so the limit, not the host, is what kills in the case above.
    let Some(tree) = make_tree("nolimit", &[("memory.swap.max", "0")]) else {
        return;
    };
    let (_, audit) = run(tree.slot(), ALLOC_4GIB);
    assert_eq!(audit.limit_events.oom_kill, 0, "{audit:?}");
    assert!(
        matches!(audit.outcome, WorkerOutcome::Exited(_)),
        "{audit:?}"
    );
}

#[test]
fn limits_live_pids_max_is_recorded_and_changes_no_outcome() {
    let Some(tree) = make_tree("pids", &[("pids.max", "64")]) else {
        return;
    };
    // Starts 100 sleepers and counts the forks the kernel refuses; exits 7 whatever happens.
    let script = "exec python3 -c 'import subprocess as s\nk=[]\nfor _ in range(100):\n  try: k.append(s.Popen([\"sleep\",\"3\"]))\n  except OSError: pass\nraise SystemExit(7)'";
    let (_, audit) = run(tree.slot(), script);
    assert!(audit.limit_events.pids_max_hits > 0, "{audit:?}");
    assert_eq!(audit.outcome, WorkerOutcome::Exited(7), "{audit:?}");
}
