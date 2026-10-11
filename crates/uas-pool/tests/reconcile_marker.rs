// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Spec 0040 D1, REQ-0040-01 and REQ-0040-12: the locked slot marker written
//! before the worktree directory and the branch exist.

mod common;

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use common::{create_test_base_repo, TempDirGuard};
use uas_domain::SlotId;
use uas_pool::WarmWorktree;

const MARKER: &str = "uas-pool.json";

fn marker_path(base: &Path, slot: &str) -> PathBuf {
    base.join(".git").join("worktrees").join(slot).join(MARKER)
}

fn locked_elsewhere(path: &Path) -> bool {
    let f = OpenOptions::new()
        .read(true)
        .open(path)
        .expect("open marker");
    matches!(f.try_lock(), Err(std::fs::TryLockError::WouldBlock))
}

#[test]
fn reconcile_marker_exists_locked_and_holds_six_fields() {
    let tmp = TempDirGuard::new("marker-fields");
    let base = tmp.path().join("base");
    create_test_base_repo(&base).expect("base");
    let wt = WarmWorktree::provision(
        &base,
        SlotId::new("slot-01").expect("id"),
        tmp.path().join("wt"),
        "uas/slot-01",
    )
    .expect("provision");

    let marker = marker_path(&base, "slot-01");
    assert!(marker.is_file(), "marker must exist after provision");
    assert!(locked_elsewhere(&marker), "marker must be locked");
    assert!(
        !marker.with_file_name("uas-pool.json.tmp").exists(),
        "tmp must be renamed away"
    );

    let v: serde_json::Value =
        serde_json::from_slice(&fs::read(&marker).expect("read")).expect("json");
    assert_eq!(v["slot_id"], "slot-01");
    assert_eq!(v["branch"], "uas/slot-01");
    assert_eq!(v["cgroup"], "slot-slot-01");
    assert_eq!(
        Path::new(v["worktree_dir"].as_str().expect("worktree_dir")),
        wt.worktree_dir()
    );
    assert_eq!(v["pool_pid"], std::process::id());
    assert!(v["provisioned_at"].as_u64().expect("provisioned_at") > 1_600_000_000);
    assert_eq!(v.as_object().expect("object").len(), 6);
}

#[test]
fn reconcile_marker_descriptor_is_close_on_exec() {
    let tmp = TempDirGuard::new("marker-cloexec");
    let base = tmp.path().join("base");
    create_test_base_repo(&base).expect("base");
    let _wt = WarmWorktree::provision(
        &base,
        SlotId::new("slot-01").expect("id"),
        tmp.path().join("wt"),
        "uas/slot-01",
    )
    .expect("provision");

    let marker = fs::canonicalize(marker_path(&base, "slot-01")).expect("canon");
    let mut checked = false;
    for entry in fs::read_dir("/proc/self/fd").expect("fd dir") {
        let entry = entry.expect("entry");
        if fs::read_link(entry.path()).ok().as_deref() != Some(marker.as_path()) {
            continue;
        }
        let info = fs::read_to_string(format!(
            "/proc/self/fdinfo/{}",
            entry.file_name().to_string_lossy()
        ))
        .unwrap_or_default();
        let flags = info
            .lines()
            .find_map(|l| l.strip_prefix("flags:"))
            .map(|s| u32::from_str_radix(s.trim(), 8).expect("octal flags"))
            .expect("flags line");
        assert_ne!(flags & 0o2_000_000, 0, "O_CLOEXEC must be set");
        checked = true;
    }
    assert!(
        checked,
        "no descriptor for the marker found under /proc/self/fd"
    );
}

#[test]
fn reconcile_marker_precedes_branch_and_worktree_dir() {
    let tmp = TempDirGuard::new("marker-order");
    let base = tmp.path().join("base");
    create_test_base_repo(&base).expect("base");
    let blocker = tmp.path().join("blocker");
    fs::write(&blocker, b"x").expect("blocker");

    let res = WarmWorktree::provision(
        &base,
        SlotId::new("slot-01").expect("id"),
        blocker.join("wt"),
        "uas/slot-01",
    );
    assert!(res.is_err(), "worktree dir under a regular file must fail");
    assert!(
        marker_path(&base, "slot-01").is_file(),
        "marker written first"
    );
    assert!(
        !base.join(".git/refs/heads/uas/slot-01").exists(),
        "branch must not exist yet"
    );
    assert!(!blocker.join("wt").exists());
}

#[test]
fn reconcile_marker_existing_live_marker_blocks_provision_untouched() {
    let tmp = TempDirGuard::new("marker-noreplace");
    let base = tmp.path().join("base");
    create_test_base_repo(&base).expect("base");
    let dir = base.join(".git/worktrees/slot-01");
    fs::create_dir_all(&dir).expect("dir");
    let existing = dir.join(MARKER);
    fs::write(&existing, b"live-owner").expect("seed");
    let held: File = OpenOptions::new().read(true).open(&existing).expect("open");
    held.try_lock().expect("hold lock");

    let res = WarmWorktree::provision(
        &base,
        SlotId::new("slot-01").expect("id"),
        tmp.path().join("wt"),
        "uas/slot-01",
    );
    assert!(res.is_err(), "provision must not overwrite a live marker");
    assert_eq!(fs::read(&existing).expect("read"), b"live-owner");
    assert!(!tmp.path().join("wt").exists());
    assert!(!dir.join("uas-pool.json.tmp").exists(), "tmp cleaned up");
}

#[test]
fn reconcile_marker_teardown_releases_lock_and_removes_marker() {
    let tmp = TempDirGuard::new("marker-teardown");
    let base = tmp.path().join("base");
    create_test_base_repo(&base).expect("base");
    let wt = WarmWorktree::provision(
        &base,
        SlotId::new("slot-01").expect("id"),
        tmp.path().join("wt"),
        "uas/slot-01",
    )
    .expect("provision");

    let marker = marker_path(&base, "slot-01");
    // A second open file description on the same inode: its lock outcome shows
    // whether the pool still holds the first.
    let probe = OpenOptions::new().read(true).open(&marker).expect("open");
    assert!(matches!(
        probe.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));

    wt.teardown().expect("teardown");
    assert!(!marker.exists(), "marker removed with the control dir");
    probe
        .try_lock()
        .expect("lock must be released after teardown");
}
