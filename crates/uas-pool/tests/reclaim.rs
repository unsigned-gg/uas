// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Integration tests for REQ-0015-06: slot reclamation, worker reaping, and gitoxide worktree reset.
//!
//! WHILE reclaiming an expired or finished slot, the pool shall reap child processes
//! and reset worktree state (spec 0015 D2, D6, REQ-0015-06).

mod common;

use common::{create_test_base_repo, TempDirGuard};
use std::fs;
use uas_domain::{Budget, Lease, LeaseId, LeaseToken, PrincipalRef, SlotId, SlotState};
use uas_pool::{PoolConfig, SlotPool, WarmWorktree};

fn make_lease(lease_id: &str, slot_id: &str) -> Lease {
    Lease::new(
        LeaseId::new(lease_id).expect("lease id"),
        SlotId::new(slot_id).expect("slot id"),
        PrincipalRef::new("user:tester").expect("principal ref"),
        LeaseToken::new("token-abc-12345678").expect("lease token"),
        1000,
        2000,
        Budget::new(1000, 3600).expect("budget"),
    )
    .expect("valid lease")
}

#[test]
fn reclaim_slot_reaps_worker_and_resets_worktree_clean() {
    let base_guard = TempDirGuard::new("base");
    create_test_base_repo(base_guard.path()).expect("base repo created");

    let wt_guard = TempDirGuard::new("wt");
    let slot_id = SlotId::new("slot-reclaim-01").unwrap();

    let worktree = WarmWorktree::provision(
        base_guard.path(),
        slot_id.clone(),
        wt_guard.path(),
        "uas-slot-reclaim-branch",
    )
    .expect("worktree provisioned");

    let config = PoolConfig {
        max_slots: 2,
        idle_suspend_duration: None,
        cgroup_base: None,
        reconcile: None,
        ..PoolConfig::default()
    };
    let pool = SlotPool::new(config);

    // Create slot and warm it with worktree reference
    pool.create_slot(slot_id.clone()).expect("create slot");
    pool.warm_slot(&slot_id, worktree.worktree_ref().clone())
        .expect("warm slot");
    assert_eq!(pool.slot_state(&slot_id).unwrap(), SlotState::Warm);

    // Acquire lease -> transitions to Leased
    let lease = make_lease("lease-01", slot_id.as_str());
    pool.acquire_lease(lease).expect("lease acquired");
    assert_eq!(pool.slot_state(&slot_id).unwrap(), SlotState::Leased);

    // Simulate dirty worker execution: modify existing file and write an untracked file
    let readme_path = wt_guard.path().join("README.md");
    fs::write(&readme_path, b"DIRTY UNCOMMITTED DATA").expect("modify readme");

    let untracked_path = wt_guard.path().join("untracked_scratch.txt");
    fs::write(&untracked_path, b"ephemeral scratch file").expect("write untracked");
    assert!(untracked_path.exists());

    // Execute slot reclamation
    pool.reclaim_slot(&slot_id, Some(&worktree))
        .expect("reclaim slot succeeds");

    // Slot state returns to Warm
    assert_eq!(pool.slot_state(&slot_id).unwrap(), SlotState::Warm);

    // Worktree state is cleanly restored: README restored to clean HEAD, untracked removed
    let restored_readme = fs::read_to_string(&readme_path).expect("read restored readme");
    assert!(
        restored_readme.contains("Initial commit content."),
        "README was not restored: {restored_readme}"
    );
    assert!(
        !untracked_path.exists(),
        "untracked file was not purged during reset_clean"
    );

    // The slot can immediately be leased again
    let lease2 = make_lease("lease-02", slot_id.as_str());
    pool.acquire_lease(lease2).expect("second lease acquired");
    assert_eq!(pool.slot_state(&slot_id).unwrap(), SlotState::Leased);
}

#[test]
fn reclaim_suspended_slot_thaws_and_returns_to_warm() {
    let slot_id = SlotId::new("slot-reclaim-02").unwrap();
    let config = PoolConfig {
        max_slots: 2,
        idle_suspend_duration: None,
        cgroup_base: None,
        reconcile: None,
        ..PoolConfig::default()
    };
    let pool = SlotPool::new(config);

    pool.create_slot(slot_id.clone()).expect("create slot");
    let wt_ref = uas_domain::WorktreeRef::new("slots/slot-reclaim-02").unwrap();
    pool.warm_slot(&slot_id, wt_ref).expect("warm slot");

    let lease = make_lease("lease-suspend-01", slot_id.as_str());
    pool.acquire_lease(lease).expect("acquire lease");
    assert_eq!(pool.slot_state(&slot_id).unwrap(), SlotState::Leased);

    // Suspend slot
    pool.suspend_slot(&slot_id).expect("suspend slot");
    assert_eq!(pool.slot_state(&slot_id).unwrap(), SlotState::Suspended);

    // Reclaim suspended slot (without worktree reset)
    pool.reclaim_slot(&slot_id, None)
        .expect("reclaim suspended slot");
    assert_eq!(pool.slot_state(&slot_id).unwrap(), SlotState::Warm);
}

#[test]
fn reclaim_slot_kills_worker_process_group_and_reaps_zombies() {
    use std::time::{Duration, Instant};

    let slot_id = SlotId::new("slot-reclaim-subreaper").unwrap();
    let config = PoolConfig {
        max_slots: 2,
        idle_suspend_duration: None,
        cgroup_base: None,
        reconcile: None,
        ..PoolConfig::default()
    };
    let pool = SlotPool::new(config);

    pool.create_slot(slot_id.clone()).expect("create slot");
    let wt_ref = uas_domain::WorktreeRef::new("slots/slot-reclaim-subreaper").unwrap();
    pool.warm_slot(&slot_id, wt_ref).expect("warm slot");

    let lease = make_lease("lease-proc-01", slot_id.as_str());
    pool.acquire_lease(lease).expect("acquire lease");
    assert_eq!(pool.slot_state(&slot_id).unwrap(), SlotState::Leased);

    let pid_file = std::env::temp_dir().join(format!("uas-test-child-{}.pid", std::process::id()));
    let _ = fs::remove_file(&pid_file);

    let mut cmd = std::process::Command::new("sh");
    cmd.args([
        "-c",
        &format!("sleep 60 & echo $! > {} && wait", pid_file.display()),
    ]);

    let worker = pool
        .spawn_worker(&slot_id, cmd, &uas_pool::LaunchConfig::default())
        .expect("spawn worker");

    // Wait for the grandchild pid to be written
    let start = Instant::now();
    let mut grandchild_raw = None;
    while start.elapsed() < Duration::from_secs(3) {
        if let Ok(content) = fs::read_to_string(&pid_file) {
            let trimmed = content.trim();
            if let Ok(p) = trimmed.parse::<i32>() {
                if p > 1 {
                    grandchild_raw = Some(p);
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = fs::remove_file(&pid_file);

    let grandchild_pid = grandchild_raw.expect("grandchild pid written");
    let grandchild = rustix::process::Pid::from_raw(grandchild_pid).expect("valid pid");

    // Verify grandchild is initially running
    assert!(
        rustix::process::test_kill_process(grandchild).is_ok(),
        "grandchild process should be alive before reclaim"
    );

    // Reclaim the slot: sends KILL to the group, resets state, and reaps zombies
    pool.reclaim_slot(&slot_id, None).expect("reclaim slot");

    // Worker finish to release child handle
    let _ = worker.finish();

    // Verify grandchild was terminated
    let start = Instant::now();
    let mut dead = false;
    while start.elapsed() < Duration::from_secs(3) {
        pool.reap_zombies();
        if rustix::process::test_kill_process(grandchild).is_err() {
            dead = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        dead,
        "grandchild process should have been terminated and reaped by reclaim_slot"
    );
    assert_eq!(pool.slot_state(&slot_id).unwrap(), SlotState::Warm);
}
