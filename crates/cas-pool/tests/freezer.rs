//! CER-2722: cgroup v2 freezer and process suspension integration tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cas_domain::SlotId;
use cas_pool::freezer::{
    current_process_cgroup_path, CgroupFreezer, CpuCap, Freezer, FreezerState, ResourceLimits,
    SignalFreezer,
};

/// RAII temporary directory guard that cleans up on drop.
struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cas-pool-test-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&path).expect("failed to create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------
// Safety Invariants (Spec 0015 D3, CER-2722)
// ---------------------------------------------------------------------------

#[test]
fn safety_validation_rejects_root_and_top_level_cgroup() {
    let root = Path::new("/");
    let top_cg = Path::new("/sys/fs/cgroup");
    let child = Path::new("/sys/fs/cgroup/slot-test");

    // Base cannot be root
    assert!(CgroupFreezer::verify_safety(root, child).is_err());
    // Base cannot be top-level /sys/fs/cgroup
    assert!(CgroupFreezer::verify_safety(top_cg, child).is_err());
    // Child cannot be root or top-level /sys/fs/cgroup
    let dummy_base = Path::new("/sys/fs/cgroup/cas");
    assert!(CgroupFreezer::verify_safety(dummy_base, root).is_err());
    assert!(CgroupFreezer::verify_safety(dummy_base, top_cg).is_err());
}

#[test]
fn safety_validation_rejects_non_child_paths() {
    let base = Path::new("/sys/fs/cgroup/cas");
    let non_child = Path::new("/sys/fs/cgroup/other/slot-01");
    assert!(CgroupFreezer::verify_safety(base, non_child).is_err());

    // Identical path (not strict child) is rejected
    assert!(CgroupFreezer::verify_safety(base, base).is_err());
}

#[test]
fn safety_validation_rejects_own_process_cgroup() {
    if let Ok(own_cg) = current_process_cgroup_path() {
        if let Some(parent) = own_cg.parent() {
            // Attempting to freeze the process's own cgroup MUST be rejected
            assert!(
                CgroupFreezer::verify_safety(parent, &own_cg).is_err(),
                "must reject own process cgroup {:?}",
                own_cg
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Directory Provisioning and Resource Limits (Spec 0020, CER-2722)
// ---------------------------------------------------------------------------

#[test]
fn cgroup_freezer_creates_child_directory_and_writes_limits() {
    let temp = TempDirGuard::new("cgroup-limits");
    let base = temp.path().join("cas-base");
    fs::create_dir_all(&base).expect("create base");

    let slot_id = SlotId::new("slot-01").expect("valid slot id");
    let freezer = CgroupFreezer::new(&base, slot_id.clone()).expect("create cgroup freezer");

    assert_eq!(freezer.slot_id(), &slot_id);
    let slot_path = freezer.path();
    assert!(slot_path.exists());
    assert_eq!(slot_path, base.join("slot-slot-01"));

    // Pre-create dummy limit files as cgroup v2 kernel would provide
    let cpu_file = slot_path.join("cpu.max");
    let mem_high_file = slot_path.join("memory.high");
    let mem_max_file = slot_path.join("memory.max");

    fs::write(&cpu_file, b"max 100000\n").expect("write cpu.max");
    fs::write(&mem_high_file, b"max\n").expect("write mem high");
    fs::write(&mem_max_file, b"max\n").expect("write mem max");

    let limits = ResourceLimits {
        cpu_quota_us: Some(25_000),
        cpu_period_us: Some(100_000),
        memory_high_bytes: Some(128 * 1024 * 1024),
        memory_max_bytes: Some(256 * 1024 * 1024),
    };

    let cap = freezer.configure_limits(&limits).expect("configure limits");
    assert_eq!(cap, CpuCap::Capped("25000 100000".to_string()));
    assert_eq!(
        freezer.cpu_cap(),
        CpuCap::Capped("25000 100000".to_string())
    );

    assert_eq!(
        fs::read_to_string(&cpu_file).expect("read cpu.max"),
        "25000 100000\n"
    );
    assert_eq!(
        fs::read_to_string(&mem_high_file).expect("read mem high"),
        format!("{}\n", 128 * 1024 * 1024)
    );
    assert_eq!(
        fs::read_to_string(&mem_max_file).expect("read mem max"),
        format!("{}\n", 256 * 1024 * 1024)
    );

    // Destroy cleans up the slot cgroup
    freezer.destroy().expect("destroy");
    assert!(!slot_path.exists());
}

#[test]
fn cgroup_freezer_records_uncapped_and_warns_when_cpu_controller_absent() {
    let temp = TempDirGuard::new("cgroup-absent");
    let base = temp.path().join("cas-base");
    fs::create_dir_all(&base).expect("create base");

    let slot_id = SlotId::new("slot-no-cpu").expect("valid slot id");
    // Slot directory is created without cpu.max
    let freezer = CgroupFreezer::new(&base, slot_id).expect("create cgroup freezer");

    // Default configuration on creation records Uncapped because cpu.max was absent
    assert_eq!(freezer.cpu_cap(), CpuCap::Uncapped);

    let limits = ResourceLimits {
        cpu_quota_us: Some(200_000),
        cpu_period_us: Some(100_000),
        memory_high_bytes: None,
        memory_max_bytes: None,
    };

    let cap = freezer
        .configure_limits(&limits)
        .expect("configure limits must not fail");
    assert_eq!(cap, CpuCap::Uncapped);
    assert_eq!(freezer.cpu_cap(), CpuCap::Uncapped);

    freezer.destroy().expect("destroy");
}

#[test]
fn cgroup_freezer_defaults_to_two_cpus_quota() {
    let temp = TempDirGuard::new("cgroup-default-quota");
    let base = temp.path().join("cas-base");
    let slot_dir = base.join("slot-slot-two-cpu");
    fs::create_dir_all(&slot_dir).expect("create slot dir");

    // Pre-create cpu.max before CgroupFreezer::new
    let cpu_file = slot_dir.join("cpu.max");
    fs::write(&cpu_file, b"max 100000\n").expect("write initial cpu.max");

    let slot_id = SlotId::new("slot-two-cpu").expect("valid slot id");
    let freezer = CgroupFreezer::new(&base, slot_id).expect("create freezer");

    // Default initialization should have written 2 CPUs quota: 200_000 100_000
    assert_eq!(
        fs::read_to_string(&cpu_file).expect("read cpu.max"),
        "200000 100000\n"
    );
    assert_eq!(
        freezer.cpu_cap(),
        CpuCap::Capped("200000 100000".to_string())
    );

    freezer.destroy().expect("destroy");
}

#[test]
fn cgroup_freezer_state_parsing_and_pids() {
    let temp = TempDirGuard::new("cgroup-state");
    let base = temp.path().join("cas-base");
    fs::create_dir_all(&base).expect("create base");

    let slot_id = SlotId::new("slot-02").expect("valid slot id");
    let freezer = CgroupFreezer::new(&base, slot_id).expect("create freezer");
    let slot_path = freezer.path();

    let events_file = slot_path.join("cgroup.events");
    let procs_file = slot_path.join("cgroup.procs");

    // Initially thawed
    fs::write(&events_file, b"populated 1\nfrozen 0\n").expect("write events");
    assert_eq!(freezer.state().expect("state"), FreezerState::Thawed);

    // Transition to frozen
    fs::write(&events_file, b"populated 1\nfrozen 1\n").expect("write events");
    assert_eq!(freezer.state().expect("state"), FreezerState::Frozen);

    // Write PIDs into cgroup.procs
    fs::write(&procs_file, b"101\n202\n303\n").expect("write procs");
    let pids = freezer.pids().expect("pids");
    assert_eq!(pids, vec![101, 202, 303]);

    freezer.destroy().expect("destroy");
}

// ---------------------------------------------------------------------------
// POSIX Signal Freezer Fallback (Spec 0015 D4, CER-2722)
// ---------------------------------------------------------------------------

#[test]
fn signal_freezer_suspends_and_resumes_worker_process() {
    let slot_id = SlotId::new("slot-sig").expect("valid slot id");
    let freezer = SignalFreezer::new(slot_id);

    assert_eq!(
        freezer.state().expect("initial state"),
        FreezerState::Thawed
    );

    let mut child = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleep child");

    let pid = child.id();
    freezer.add_pid(pid).expect("add pid");
    assert_eq!(freezer.pids().expect("pids"), vec![pid]);

    // Freeze worker via SIGSTOP
    freezer.freeze().expect("freeze");
    assert_eq!(freezer.state().expect("state frozen"), FreezerState::Frozen);

    // Resume worker via SIGCONT
    freezer.unfreeze().expect("unfreeze");
    assert_eq!(freezer.state().expect("state thawed"), FreezerState::Thawed);

    // Clean up child process
    let _ = child.kill();
    let _ = child.wait();
}

// ---------------------------------------------------------------------------
// Live cgroup v2 hierarchy (WSL2 / Linux host user slice probe)
// ---------------------------------------------------------------------------

fn probe_writable_cgroup_base() -> Option<PathBuf> {
    let candidate =
        PathBuf::from("/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/app.slice");
    if candidate.exists() {
        let probe_dir = candidate.join(format!("cas-test-probe-{}", std::process::id()));
        if fs::create_dir(&probe_dir).is_ok() {
            let _ = fs::remove_dir(&probe_dir);
            return Some(candidate);
        }
    }
    None
}

#[test]
fn live_cgroup_v2_freeze_and_unfreeze_lifecycle() {
    let Some(base) = probe_writable_cgroup_base() else {
        eprintln!("skipped: cgroup v2 user slice not writable on this host");
        return;
    };

    let slot_id = SlotId::new("live-test-01").expect("valid slot id");
    let freezer = CgroupFreezer::new(&base, slot_id).expect("create live freezer");

    let mut child = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleep worker");

    let pid = child.id();
    if let Err(e) = freezer.add_pid(pid) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = freezer.destroy();
        panic!("failed to add pid {pid} to cgroup: {e}");
    }

    assert_eq!(
        freezer.state().expect("initial state"),
        FreezerState::Thawed
    );

    // Freeze slot
    freezer.freeze().expect("live freeze");
    assert_eq!(freezer.state().expect("state frozen"), FreezerState::Frozen);

    // Unfreeze slot
    freezer.unfreeze().expect("live unfreeze");
    assert_eq!(freezer.state().expect("state thawed"), FreezerState::Thawed);

    // Reap child and clean up
    let _ = child.kill();
    let _ = child.wait();
    freezer.destroy().expect("clean up live cgroup");
}
