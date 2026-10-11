//! Spec 0043 D2/D4 gate for `seccomp::install`: `cargo nextest run -p uas-sandbox seccomp -j 2`.
//!
//! The worker is this test binary, re-executed with `seccomp_helper` as the only
//! test and `UAS_SECCOMP_PROBE` naming a probe. The helper builds the programs,
//! calls `install()`, runs the probe and prints `result=errno<N>` or `result=ok`.
//! Expected errnos: EPERM 1, ENOSYS 38.

use std::env;
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};

use uas_sandbox::seccomp;

const PROBE_ENV: &str = "UAS_SECCOMP_PROBE";
const EPERM: i32 = 1;
const ENOSYS: i32 = 38;

/// Raw syscall result as `Ok(ret)` or `Err(errno)`.
#[allow(unsafe_code)]
fn raw(nr: libc::c_long, a0: libc::c_long, a1: libc::c_long) -> Result<i64, i32> {
    // SAFETY: the probes pass either small integers or a pointer to a zeroed struct that
    // outlives the call; the calls under test are refused by the filter before the kernel
    // acts on the arguments.
    let ret = unsafe { libc::syscall(nr, a0, a1, 0, 0, 0, 0) };
    if ret == -1 {
        Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(0))
    } else {
        Ok(ret)
    }
}

/// Run `f` in a forked child of the helper and return its result. An unfiltered
/// `PTRACE_TRACEME` makes the caller's parent the tracer; forking keeps that tracer
/// inside the helper, so the test process that spawned the helper is never traced.
#[allow(unsafe_code)]
fn in_forked_child(f: impl FnOnce() -> Result<i64, i32>) -> Result<i64, i32> {
    // SAFETY: the child runs `f`, which makes one raw syscall, then `_exit`; it returns
    // nothing to the caller and touches no lock or allocator. The parent only waits.
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork failed");
    if pid == 0 {
        let code = match f() {
            Ok(_) => 0,
            Err(errno) => errno,
        };
        // SAFETY: `_exit` ends the forked child without running destructors or atexit hooks.
        unsafe { libc::_exit(code) };
    }
    let mut status = 0;
    // SAFETY: `pid` is our child and `status` is a valid out pointer.
    let waited = unsafe { libc::waitpid(pid, &mut status, 0) };
    assert_eq!(waited, pid, "waitpid failed");
    assert!(
        libc::WIFEXITED(status),
        "probe child did not exit: {status:#x}"
    );
    match libc::WEXITSTATUS(status) {
        0 => Ok(0),
        errno => Err(errno),
    }
}

fn probe(name: &str) -> Result<i64, i32> {
    match name {
        "ptrace" => {
            in_forked_child(|| raw(libc::SYS_ptrace, libc::PTRACE_TRACEME as libc::c_long, 0))
        }
        // size 0 is invalid (EINVAL) on the unfiltered path, so the filter's ENOSYS is distinct.
        "clone3" => raw(libc::SYS_clone3, 0, 0),
        "getpid" => raw(libc::SYS_getpid, 0, 0),
        other => panic!("unknown probe {other}"),
    }
}

/// The re-executed worker. A no-op unless the parent set the probe variable.
#[test]
fn seccomp_helper() {
    let Ok(name) = env::var(PROBE_ENV) else {
        return;
    };
    let installed = seccomp::build()
        .map_err(|e| std::io::Error::other(e.to_string()))
        .and_then(|p| seccomp::install(&p));
    installed.expect("install");
    match probe(&name) {
        Ok(_) => println!("result=ok"),
        Err(errno) => println!("result=errno{errno}"),
    }
}

fn errno_action_available() -> bool {
    fs::read_to_string("/proc/sys/kernel/seccomp/actions_avail")
        .map(|s| s.split_whitespace().any(|a| a == "errno"))
        .unwrap_or(false)
}

/// Run one probe in a filtered child and return its `result=` value. An unfiltered
/// `PTRACE_TRACEME` makes this process the child's tracer and the child then stops, so
/// the wait is bounded and a hang is reported as a failure.
fn run(probe_name: &str) -> String {
    let log = env::temp_dir().join(format!(
        "uas-seccomp-{}-{probe_name}.log",
        std::process::id()
    ));
    let file = fs::File::create(&log).expect("create log");
    let mut child = Command::new(env::current_exe().expect("current exe"))
        .args([
            "seccomp_helper",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(PROBE_ENV, probe_name)
        .stdout(file.try_clone().expect("clone log"))
        .stderr(file)
        .spawn()
        .expect("spawn helper");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut timed_out = false;
    while child.try_wait().expect("try_wait").is_none() {
        if Instant::now() > deadline {
            timed_out = true;
            child.kill().expect("kill helper");
            child.wait().expect("reap helper");
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = fs::read_to_string(&log).unwrap_or_default();
    let _ = fs::remove_file(&log);
    assert!(
        !timed_out,
        "helper hung (filter not applied?); output: {output}"
    );
    output
        .lines()
        .find_map(|l| l.split_once("result=").map(|(_, v)| v.trim().to_owned()))
        .unwrap_or_else(|| panic!("no result line; output: {output}"))
}

fn skip_without_errno() -> bool {
    if errno_action_available() {
        return false;
    }
    println!("skipped: seccomp errno action not available");
    true
}

#[test]
fn seccomp_install_ptrace_traceme_is_eperm() {
    if skip_without_errno() {
        return;
    }
    assert_eq!(run("ptrace"), format!("errno{EPERM}"));
}

#[test]
fn seccomp_install_clone3_is_enosys() {
    if skip_without_errno() {
        return;
    }
    assert_eq!(run("clone3"), format!("errno{ENOSYS}"));
}

#[test]
fn seccomp_install_leaves_other_calls_alone() {
    if skip_without_errno() {
        return;
    }
    assert_eq!(run("getpid"), "ok");
}
