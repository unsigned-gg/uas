//! Spec 0043 D1 and REQ-0043-01 gate for the launcher wiring:
//! `cargo test -p uas-sandbox -j 2 --test launcher_seccomp`.
//!
//! The worker is this test binary, re-executed through `launch` with
//! `launcher_seccomp_helper` as the only test. Inside the Landlock jail it forks a
//! probe that calls `ptrace(PTRACE_TRACEME)` and writes outside the workspace, then
//! prints `check=<name> result=<value>`. Forking the probe keeps an unfiltered
//! `PTRACE_TRACEME` from tracing the helper's parent. Expected errnos: EPERM 1, EACCES 13.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use uas_sandbox::{launch, LandlockRulesetBuilder};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const HELPER_ENV: &str = "UAS_LAUNCHER_SECCOMP_HELPER";
const WS_ENV: &str = "UAS_LAUNCHER_SECCOMP_WS";
const OUTSIDE_ENV: &str = "UAS_LAUNCHER_SECCOMP_OUTSIDE";

const EPERM: i32 = 1;
const EACCES: i32 = 13;

/// Run `ptrace(PTRACE_TRACEME)` in a forked child and return its errno (0 on success).
#[allow(unsafe_code)]
fn ptrace_traceme_errno() -> i32 {
    // SAFETY: the forked child makes one raw syscall and then `_exit`; it returns nothing
    // to the caller and touches no lock or allocator. The parent only waits.
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork failed");
    if pid == 0 {
        // SAFETY: PTRACE_TRACEME takes no pointer arguments.
        let ret = unsafe { libc::syscall(libc::SYS_ptrace, libc::PTRACE_TRACEME, 0, 0, 0) };
        let code = if ret == -1 {
            io::Error::last_os_error().raw_os_error().unwrap_or(0)
        } else {
            0
        };
        // SAFETY: `_exit` ends the forked child without destructors or atexit hooks.
        unsafe { libc::_exit(code) };
    }
    let mut status = 0;
    // SAFETY: `pid` is our child and `status` is a valid out pointer.
    let waited = unsafe { libc::waitpid(pid, &mut status, 0) };
    assert_eq!(waited, pid, "waitpid failed");
    assert!(libc::WIFEXITED(status), "probe did not exit: {status:#x}");
    libc::WEXITSTATUS(status)
}

fn describe<T>(result: io::Result<T>) -> String {
    match result {
        Ok(_) => "ok".to_string(),
        Err(err) => match err.raw_os_error() {
            Some(errno) => format!("errno{errno}"),
            None => format!("{:?}", err.kind()),
        },
    }
}

/// The re-executed worker. A no-op unless the parent set the helper variable.
#[test]
fn launcher_seccomp_helper() {
    if env::var_os(HELPER_ENV).is_none() {
        return;
    }
    let ws = PathBuf::from(env::var_os(WS_ENV).expect("workspace path"));
    let outside = PathBuf::from(env::var_os(OUTSIDE_ENV).expect("outside path"));
    println!(
        "check=write_ws result={}",
        describe(fs::write(ws.join("ok.txt"), b"inside"))
    );
    println!(
        "check=write_outside result={}",
        describe(fs::write(outside.join("escape.txt"), b"x"))
    );
    match ptrace_traceme_errno() {
        0 => println!("check=ptrace_traceme result=ok"),
        errno => println!("check=ptrace_traceme result=errno{errno}"),
    }
}

fn fresh(dir: PathBuf) -> io::Result<PathBuf> {
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn target_dir() -> io::Result<PathBuf> {
    env::current_exe()?
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("test binary is not under target/"))
}

fn errno_action_available() -> bool {
    fs::read_to_string("/proc/sys/kernel/seccomp/actions_avail")
        .map(|s| s.split_whitespace().any(|a| a == "errno"))
        .unwrap_or(false)
}

/// Launch the helper through `launch` and return its `check=` lines as `name=value`.
/// The outside directory is under `$HOME`, which no worker preset rule reaches; the
/// target directory may sit under `/tmp`, which the preset makes writable.
/// Returns `None` when `HOME` is unset.
fn run_helper(tag: &str) -> Result<Option<Vec<(String, String)>>, Box<dyn std::error::Error>> {
    let Some(home) = env::var_os("HOME") else {
        println!("skipped: HOME is unset");
        return Ok(None);
    };
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{tag}-{}", std::process::id()));
    let ws = fresh(base.join("ws"))?;
    let outside = fresh(
        Path::new(&home)
            .join(".cache")
            .join("uas-sandbox-tests")
            .join(format!("{tag}-{}", std::process::id())),
    )?;
    let (ruleset, _) = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new())
        .allow_read_exec(target_dir()?)
        .build()?;
    let mut command = Command::new(env::current_exe()?);
    command
        .args(["launcher_seccomp_helper", "--exact", "--nocapture"])
        .env(HELPER_ENV, "1")
        .env(WS_ENV, &ws)
        .env(OUTSIDE_ENV, &outside)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = launch(ruleset, &mut command)?;
    // Bounded wait: an unfiltered probe must fail the test, not hang it.
    let deadline = Instant::now() + Duration::from_secs(20);
    while child.try_wait()?.is_none() {
        if Instant::now() > deadline {
            child.kill()?;
            child.wait()?;
            return Err("helper hung (filter not installed?)".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "worker failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let _ = fs::remove_dir_all(&base);
    let _ = fs::remove_dir_all(&outside);
    Ok(Some(
        stdout
            .lines()
            .filter_map(|line| {
                let (name, value) = line.strip_prefix("check=")?.split_once(" result=")?;
                Some((name.to_string(), value.trim().to_string()))
            })
            .collect(),
    ))
}

fn get<'a>(results: &'a [(String, String)], name: &str) -> &'a str {
    results
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or("missing")
}

fn landlock_available() -> Result<bool, Box<dyn std::error::Error>> {
    let report = uas_sandbox::probe()?;
    if !report.is_available() {
        println!("skipped: landlock not available ({:?})", report.status);
    }
    Ok(report.is_available())
}

#[test]
fn launched_worker_gets_eperm_from_ptrace_traceme() -> TestResult {
    if !landlock_available()? {
        return Ok(());
    }
    if !errno_action_available() {
        println!("skipped: seccomp errno action not available");
        return Ok(());
    }
    let Some(results) = run_helper("launcher-seccomp-ptrace")? else {
        return Ok(());
    };
    assert_eq!(get(&results, "write_ws"), "ok", "positive control");
    assert_eq!(
        get(&results, "ptrace_traceme"),
        format!("errno{EPERM}"),
        "launch must install the seccomp filter"
    );
    Ok(())
}

#[test]
fn launched_worker_still_gets_eacces_outside_the_workspace() -> TestResult {
    if !landlock_available()? {
        return Ok(());
    }
    let Some(results) = run_helper("launcher-seccomp-landlock")? else {
        return Ok(());
    };
    assert_eq!(get(&results, "write_ws"), "ok", "positive control");
    assert_eq!(
        get(&results, "write_outside"),
        format!("errno{EACCES}"),
        "the filter must not displace Landlock"
    );
    Ok(())
}
