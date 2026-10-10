//! CER-2719 follow-up: clamped worker launch and daemon tracking (spec 0015 D6, spec 0014 D14).
//!
//! The daemon runs over `FakeProbeHandle`, so no test needs `CAP_BPF`.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use cas_domain::{
    AuditStatus, Budget, Lease, LeaseId, LeaseToken, PrincipalRef, SlotId, WorktreeRef,
};
use cas_ebpf::{
    clamp::{nice_of, pgid_of, WORKER_NICE},
    daemon::Daemon,
    handle::FakeProbeHandle,
    EbpfError,
};
use cas_pool::{
    spawn_worker, AuditDegradation, CpuCap, LaunchConfig, PoolConfig, PoolError, SlotPool, Worker,
};

/// `run_hash` of an empty transcript: SHA-256 of nothing.
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

static NEXT: AtomicU32 = AtomicU32::new(0);

/// `sleep <secs>` with no stdio.
fn sleeper(secs: u32) -> Command {
    let mut cmd = Command::new("sleep");
    cmd.arg(secs.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

/// A running daemon on a fresh socket over a fake handle. Dropping it stops the server.
struct Fixture {
    path: PathBuf,
    handle: Arc<Mutex<FakeProbeHandle>>,
    server: tokio::task::JoinHandle<Result<(), EbpfError>>,
}

impl Fixture {
    async fn start() -> Self {
        Self::start_with(|daemon| daemon).await
    }

    async fn start_with(
        configure: impl FnOnce(Daemon<FakeProbeHandle>) -> Daemon<FakeProbeHandle>,
    ) -> Self {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("cas-pool-launch-{}-{n}.sock", std::process::id()));
        let handle = Arc::new(Mutex::new(FakeProbeHandle::new().expect("fake")));
        let daemon = configure(Daemon::new(Arc::clone(&handle), path.clone()));
        let server = tokio::spawn(daemon.serve());
        let fixture = Self {
            path,
            handle,
            server,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while std::os::unix::net::UnixStream::connect(&fixture.path).is_err() {
            assert!(Instant::now() < deadline, "daemon never listened");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        fixture
    }

    fn config(&self) -> LaunchConfig {
        LaunchConfig {
            daemon_socket: Some(self.path.clone()),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Runs a blocking closure off the async runtime.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.expect("join")
}

fn spawn_blocking_worker(secs: u32, cfg: LaunchConfig) -> Worker {
    spawn_worker(sleeper(secs), &cfg, "uncapped".into()).expect("spawn")
}

#[tokio::test(flavor = "multi_thread")]
async fn launch_worker_runs_at_nice_19_in_its_own_group() {
    let (pid, nice, pgid, status) = blocking(|| {
        let worker = spawn_blocking_worker(1, LaunchConfig::default());
        let pid = worker.id();
        let nice = nice_of(pid).expect("nice");
        let pgid = pgid_of(pid).expect("pgid");
        let (status, _) = worker.finish().expect("finish");
        (pid, nice, pgid, status)
    })
    .await;
    assert!(status.success());
    assert_eq!(pgid, pid, "the worker leads its own group");
    assert_eq!(nice, WORKER_NICE);
}

#[tokio::test(flavor = "multi_thread")]
async fn launch_without_socket_is_unattested_and_runs() {
    let (status, audit) = blocking(|| {
        spawn_blocking_worker(0, LaunchConfig::default())
            .finish()
            .expect("finish")
    })
    .await;
    assert!(status.success());
    assert_eq!(audit.audit, AuditStatus::Unattested { dropped_events: 0 });
    assert_eq!(audit.run_hash, None);
    assert_eq!(audit.degradation, Some(AuditDegradation::NoSocket));
}

#[tokio::test(flavor = "multi_thread")]
async fn launch_absent_socket_degrades_to_unavailable() {
    let missing = std::env::temp_dir().join(format!("cas-pool-absent-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&missing);
    let cfg = LaunchConfig {
        daemon_socket: Some(missing),
    };
    let (status, audit) =
        blocking(move || spawn_blocking_worker(0, cfg).finish().expect("finish")).await;
    assert!(status.success());
    assert_eq!(audit.audit, AuditStatus::Unattested { dropped_events: 0 });
    assert!(
        matches!(audit.degradation, Some(AuditDegradation::Unavailable(_))),
        "{:?}",
        audit.degradation
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn launch_tracks_the_worker_with_the_daemon() {
    let fx = Fixture::start().await;
    let cfg = fx.config();
    let worker = blocking(move || spawn_blocking_worker(1, cfg)).await;
    let child = worker.id();
    let own = std::process::id();
    assert_eq!(
        fx.handle.lock().expect("lock").tracked(),
        vec![(child, own)]
    );
    let (status, audit) = blocking(move || worker.finish().expect("finish")).await;
    assert!(status.success());
    assert_eq!(audit.audit, AuditStatus::Attested);
    assert_eq!(audit.run_hash.as_deref(), Some(EMPTY_SHA256));
    assert_eq!(audit.degradation, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn launch_quota_refusal_is_typed_and_non_fatal() {
    if rustix::process::geteuid().is_root() {
        eprintln!("skipped: running as root, quota does not apply");
        return;
    }
    let fx = Fixture::start_with(|daemon| daemon.with_track_quota(0)).await;
    let cfg = fx.config();
    let (status, audit) =
        blocking(move || spawn_blocking_worker(0, cfg).finish().expect("finish")).await;
    assert!(status.success());
    assert!(
        matches!(&audit.degradation, Some(AuditDegradation::Refused(r)) if r.contains("track quota")),
        "{:?}",
        audit.degradation
    );
    assert_eq!(audit.audit, AuditStatus::Unattested { dropped_events: 0 });
}

#[tokio::test(flavor = "multi_thread")]
async fn launch_foreign_namespace_refusal_is_typed_and_non_fatal() {
    let fx = Fixture::start_with(|daemon| daemon.with_pid_namespace([1, 1])).await;
    let cfg = fx.config();
    let (status, audit) =
        blocking(move || spawn_blocking_worker(0, cfg).finish().expect("finish")).await;
    assert!(status.success());
    assert!(
        matches!(&audit.degradation, Some(AuditDegradation::Refused(r)) if r.contains("namespace")),
        "{:?}",
        audit.degradation
    );
    assert_eq!(audit.audit, AuditStatus::Unattested { dropped_events: 0 });
}

#[tokio::test(flavor = "multi_thread")]
async fn launch_dropped_events_unattest_the_run() {
    let fx = Fixture::start().await;
    let cfg = fx.config();
    let worker = blocking(move || spawn_blocking_worker(1, cfg)).await;
    // A drop while the worker runs is charged to its window.
    fx.handle.lock().expect("lock").add_dropped(3);
    let (status, audit) = blocking(move || worker.finish().expect("finish")).await;
    assert!(status.success());
    assert_eq!(audit.audit, AuditStatus::Unattested { dropped_events: 3 });
    assert_eq!(audit.run_hash.as_deref(), Some(EMPTY_SHA256));
}

#[tokio::test(flavor = "multi_thread")]
async fn launch_attested_needs_tracking_and_a_clean_receipt() {
    let fx = Fixture::start().await;
    let cfg = fx.config();
    let tracked = blocking(move || spawn_blocking_worker(0, cfg).finish().expect("finish"))
        .await
        .1;
    assert_eq!(tracked.audit, AuditStatus::Attested);
    let untracked = blocking(|| {
        spawn_blocking_worker(0, LaunchConfig::default())
            .finish()
            .expect("finish")
    })
    .await
    .1;
    assert_eq!(
        untracked.audit,
        AuditStatus::Unattested { dropped_events: 0 }
    );
}

fn make_lease(lease_id: &str, slot_id: &str, token: &str) -> Lease {
    Lease::new(
        LeaseId::new(lease_id).expect("lease id"),
        SlotId::new(slot_id).expect("slot id"),
        PrincipalRef::new("user:tester").expect("principal"),
        LeaseToken::new(token).expect("token"),
        1000,
        2000,
        Budget::new(1000, 3600).expect("budget"),
    )
    .expect("lease")
}

#[test]
fn launch_slot_spawn_requires_a_leased_slot() {
    let pool = SlotPool::new(PoolConfig {
        max_slots: 2,
        idle_suspend_duration: None,
        cgroup_base: None,
    });
    let id = SlotId::new("slot-spawn").expect("slot id");
    pool.create_slot(id.clone()).expect("create");
    let cfg = LaunchConfig::default();

    let cold = pool.spawn_worker(&id, sleeper(0), &cfg).expect_err("cold");
    assert!(
        matches!(cold, PoolError::InvalidSlotState(_, _, "spawn")),
        "{cold}"
    );

    pool.warm_slot(&id, WorktreeRef::new("wt-spawn").expect("wt"))
        .expect("warm");
    let warm = pool.spawn_worker(&id, sleeper(0), &cfg).expect_err("warm");
    assert!(
        matches!(warm, PoolError::InvalidSlotState(_, _, "spawn")),
        "{warm}"
    );

    pool.acquire_lease(make_lease("lease-spawn", "slot-spawn", "tok-spawn"))
        .expect("lease");
    let worker = pool
        .spawn_worker(&id, sleeper(0), &cfg)
        .expect("leased spawn");
    let (status, audit) = worker.finish().expect("finish");
    assert!(status.success());
    assert_eq!(audit.cpu_cap, CpuCap::Uncapped.to_string());
}
