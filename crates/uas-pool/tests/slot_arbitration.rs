// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Spec 0036 multi-process tests: two processes share one host ceiling.
//!
//! Helper children are this test binary re-run through `std::env::current_exe()`. A child that
//! cannot start fails the test; nothing here skips (REQ-0036-12).
#![allow(missing_docs)]

mod common;

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::TempDirGuard;
use uas_domain::{HostCapacity, SlotAcquirePolicy, SlotId};
use uas_pool::slot::arbitration::HostSlotArbiter;
use uas_pool::{HostProbe, PoolConfig, PoolError, SlotPool};

const ENV_DIR: &str = "UAS_ARB_HELPER_DIR";
const ENV_HOLD: &str = "UAS_ARB_HELPER_HOLD";

/// Not a test of its own: when the parent sets the env, this process holds `HOLD` host slots
/// until its stdin closes or it is killed. Without the env it returns at once.
#[test]
fn helper_hold_slots() {
    let (Ok(dir), Ok(n)) = (std::env::var(ENV_DIR), std::env::var(ENV_HOLD)) else {
        return;
    };
    let n: usize = n.parse().expect("hold count");
    let arbiter = HostSlotArbiter::new(dir, HostCapacity::new(2).unwrap());
    let leases: Vec<_> = (0..n)
        .map(|_| arbiter.try_acquire().expect("helper acquire"))
        .collect();
    println!("HELD");
    let mut sink = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut sink);
    drop(leases);
}

/// Start the helper at `exe`. Panics when it cannot start or never reports `HELD`.
fn spawn_holder_at(exe: &Path, dir: &Path, n: usize) -> Child {
    let mut child = Command::new(exe)
        .args([
            "--exact",
            "helper_hold_slots",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ENV_DIR, dir)
        .env(ENV_HOLD, n.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|e| panic!("cannot spawn helper {}: {e}", exe.display()));
    let mut lines = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
    let held = lines
        .by_ref()
        .map_while(Result::ok)
        .any(|l| l.trim().ends_with("HELD"));
    assert!(held, "helper exited without holding its slots");
    // Keep draining so the child never blocks on a full pipe.
    std::thread::spawn(move || lines.for_each(drop));
    child
}

fn spawn_holder(dir: &Path, n: usize) -> Child {
    spawn_holder_at(&std::env::current_exe().expect("current_exe"), dir, n)
}

fn slot_dir(guard: &TempDirGuard) -> PathBuf {
    guard.path().join("slots")
}

/// REQ-0036-01, REQ-0036-05: a second process is refused while the first holds both slots.
#[test]
fn second_process_gets_host_capacity_exceeded() {
    let tmp = TempDirGuard::new("arb-full");
    let dir = slot_dir(&tmp);
    let arbiter = HostSlotArbiter::new(&dir, HostCapacity::new(2).unwrap());
    // Create the directory with the right mode before the child runs.
    drop(arbiter.try_acquire().expect("create dir"));

    let mut child = spawn_holder(&dir, 2);
    let err = arbiter.try_acquire().expect_err("host is full");
    assert!(matches!(err, PoolError::HostCapacityExceeded(2)), "{err:?}");
    child.kill().expect("kill helper");
    child.wait().expect("reap helper");
}

/// REQ-0036-01: the end-to-end defect. A pool in this process cannot admit a slot while another
/// process holds a host slot, and a drain frees one.
#[test]
fn pool_create_slot_respects_the_other_process() {
    let tmp = TempDirGuard::new("arb-pool");
    let dir = slot_dir(&tmp);
    drop(
        HostSlotArbiter::new(&dir, HostCapacity::new(2).unwrap())
            .try_acquire()
            .expect("create dir"),
    );
    let mut child = spawn_holder(&dir, 1);

    let host = HostProbe {
        mem_total: Some(64 << 30),
        env: Box::new(|_| None),
        free_bytes: Box::new(|_| Ok(1 << 40)),
    };
    let cfg = PoolConfig {
        max_slots: 2,
        host_max_slots: Some(2),
        host_slot_dir: Some(dir.clone()),
        ..PoolConfig::default()
    };
    let pool = SlotPool::with_host(cfg, host);
    pool.create_slot(SlotId::new("a").unwrap())
        .expect("one host slot is free");
    let err = pool
        .create_slot(SlotId::new("b").unwrap())
        .expect_err("host full");
    assert!(matches!(err, PoolError::HostCapacityExceeded(2)), "{err:?}");

    pool.drain_slot(&SlotId::new("a").unwrap()).expect("drain");
    pool.create_slot(SlotId::new("b").unwrap())
        .expect("drain freed the host slot");
    child.kill().expect("kill helper");
    child.wait().expect("reap helper");
}

/// REQ-0036-07: after the holder is SIGKILLed, a waiter gets the slot within one max poll interval.
#[test]
fn killed_holder_releases_slot_to_waiter_within_500ms() {
    let tmp = TempDirGuard::new("arb-kill");
    let dir = slot_dir(&tmp);
    let arbiter = HostSlotArbiter::new(&dir, HostCapacity::new(2).unwrap());
    drop(arbiter.try_acquire().expect("create dir"));
    let mut child = spawn_holder(&dir, 2);

    let waiter = std::thread::spawn({
        let arbiter = arbiter.clone();
        move || {
            let lease = arbiter
                .acquire(SlotAcquirePolicy::WaitBounded, Duration::from_secs(10))
                .expect("waiter acquires after the kill");
            (Instant::now(), lease)
        }
    });
    // Long enough for the waiter to back off to its 500 ms ceiling.
    std::thread::sleep(Duration::from_millis(1600));
    assert!(
        !waiter.is_finished(),
        "waiter got a slot while the host was full"
    );
    let killed_at = Instant::now();
    child.kill().expect("SIGKILL helper");
    child.wait().expect("reap helper");
    let (got_at, lease) = waiter.join().expect("waiter thread");
    let took = got_at.duration_since(killed_at);
    assert!(
        took <= Duration::from_millis(650),
        "waiter needed {took:?} after the kill"
    );
    assert_eq!(lease.index().get(), 0);
}

/// REQ-0036-12: a helper that cannot start fails the harness instead of skipping.
#[test]
#[should_panic(expected = "cannot spawn helper")]
fn harness_fails_when_the_child_cannot_spawn() {
    let tmp = TempDirGuard::new("arb-nospawn");
    spawn_holder_at(Path::new("/nonexistent/uas-helper"), &slot_dir(&tmp), 1)
        .wait()
        .ok();
}
