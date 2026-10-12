// SPDX-License-Identifier: Apache-2.0 OR MIT
//! End-to-end integration tests for Spec 0015: slot lifecycle, capacity, gix reset, clamp, and audit degradation.

mod common;

use common::{create_test_base_repo, TempDirGuard};
use std::fs;
use std::process::{Command, Stdio};
use uas_domain::{AuditStatus, Budget, Lease, LeaseId, LeaseToken, PrincipalRef, SlotId};
use uas_ebpf::clamp::{nice_of, pgid_of, WORKER_NICE};
use uas_pool::{
    spawn_worker, AuditDegradation, LaunchConfig, PoolConfig, PoolError, SlotPool, WarmWorktree,
};

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
fn slot_lifecycle_enforces_capacity_ceiling() {
    let config = PoolConfig {
        max_slots: 2,
        idle_suspend_duration: None,
        cgroup_base: None,
        reconcile: None,
        ..PoolConfig::default()
    };
    let pool = SlotPool::new(config);

    let s1 = SlotId::new("slot-cap-1").unwrap();
    let s2 = SlotId::new("slot-cap-2").unwrap();
    let s3 = SlotId::new("slot-cap-3").unwrap();

    pool.create_slot(s1).expect("create s1");
    pool.create_slot(s2).expect("create s2");

    let err = pool.create_slot(s3).unwrap_err();
    match err {
        PoolError::CapacityExhausted(max) => assert_eq!(max, 2),
        other => panic!("expected CapacityExhausted, got {:?}", other),
    }
}

#[test]
fn slot_lifecycle_reclaim_restores_clean_worktree() {
    let base_guard = TempDirGuard::new("base-wt");
    create_test_base_repo(base_guard.path()).expect("base repo created");

    let wt_guard = TempDirGuard::new("wt-slot");
    let slot_id = SlotId::new("slot-wt-01").unwrap();

    let worktree = WarmWorktree::provision(
        base_guard.path(),
        slot_id.clone(),
        wt_guard.path(),
        "uas-branch-wt",
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

    pool.create_slot(slot_id.clone()).expect("create slot");
    pool.warm_slot(&slot_id, worktree.worktree_ref().clone())
        .expect("warm slot");

    let lease = make_lease("lease-wt", slot_id.as_str());
    pool.acquire_lease(lease).expect("lease acquired");

    // Modify file
    let readme = wt_guard.path().join("README.md");
    fs::write(&readme, b"MODIFIED UNSTAGED").expect("modify readme");

    // Reclaim
    pool.reclaim_slot(&slot_id, Some(&worktree))
        .expect("reclaim succeeds");

    let restored = fs::read_to_string(&readme).expect("read restored");
    assert!(restored.contains("Initial commit content."));
}

#[test]
fn slot_lifecycle_worker_spawns_with_nice_19() {
    let mut cmd = Command::new("sleep");
    cmd.arg("1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let worker =
        spawn_worker(cmd, &LaunchConfig::default(), "uncapped".into(), None).expect("spawn worker");
    let pid = worker.id();

    let nice = nice_of(pid).expect("nice_of pid");
    let pgid = pgid_of(pid).expect("pgid_of pid");
    assert_eq!(nice, WORKER_NICE);
    assert_eq!(pgid, pid, "worker must lead its own process group");

    let (status, _) = worker.finish().expect("finish");
    assert!(status.success());
}

#[test]
fn slot_lifecycle_worker_without_daemon_degrades_to_unattested() {
    let mut cmd = Command::new("true");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let worker =
        spawn_worker(cmd, &LaunchConfig::default(), "uncapped".into(), None).expect("spawn worker");
    let (status, audit) = worker.finish().expect("finish");

    assert!(status.success());
    assert_eq!(audit.audit, AuditStatus::Unattested { dropped_events: 0 });
    assert_eq!(audit.degradation, Some(AuditDegradation::NoSocket));
}
