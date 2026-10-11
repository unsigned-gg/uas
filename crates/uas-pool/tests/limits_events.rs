//! Worker enrolment and limit-event classification (spec 0041 REQ-02, REQ-05 to REQ-08).
//!
//! The cgroup tree is a temp directory of plain files, so the tests run on any host.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use uas_domain::{KillReason, LimitEvents, WorkerOutcome};
use uas_pool::{spawn_worker, LaunchConfig, PoolError, SlotCgroup};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A fresh fake slot cgroup with empty `cgroup.procs` and the two events files.
fn fake_cgroup(memory_events: &str, pids_events: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("uas-limits-events-{}-{n}", std::process::id()));
    fs::create_dir_all(&dir).expect("mkdir");
    fs::write(dir.join("cgroup.procs"), "").expect("procs");
    fs::write(dir.join("memory.events"), memory_events).expect("memory.events");
    fs::write(dir.join("pids.events"), pids_events).expect("pids.events");
    dir
}

fn sh(script: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", script]);
    cmd
}

fn run(
    script: &str,
    dir: &Path,
    baseline: LimitEvents,
) -> Result<(std::process::ExitStatus, uas_pool::WorkerAudit), PoolError> {
    let cgroup = SlotCgroup {
        path: dir.to_path_buf(),
        baseline,
    };
    spawn_worker(
        sh(script),
        &LaunchConfig::default(),
        "uncapped".into(),
        Some(cgroup),
    )?
    .finish()
}

#[test]
fn limits_events_worker_is_enrolled_before_exec() {
    let dir = fake_cgroup("oom_kill 0\n", "max 0\n");
    run("exit 0", &dir, LimitEvents::default()).expect("run");
    assert_eq!(fs::read_to_string(dir.join("cgroup.procs")).unwrap(), "0");
}

#[test]
fn limits_events_unwritable_procs_fails_closed_with_no_exec() {
    // A directory cannot be opened for writing, whoever the user is.
    let dir = fake_cgroup("oom_kill 0\n", "max 0\n");
    fs::remove_file(dir.join("cgroup.procs")).unwrap();
    fs::create_dir(dir.join("cgroup.procs")).unwrap();
    let marker = dir.join("ran");
    let script = format!("touch {}", marker.display());
    assert!(run(&script, &dir, LimitEvents::default()).is_err());
    assert!(!marker.exists(), "the worker must not exec");
}

#[test]
fn limits_events_failed_procs_write_fails_closed_with_no_exec() {
    // /dev/full opens for writing and refuses every write with ENOSPC, which
    // fails inside the child hook, after the fork and before exec.
    let dir = fake_cgroup("oom_kill 0\n", "max 0\n");
    fs::remove_file(dir.join("cgroup.procs")).unwrap();
    std::os::unix::fs::symlink("/dev/full", dir.join("cgroup.procs")).unwrap();
    let marker = dir.join("ran");
    let script = format!("touch {}", marker.display());
    assert!(run(&script, &dir, LimitEvents::default()).is_err());
    assert!(!marker.exists(), "the worker must not exec");
}

#[test]
fn limits_events_oom_kill_gives_killed_oom() {
    let dir = fake_cgroup("oom_kill 0\n", "max 0\n");
    // The counter moves while the worker runs, as the kernel would move it.
    let script = format!(
        "echo 'oom_kill 1' > {}/memory.events; exit 137",
        dir.display()
    );
    let (_, audit) = run(&script, &dir, LimitEvents::default()).expect("run");
    assert_eq!(audit.limit_events.oom_kill, 1);
    assert_eq!(audit.outcome, WorkerOutcome::Killed(KillReason::Oom));
}

#[test]
fn limits_events_baseline_is_subtracted() {
    let dir = fake_cgroup("oom_kill 2\n", "max 0\n");
    let before = LimitEvents {
        oom_kill: 2,
        pids_max_hits: 0,
    };
    let (_, audit) = run("exit 0", &dir, before).expect("run");
    assert_eq!(audit.limit_events, LimitEvents::default());
    assert_eq!(audit.outcome, WorkerOutcome::Exited(0));
}

#[test]
fn limits_events_pids_hits_do_not_change_the_outcome() {
    let dir = fake_cgroup("oom_kill 0\n", "max 5\n");
    let (_, audit) = run("exit 1", &dir, LimitEvents::default()).expect("run");
    assert_eq!(audit.limit_events.pids_max_hits, 5);
    assert_eq!(audit.outcome, WorkerOutcome::Exited(1));
}

#[test]
fn limits_events_signal_death_is_128_plus_signal() {
    let dir = fake_cgroup("oom_kill 0\n", "max 0\n");
    let (_, audit) = run("kill -9 $$", &dir, LimitEvents::default()).expect("run");
    assert_eq!(audit.outcome, WorkerOutcome::Exited(137));
}

#[test]
fn limits_events_sigkill_with_oom_delta_is_killed_oom() {
    let dir = fake_cgroup("oom_kill 0\n", "max 0\n");
    let script = format!(
        "echo 'oom_kill 1' > {}/memory.events; kill -9 $$",
        dir.display()
    );
    let (status, audit) = run(&script, &dir, LimitEvents::default()).expect("run");
    assert_eq!(
        std::os::unix::process::ExitStatusExt::signal(&status),
        Some(9)
    );
    assert_eq!(audit.outcome, WorkerOutcome::Killed(KillReason::Oom));
}
