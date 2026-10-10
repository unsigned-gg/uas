//! Worker slot lifecycle micro-benchmarks.
//!
//! Evaluates lease acquisition, worktree warming, suspension (freeze/thaw),
//! and lease release performance per Spec 0015 / Spec 0020.
#![allow(missing_docs)]

use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};

use cas_domain::{Budget, Lease, LeaseId, LeaseToken, PrincipalRef, SlotId, WorktreeRef};
use cas_pool::freezer::{CgroupFreezer, ResourceLimits};
use cas_pool::slot::{PoolConfig, SlotPool};
use criterion::{criterion_group, criterion_main, Criterion};

struct BenchDirGuard {
    path: PathBuf,
}

impl BenchDirGuard {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cas-pool-bench-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&path).expect("failed to create bench dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for BenchDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn make_lease(
    lease_id: &str,
    slot_id: &SlotId,
    token_str: &str,
    issued_at: u64,
    expires_at: u64,
) -> Lease {
    Lease::new(
        LeaseId::new(lease_id).expect("lease id"),
        slot_id.clone(),
        PrincipalRef::new("user:tester").expect("principal ref"),
        LeaseToken::new(token_str).expect("lease token"),
        issued_at,
        expires_at,
        Budget::new(1000, 3600).expect("budget"),
    )
    .expect("valid lease")
}

fn bench_slot_lifecycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("slot_lifecycle");

    group.bench_function("pool_create_and_warm", |b| {
        b.iter(|| {
            let pool = SlotPool::new(PoolConfig::default());
            let slot_id = SlotId::new("bench-cw-1").unwrap();
            let wt = WorktreeRef::new("wt-1").unwrap();
            pool.create_slot(slot_id.clone()).unwrap();
            pool.warm_slot(&slot_id, wt).unwrap();
            black_box(&pool);
        });
    });

    group.bench_function("lease_acquire_and_release", |b| {
        let config = PoolConfig {
            max_slots: 10,
            ..Default::default()
        };
        let pool = SlotPool::new(config);
        let slot_id = SlotId::new("bench-slot-1").unwrap();
        let wt = WorktreeRef::new("wt-1").unwrap();
        pool.create_slot(slot_id.clone()).unwrap();
        pool.warm_slot(&slot_id, wt).unwrap();

        let mut lease_idx = 0u64;

        b.iter(|| {
            lease_idx += 1;
            let lease_id = format!("ls-{lease_idx}");
            let token_str = format!("tok-{lease_idx:016x}");
            let lease = make_lease(
                &lease_id,
                &slot_id,
                &token_str,
                1_000_000 + lease_idx,
                1_000_000 + lease_idx + 300,
            );
            let token = lease.token.clone();
            pool.acquire_lease(lease).unwrap();
            pool.release_lease(&slot_id, &token).unwrap();
        });
    });

    group.bench_function("suspend_and_resume", |b| {
        let config = PoolConfig {
            max_slots: 10,
            ..Default::default()
        };
        let pool = SlotPool::new(config);
        let slot_id = SlotId::new("bench-slot-suspend").unwrap();
        let wt = WorktreeRef::new("wt-suspend").unwrap();
        pool.create_slot(slot_id.clone()).unwrap();
        pool.warm_slot(&slot_id, wt).unwrap();

        let lease = make_lease(
            "lease-suspend",
            &slot_id,
            "tok-suspend-0001",
            1_000_000,
            1_003_600,
        );
        pool.acquire_lease(lease).unwrap();

        b.iter(|| {
            pool.suspend_slot(&slot_id).unwrap();
            pool.resume_slot(&slot_id).unwrap();
        });
    });

    group.bench_function("cgroup_configure_limits", |b| {
        let guard = BenchDirGuard::new("cgroup-limits");
        let slot_id = SlotId::new("bench-cg-slot").unwrap();
        let child_dir = guard.path().join(format!("slot-{}", slot_id));
        fs::create_dir_all(&child_dir).unwrap();
        fs::write(child_dir.join("cgroup.freeze"), "0\n").unwrap();
        fs::write(child_dir.join("cgroup.procs"), "").unwrap();
        fs::write(child_dir.join("cpu.max"), "max 100000\n").unwrap();

        let freezer = CgroupFreezer::new(guard.path(), slot_id).unwrap();
        let limits = ResourceLimits {
            cpu_quota_us: Some(200_000),
            cpu_period_us: Some(100_000),
            memory_high_bytes: None,
            memory_max_bytes: None,
        };

        b.iter(|| {
            freezer.configure_limits(black_box(&limits)).unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, bench_slot_lifecycle);
criterion_main!(benches);
