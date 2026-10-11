//! Spec 0040 D2, D4, D6 (REQ-0040-02, 03, 04, 07, 08): a starting pool
//! reclaims the filesystem remains of dead pools and leaves live ones alone.
//!
//! The multi-process cases run a child through `current_exe()`: the test
//! `reconcile_child_fixture` is the child body and does nothing unless
//! `UAS_RECONCILE_CHILD` is set.

mod common;

use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use common::{create_test_base_repo, open_repo, TempDirGuard};
use uas_domain::SlotId;
use uas_pool::{PoolConfig, ReconcileRoots, SlotPool, WarmWorktree};

const CHILD_ENV: &str = "UAS_RECONCILE_CHILD";

/// Child body: provision a slot, print `ready`, then hold the lock until killed.
#[test]
fn reconcile_child_fixture() {
    let Ok(spec) = std::env::var(CHILD_ENV) else {
        return;
    };
    let mut parts = spec.split('|');
    let (base, slot, wt, branch) = (
        parts.next().expect("base"),
        parts.next().expect("slot"),
        parts.next().expect("worktree"),
        parts.next().expect("branch"),
    );
    let _held = WarmWorktree::provision(base, SlotId::new(slot).expect("slot id"), wt, branch)
        .expect("child provision");
    println!("ready");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_child(base: &Path, slot: &str, wt: &Path, branch: &str) -> ChildGuard {
    let exe = std::env::current_exe().expect("current_exe");
    let mut child = Command::new(exe)
        .args(["reconcile_child_fixture", "--exact", "--nocapture"])
        .env(
            CHILD_ENV,
            format!("{}|{slot}|{}|{branch}", base.display(), wt.display()),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("cannot spawn the child fixture");
    let stdout = child.stdout.take().expect("child stdout");
    let mut guard = ChildGuard(child);
    for line in BufReader::new(stdout).lines() {
        if line.expect("read child stdout") == "ready" {
            return guard;
        }
    }
    let status = guard.0.wait().expect("wait child");
    panic!("child exited before it printed ready: {status}");
}

struct Fixture {
    tmp: TempDirGuard,
    base: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let tmp = TempDirGuard::new(name);
        let base = tmp.path().join("base");
        create_test_base_repo(&base).expect("base repo");
        Self { tmp, base }
    }

    fn wt_root(&self) -> PathBuf {
        let root = self.tmp.path().join("worktrees");
        fs::create_dir_all(&root).expect("worktree root");
        root
    }

    fn control(&self, slot: &str) -> PathBuf {
        self.base.join(".git").join("worktrees").join(slot)
    }

    fn pool(&self) -> SlotPool {
        SlotPool::new(PoolConfig {
            reconcile: Some(ReconcileRoots {
                common_git_dir: self.base.join(".git"),
                worktree_root: self.wt_root(),
            }),
            ..PoolConfig::default()
        })
    }

    fn branch_exists(&self, branch: &str) -> bool {
        open_repo(&self.base)
            .expect("open base")
            .try_find_reference(format!("refs/heads/{branch}").as_str())
            .expect("find ref")
            .is_some()
    }

    fn plant_marker(&self, slot: &str, worktree_dir: &Path) {
        let control = self.control(slot);
        fs::create_dir_all(&control).expect("control dir");
        let body = serde_json::json!({
            "slot_id": slot,
            "worktree_dir": worktree_dir,
            "cgroup": format!("slot-{slot}"),
            "branch": "uas/planted",
            "pool_pid": 1,
            "provisioned_at": 1_700_000_000u64,
        });
        fs::write(control.join("uas-pool.json"), body.to_string()).expect("plant marker");
    }
}

#[test]
fn reconcile_kills_child_holding_marker_then_reclaims() {
    let fx = Fixture::new("reclaim");
    let wt = fx.wt_root().join("wt-a");
    let mut child = spawn_child(&fx.base, "slot-a", &wt, "uas/slot-a");
    assert!(wt.is_dir() && fx.control("slot-a").is_dir() && fx.branch_exists("uas/slot-a"));

    child.0.kill().expect("SIGKILL child");
    child.0.wait().expect("reap child");

    let pool = fx.pool();
    assert!(!wt.exists(), "worktree dir must be gone");
    assert!(!fx.control("slot-a").exists(), "control dir must be gone");
    assert!(!fx.branch_exists("uas/slot-a"), "branch must be gone");
    let report = pool.reconcile_report();
    assert_eq!((report.reclaimed, report.kept, report.failed), (1, 0, 0));
}

#[test]
fn reconcile_leaves_live_owner_alone() {
    let fx = Fixture::new("live");
    let wt = fx.wt_root().join("wt-b");
    let _child = spawn_child(&fx.base, "slot-b", &wt, "uas/slot-b");

    let pool = fx.pool();
    assert!(wt.is_dir(), "live worktree must stay");
    assert!(fx.control("slot-b").join("uas-pool.json").is_file());
    assert!(fx.branch_exists("uas/slot-b"), "live branch must stay");
    let report = pool.reconcile_report();
    assert_eq!((report.reclaimed, report.kept, report.failed), (0, 1, 0));
}

#[test]
fn reconcile_refuses_planted_marker_outside_the_root() {
    let fx = Fixture::new("planted");
    let victim = fx.tmp.path().join("victim");
    fs::create_dir_all(&victim).expect("victim");
    fs::write(victim.join("sentinel"), b"keep").expect("sentinel");
    fx.plant_marker("evil", &victim);

    let pool = fx.pool();
    assert!(victim.join("sentinel").is_file(), "sentinel must survive");
    assert!(fx.control("evil").join("uas-pool.json").is_file());
    let report = pool.reconcile_report();
    assert_eq!((report.reclaimed, report.kept, report.failed), (0, 0, 1));
}

#[test]
fn reconcile_refuses_symlinked_worktree_dir() {
    let fx = Fixture::new("symlink-wt");
    let victim = fx.tmp.path().join("victim");
    fs::create_dir_all(&victim).expect("victim");
    fs::write(victim.join("sentinel"), b"keep").expect("sentinel");
    let link = fx.wt_root().join("link");
    symlink(&victim, &link).expect("symlink");
    fx.plant_marker("linked", &link);

    let pool = fx.pool();
    assert!(victim.join("sentinel").is_file(), "sentinel must survive");
    assert!(link.symlink_metadata().is_ok(), "link must stay");
    assert_eq!(pool.reconcile_report().failed, 1);
}

#[test]
fn reconcile_refuses_symlinked_control_dir() {
    let fx = Fixture::new("symlink-ctl");
    let victim = fx.tmp.path().join("victim");
    fs::create_dir_all(&victim).expect("victim");
    fs::write(victim.join("sentinel"), b"keep").expect("sentinel");
    fs::create_dir_all(fx.base.join(".git").join("worktrees")).expect("worktrees");
    symlink(&victim, fx.control("sneaky")).expect("symlink");

    let pool = fx.pool();
    assert!(victim.join("sentinel").is_file(), "sentinel must survive");
    assert_eq!(pool.reconcile_report().failed, 1);
}

#[test]
fn reconcile_reclaims_empty_control_dir() {
    let fx = Fixture::new("empty");
    fs::create_dir_all(fx.control("slot-empty")).expect("control dir");

    let pool = fx.pool();
    assert!(!fx.control("slot-empty").exists(), "empty dir must go");
    let report = pool.reconcile_report();
    assert_eq!((report.reclaimed, report.kept, report.failed), (1, 0, 0));
}

#[test]
fn reconcile_leaves_foreign_control_dir_alone() {
    let fx = Fixture::new("foreign");
    let control = fx.control("user-worktree");
    fs::create_dir_all(&control).expect("control dir");
    fs::write(control.join("gitdir"), b"/somewhere/.git\n").expect("gitdir");

    let pool = fx.pool();
    assert!(control.join("gitdir").is_file(), "not ours, not touched");
    let report = pool.reconcile_report();
    assert_eq!((report.reclaimed, report.kept, report.failed), (0, 0, 0));
}

#[test]
fn reconcile_removes_stale_tmp_marker() {
    let fx = Fixture::new("tmp");
    let control = fx.control("slot-tmp");
    fs::create_dir_all(&control).expect("control dir");
    fs::write(control.join("uas-pool.json.tmp"), b"{").expect("tmp");

    let pool = fx.pool();
    assert!(!control.exists(), "garbage tmp and its dir must go");
    assert_eq!(pool.reconcile_report().reclaimed, 1);
}

#[test]
fn reconcile_keeps_tmp_marker_with_held_lock() {
    let fx = Fixture::new("tmp-held");
    let control = fx.control("slot-tmp");
    fs::create_dir_all(&control).expect("control dir");
    let tmp = control.join("uas-pool.json.tmp");
    fs::write(&tmp, b"{").expect("tmp");
    let holder = fs::File::open(&tmp).expect("open tmp");
    holder.try_lock().expect("hold lock");

    let pool = fx.pool();
    assert!(tmp.is_file(), "a slot being provisioned must stay");
    let report = pool.reconcile_report();
    assert_eq!((report.reclaimed, report.kept, report.failed), (0, 1, 0));
}

#[test]
fn reconcile_does_not_run_without_roots() {
    let fx = Fixture::new("noroots");
    fs::create_dir_all(fx.control("slot-empty")).expect("control dir");
    let pool = SlotPool::new(PoolConfig::default());
    assert!(fx.control("slot-empty").is_dir());
    assert_eq!(pool.reconcile_report(), Default::default());
}
