//! The process-group clamp, measured on a real child (spec 0014 D4, ruling R2).
//!
//! Lowering priority needs no privilege, so this gate runs unprivileged in CI.

use std::{
    os::unix::process::CommandExt,
    process::{Command, Stdio},
};

use cas_ebpf::clamp::{enter_clamped_group, nice_of, pgid_of, WORKER_NICE};

#[test]
fn child_spawned_with_enter_clamped_group_runs_at_nice_19_in_its_own_group() {
    let mut command = Command::new("sleep");
    command
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: the hook runs in the child between fork and exec. It makes two
    // syscalls (setpgid, setpriority) that touch only the child's own process
    // and allocate nothing, which is what pre_exec requires.
    #[allow(unsafe_code)]
    unsafe {
        command.pre_exec(enter_clamped_group);
    }
    let mut child = command.spawn().expect("spawn sleep");
    let pid = child.id();

    let nice = nice_of(pid).expect("getpriority on child");
    let pgid = pgid_of(pid).expect("getpgid on child");
    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(nice, WORKER_NICE, "child nice");
    assert_eq!(pgid, pid, "child leads its own process group");
}

#[test]
fn child_spawned_without_the_hook_keeps_the_parent_nice_and_group() {
    let mut child = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();
    let me = std::process::id();

    let child_nice = nice_of(pid).expect("getpriority on child");
    let child_pgid = pgid_of(pid).expect("getpgid on child");
    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(child_nice, nice_of(me).expect("own nice"));
    assert_eq!(child_pgid, pgid_of(me).expect("own pgid"));
}

// ---------------------------------------------------------------------------
// Multi-thread proof of the measured claim (spec 0014 T8): PRIO_PROCESS clamps
// one thread, PRIO_PGRP clamps every thread. The worker is this test binary,
// re-executed with `helper_threads` as the only test, in helper mode.
// ---------------------------------------------------------------------------

const HELPER_ENV: &str = "CAS_EBPF_CLAMP_HELPER";
const HELPER_THREADS: usize = 3;

/// The helper body. Outside helper mode it does nothing and passes.
#[test]
fn helper_threads() {
    if std::env::var_os(HELPER_ENV).is_none() {
        return;
    }
    let park = || std::thread::sleep(std::time::Duration::from_secs(30));
    let threads: Vec<_> = (0..HELPER_THREADS)
        .map(|_| std::thread::spawn(park))
        .collect();
    park();
    for thread in threads {
        let _ = thread.join();
    }
}

/// A command that runs this binary in helper mode: a process with at least
/// `HELPER_THREADS` extra threads parked for 30 seconds.
fn helper_command() -> Command {
    let mut command = Command::new(std::env::current_exe().expect("test binary path"));
    command
        .args(["helper_threads", "--exact", "--test-threads=1"])
        .env(HELPER_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// The nice value of this test process. Helpers inherit it, so it is the value
/// the control must see before any clamp. A test that already runs at
/// `WORKER_NICE` cannot show a clamp, so it fails instead of passing.
fn base_nice() -> i32 {
    let base = nice_of(std::process::id()).expect("own nice");
    assert!(
        base < WORKER_NICE,
        "cannot prove the clamp: the test runs at nice {base}"
    );
    base
}

/// Nice value of every task of `pid`, keyed by tid, once at least
/// `HELPER_THREADS + 1` tasks exist.
fn thread_nices(pid: u32) -> std::collections::BTreeMap<u32, i32> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let mut out = std::collections::BTreeMap::new();
        if let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) {
            for task in tasks.flatten() {
                let tid: u32 = match task.file_name().to_string_lossy().parse() {
                    Ok(tid) => tid,
                    Err(_) => continue,
                };
                let stat = match std::fs::read_to_string(task.path().join("stat")) {
                    Ok(stat) => stat,
                    Err(_) => continue,
                };
                // Fields after the ")" that closes comm: state is index 0, nice is index 16.
                let rest = stat.rsplit_once(')').map(|(_, rest)| rest).unwrap_or("");
                if let Some(nice) = rest.split_whitespace().nth(16).and_then(|n| n.parse().ok()) {
                    out.insert(tid, nice);
                }
            }
        }
        if out.len() > HELPER_THREADS {
            return out;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "helper never reached {} tasks: {out:?}",
            HELPER_THREADS + 1
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn enter_clamped_group_clamps_every_thread_of_the_child() {
    base_nice();
    let mut command = helper_command();
    // SAFETY: as in the first test: two syscalls on the child itself, no allocation.
    #[allow(unsafe_code)]
    unsafe {
        command.pre_exec(enter_clamped_group);
    }
    let mut child = command.spawn().expect("spawn helper");
    let nices = thread_nices(child.id());
    let _ = child.kill();
    let _ = child.wait();
    assert!(nices.len() > HELPER_THREADS, "{nices:?}");
    assert!(
        nices.values().all(|nice| *nice == WORKER_NICE),
        "every thread at nice 19: {nices:?}"
    );
}

#[test]
fn clamp_process_clamps_only_the_main_thread_and_clamp_process_group_clamps_all() {
    let base = base_nice();
    let mut command = helper_command();
    // SAFETY: one syscall on the child itself (its own process group), no allocation.
    // The helper must lead its own group, or the group clamp below would hit this test.
    #[allow(unsafe_code)]
    unsafe {
        command.pre_exec(|| rustix::process::setpgid(None, None).map_err(std::io::Error::from));
    }
    let mut child = command.spawn().expect("spawn helper");
    let pid = child.id();
    let before = thread_nices(pid);
    assert!(
        before.values().all(|nice| *nice == base),
        "control: {before:?}"
    );

    cas_ebpf::clamp::clamp_process(pid).expect("setpriority PRIO_PROCESS on child");
    let after_process = thread_nices(pid);
    assert_eq!(
        after_process.get(&pid),
        Some(&WORKER_NICE),
        "{after_process:?}"
    );
    let others: Vec<i32> = after_process
        .iter()
        .filter(|(tid, _)| **tid != pid)
        .map(|(_, nice)| *nice)
        .collect();
    assert!(
        others.iter().all(|nice| *nice == base),
        "PRIO_PROCESS must leave the other threads alone: {after_process:?}"
    );

    cas_ebpf::clamp::clamp_process_group(pgid_of(pid).expect("pgid")).expect("PRIO_PGRP");
    let after_group = thread_nices(pid);
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        after_group.values().all(|nice| *nice == WORKER_NICE),
        "PRIO_PGRP must clamp every thread: {after_group:?}"
    );
}
