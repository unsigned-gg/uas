// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Spec 0036: Multi-process host slot arbitration and capacity enforcement.

mod common;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use common::TempDirGuard;
use uas_domain::{HostSlotIndex, SlotAcquirePolicy, SlotId};
use uas_pool::{HostSlotArbiter, PoolConfig, PoolError, SlotPool};

const CHILD_ENV: &str = "UAS_ARBITRATION_CHILD";

/// Child fixture for multi-process concurrency testing (REQ-0036-01, REQ-0036-07).
#[test]
fn arbitration_child_fixture() {
    let Ok(slot_dir) = std::env::var(CHILD_ENV) else {
        return;
    };

    let arbiter = HostSlotArbiter::new(Some(PathBuf::from(slot_dir)), 2, SlotAcquirePolicy::Try)
        .expect("child arbiter init");

    let slot_id = SlotId::new("slot-child-held").unwrap();
    let _lease = arbiter.acquire(&slot_id).expect("child lease acquire");

    println!("ready");
    let _ = std::io::stdout().flush();

    loop {
        thread::sleep(Duration::from_millis(50));
    }
}

/// Spawns a child process fixture holding a host slot lock.
#[allow(clippy::zombie_processes)]
fn spawn_lock_holder(slot_dir: &Path) -> Child {
    let exe = std::env::current_exe().expect("current_exe available (REQ-0036-12)");
    let mut child = Command::new(exe)
        .args(["arbitration_child_fixture", "--exact", "--nocapture"])
        .env(CHILD_ENV, slot_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn child process");

    let stdout = child.stdout.take().expect("child stdout");
    for line in BufReader::new(stdout).lines() {
        if line.expect("read line") == "ready" {
            return child;
        }
    }
    panic!("child fixture exited without printing ready");
}

fn make_slot_dir(parent: &Path, name: &str) -> PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    let p = parent.join(name);
    fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
    p
}

#[test]
fn test_multi_process_capacity_exhaustion_and_recovery_on_kill() {
    let parent = TempDirGuard::new("uas-arbitration-multiprocess");
    let slot_dir = make_slot_dir(parent.path(), "slots");
    let arbiter = HostSlotArbiter::new(Some(slot_dir.clone()), 2, SlotAcquirePolicy::Try).unwrap();

    // 1. Acquire slot 0 in the parent process
    let parent_slot0 = SlotId::new("slot-parent-0").unwrap();
    let lease0 = arbiter.acquire(&parent_slot0).unwrap();
    assert_eq!(lease0.index(), HostSlotIndex::new(0));

    // 2. Spawn child process holding slot 1
    let mut child = spawn_lock_holder(&slot_dir);

    // 3. Parent attempts to acquire a 3rd slot when max is 2 -> HostCapacityExceeded (REQ-0036-05)
    let parent_slot2 = SlotId::new("slot-parent-2").unwrap();
    let err = arbiter.acquire(&parent_slot2).unwrap_err();
    assert!(matches!(err, PoolError::HostCapacityExceeded(2)));

    // 4. Kill the child process (SIGKILL)
    child.kill().expect("kill child");
    let _ = child.wait();

    // 5. Waiting parent process under WaitBounded acquires released slot within 500 ms (REQ-0036-07)
    let waiting_arbiter = HostSlotArbiter::new(
        Some(slot_dir),
        2,
        SlotAcquirePolicy::WaitBounded {
            timeout: Duration::from_secs(2),
        },
    )
    .unwrap();

    let start = Instant::now();
    let acquired = waiting_arbiter.acquire(&parent_slot2).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(acquired.index(), HostSlotIndex::new(1));
    assert!(
        elapsed <= Duration::from_millis(500),
        "recovery elapsed {elapsed:?} exceeded 500 ms ceiling"
    );
}

#[test]
fn test_directory_permissions_and_insecure_checks() {
    let parent = TempDirGuard::new("uas-arb-perms");
    let target = make_slot_dir(parent.path(), "real_dir");
    let link = parent.path().join("symlinked_dir");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let err = HostSlotArbiter::new(Some(link), 2, SlotAcquirePolicy::Try).unwrap_err();
    assert!(matches!(err, PoolError::DirectoryInsecure(_, _)));
}

#[test]
fn test_sequential_slot_indexing() {
    let parent = TempDirGuard::new("uas-arb-seq");
    let slot_dir = make_slot_dir(parent.path(), "slots");
    let arbiter = HostSlotArbiter::new(Some(slot_dir), 2, SlotAcquirePolicy::Try).unwrap();

    let s0 = SlotId::new("slot-seq-0").unwrap();
    let s1 = SlotId::new("slot-seq-1").unwrap();

    let l0 = arbiter.acquire(&s0).unwrap();
    let l1 = arbiter.acquire(&s1).unwrap();

    assert_eq!(l0.index(), HostSlotIndex::new(0));
    assert_eq!(l1.index(), HostSlotIndex::new(1));
}

#[test]
fn test_slot_lock_files_are_not_unlinked() {
    let parent = TempDirGuard::new("uas-arb-persist");
    let slot_dir = make_slot_dir(parent.path(), "slots");
    let arbiter = HostSlotArbiter::new(Some(slot_dir), 1, SlotAcquirePolicy::Try).unwrap();

    let s0 = SlotId::new("slot-pers-0").unwrap();
    let lease = arbiter.acquire(&s0).unwrap();
    let path = lease.path().to_path_buf();
    assert!(path.exists());

    drop(lease);
    assert!(
        path.exists(),
        "slot lock file must remain on disk after drop"
    );
}

#[test]
fn test_slot_lock_descriptor_is_close_on_exec() {
    let parent = TempDirGuard::new("uas-arb-cloexec");
    let slot_dir = make_slot_dir(parent.path(), "slots");
    let arbiter = HostSlotArbiter::new(Some(slot_dir), 1, SlotAcquirePolicy::Try).unwrap();

    let s0 = SlotId::new("slot-cloexec-0").unwrap();
    let lease = arbiter.acquire(&s0).unwrap();
    let lock_path = fs::canonicalize(lease.path()).expect("canonicalize");

    let mut checked = false;
    for entry in fs::read_dir("/proc/self/fd").expect("read fd") {
        let entry = entry.expect("entry");
        if fs::read_link(entry.path()).ok().as_deref() != Some(lock_path.as_path()) {
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
        assert_ne!(
            flags & 0o2_000_000,
            0,
            "O_CLOEXEC must be set on slot lock fd"
        );
        checked = true;
    }
    assert!(
        checked,
        "lock file descriptor must exist under /proc/self/fd"
    );
}

#[test]
fn test_wait_bounded_timeout_expires_and_fails() {
    let parent = TempDirGuard::new("uas-arb-timeout");
    let slot_dir = make_slot_dir(parent.path(), "slots");
    let arbiter = HostSlotArbiter::new(Some(slot_dir.clone()), 1, SlotAcquirePolicy::Try).unwrap();

    let s0 = SlotId::new("slot-timeout-0").unwrap();
    let _lease0 = arbiter.acquire(&s0).unwrap();

    let timeout_arbiter = HostSlotArbiter::new(
        Some(slot_dir),
        1,
        SlotAcquirePolicy::WaitBounded {
            timeout: Duration::from_millis(100),
        },
    )
    .unwrap();

    let s1 = SlotId::new("slot-timeout-1").unwrap();
    let start = Instant::now();
    let err = timeout_arbiter.acquire(&s1).unwrap_err();
    let elapsed = start.elapsed();

    assert!(matches!(err, PoolError::HostCapacityExceeded(1)));
    assert!(
        elapsed >= Duration::from_millis(90),
        "should wait at least ~100 ms, took {elapsed:?}"
    );
}

#[test]
fn test_slot_pool_allocate_enforces_host_capacity() {
    let parent = TempDirGuard::new("uas-pool-allocate");
    let slot_dir = make_slot_dir(parent.path(), "slots");

    let pool = SlotPool::new(PoolConfig {
        max_slots: 5,
        host_max_slots: 2,
        host_slot_dir: Some(slot_dir),
        slot_acquire_policy: SlotAcquirePolicy::Try,
        ..PoolConfig::default()
    });

    let s0 = SlotId::new("slot-pool-0").unwrap();
    let s1 = SlotId::new("slot-pool-1").unwrap();
    let s2 = SlotId::new("slot-pool-2").unwrap();

    pool.allocate(s0.clone()).expect("allocate s0");
    pool.allocate(s1.clone()).expect("allocate s1");

    // Internal capacity is 5, but host capacity is 2 -> HostCapacityExceeded(2)
    let err = pool.allocate(s2.clone()).unwrap_err();
    assert!(matches!(err, PoolError::HostCapacityExceeded(2)));

    // Removing s0 frees host slot 0
    pool.remove_slot(&s0).expect("remove s0");

    // Now s2 can be allocated
    pool.allocate(s2).expect("allocate s2 after release");
}
