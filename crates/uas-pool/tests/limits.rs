// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Spec 0041 P5: the pool resolves, logs and applies the slot limits and the disk floor.

mod common;

use std::fs;
use std::io;

use common::TempDirGuard;
use uas_domain::SlotId;
use uas_pool::limits::{LimitOverrides, LimitSource, ENV_MEMORY_MAX};
use uas_pool::{HostProbe, PoolConfig, PoolError, SlotPool};

const G: u64 = 1024 * 1024 * 1024;

fn host(mem_total: Option<u64>, free: u64) -> HostProbe {
    HostProbe {
        mem_total,
        env: Box::new(|_| None),
        free_bytes: Box::new(move |_| Ok(free)),
    }
}

fn slot(name: &str) -> SlotId {
    SlotId::new(name).expect("slot id")
}

/// A base with a pre-created `slot-<id>` tree holding the control files as plain files.
fn cgroup_tree(tmp: &TempDirGuard, id: &str) -> std::path::PathBuf {
    let base = tmp.path().join("cg");
    let dir = base.join(format!("slot-{id}"));
    fs::create_dir_all(&dir).expect("slot dir");
    for f in [
        "cpu.max",
        "memory.max",
        "memory.high",
        "memory.swap.max",
        "pids.max",
    ] {
        fs::write(dir.join(f), "").expect("control file");
    }
    base
}

fn cgroup_config(base: std::path::PathBuf) -> PoolConfig {
    PoolConfig {
        cgroup_base: Some(base),
        ..PoolConfig::default()
    }
}

#[test]
fn limits_create_slot_refuses_memory_low_and_creates_no_cgroup() {
    let tmp = TempDirGuard::new("limits-memlow");
    let base = tmp.path().join("cg");
    fs::create_dir_all(&base).unwrap();
    let pool = SlotPool::with_host(cgroup_config(base.clone()), host(Some(6 * G), 100 * G));
    let err = pool.create_slot(slot("a")).unwrap_err();
    assert!(
        matches!(err, PoolError::MemoryLow { per_slot_bytes, floor_bytes }
            if per_slot_bytes == G + G / 2 && floor_bytes == 2 * G),
        "got {err:?}"
    );
    assert!(!base.join("slot-a").exists());
    assert_eq!(pool.slot_count(), 0);
}

#[test]
fn limits_memory_low_refuses_without_a_cgroup_base() {
    let pool = SlotPool::with_host(PoolConfig::default(), host(Some(6 * G), 100 * G));
    let err = pool.create_slot(slot("a")).unwrap_err();
    assert!(matches!(err, PoolError::MemoryLow { .. }), "got {err:?}");
    assert_eq!(pool.slot_count(), 0);
}

#[test]
fn limits_default_pool_checks_the_disk_floor() {
    let pool = SlotPool::with_host(PoolConfig::default(), host(Some(54 * G), G));
    let err = pool.create_slot(slot("a")).unwrap_err();
    assert!(
        matches!(err, PoolError::DiskLow { free_bytes, .. } if free_bytes == G),
        "got {err:?}"
    );
}

#[test]
fn limits_freezer_error_fails_closed_with_a_cgroup_base() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDirGuard::new("limits-failclosed");
    let base = cgroup_tree(&tmp, "a");
    let file = base.join("slot-a").join("memory.max");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
    if fs::write(&file, "x").is_ok() {
        return; // running as root: a read-only file does not fail the write
    }
    let pool = SlotPool::with_host(cgroup_config(base), host(Some(54 * G), 100 * G));
    let err = pool.create_slot(slot("a")).unwrap_err();
    assert!(matches!(err, PoolError::Freezer(_)), "got {err:?}");
    assert_eq!(pool.slot_count(), 0);
}

#[test]
fn limits_create_slot_refuses_disk_low() {
    let tmp = TempDirGuard::new("limits-disklow");
    let cfg = PoolConfig {
        disk_root: Some(tmp.path().to_path_buf()),
        ..PoolConfig::default()
    };
    let pool = SlotPool::with_host(cfg, host(Some(54 * G), G));
    let err = pool.create_slot(slot("a")).unwrap_err();
    assert!(
        matches!(err, PoolError::DiskLow { free_bytes, min_free_bytes }
            if free_bytes == G && min_free_bytes == 20 * G),
        "got {err:?}"
    );
    assert_eq!(pool.slot_count(), 0);
}

#[test]
fn limits_disk_floor_override_and_memory_refusal_are_independent() {
    let tmp = TempDirGuard::new("limits-diskov");
    let cfg = PoolConfig {
        disk_root: Some(tmp.path().to_path_buf()),
        limits: LimitOverrides {
            min_free_bytes: Some(G / 2),
            ..Default::default()
        },
        ..PoolConfig::default()
    };
    let pool = SlotPool::with_host(cfg, host(Some(54 * G), G));
    pool.create_slot(slot("a"))
        .expect("1 GiB free clears a 0.5 GiB floor");
}

#[test]
fn limits_disk_probe_error_surfaces() {
    let tmp = TempDirGuard::new("limits-diskerr");
    let cfg = PoolConfig {
        disk_root: Some(tmp.path().to_path_buf()),
        ..PoolConfig::default()
    };
    let h = HostProbe {
        mem_total: Some(54 * G),
        env: Box::new(|_| None),
        free_bytes: Box::new(|_| Err(io::Error::other("statvfs failed"))),
    };
    let err = SlotPool::with_host(cfg, h)
        .create_slot(slot("a"))
        .unwrap_err();
    assert!(matches!(err, PoolError::DiskProbe(_)), "got {err:?}");
}

#[test]
fn limits_start_logs_source_per_case() {
    let h = |env: Option<&'static str>| HostProbe {
        mem_total: Some(54 * G),
        env: Box::new(move |k| {
            (k == ENV_MEMORY_MAX)
                .then(|| env.map(String::from))
                .flatten()
        }),
        free_bytes: Box::new(|_| Ok(100 * G)),
    };
    let pool = SlotPool::with_host(PoolConfig::default(), h(None));
    assert_eq!(pool.memory_source(), Some(LimitSource::Formula));
    let pool = SlotPool::with_host(PoolConfig::default(), h(Some("4294967296")));
    assert_eq!(pool.memory_source(), Some(LimitSource::Env));
    let cfg = PoolConfig {
        limits: LimitOverrides {
            memory_max_bytes: Some(8 * G),
            ..Default::default()
        },
        ..PoolConfig::default()
    };
    let pool = SlotPool::with_host(cfg, h(Some("4294967296")));
    assert_eq!(pool.memory_source(), Some(LimitSource::Config));
    assert_eq!(
        pool.resolved_limits().unwrap().memory_max_bytes,
        Some(8 * G)
    );
}

#[test]
fn limits_create_slot_writes_resolved_values() {
    let tmp = TempDirGuard::new("limits-write");
    let base = cgroup_tree(&tmp, "a");
    let pool = SlotPool::with_host(cgroup_config(base.clone()), host(Some(54 * G), 100 * G));
    pool.create_slot(slot("a")).expect("create");
    let read = |f: &str| fs::read_to_string(base.join("slot-a").join(f)).unwrap();
    assert_eq!(read("memory.max").trim(), (13 * G + G / 2).to_string());
    assert_eq!(read("memory.high").trim(), "11596411699");
    assert_eq!(read("memory.swap.max").trim(), "0");
    assert_eq!(read("pids.max").trim(), "4096");
}
