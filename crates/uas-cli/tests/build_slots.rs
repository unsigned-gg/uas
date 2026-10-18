// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Spec 0059: `uas build` against a temporary slot directory and real subprocesses of the
//! built `uas` binary. Every control fails loudly; none skips.

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

struct Tmp(PathBuf);

impl Tmp {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "uas-cli-build-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).expect("tmp");
        Self(p)
    }

    /// The slot directory. It does not exist yet, so the arbiter creates it with mode 0700.
    fn slots(&self) -> PathBuf {
        self.0.join("slots")
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `uas build --slot-dir <dir> --host-slots <n> <flags> -- <argv>` with `CARGO_BUILD_JOBS` unset.
fn uas_build(dir: &Path, slots: usize, flags: &[&str], argv: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_uas"));
    c.env_remove("CARGO_BUILD_JOBS")
        .env_remove("UAS_SLOT_DIR")
        .arg("build")
        .arg("--slot-dir")
        .arg(dir)
        .args(["--host-slots", &slots.to_string()])
        .args(flags)
        .arg("--")
        .args(argv);
    c
}

fn run(mut c: Command) -> Output {
    c.stdin(Stdio::null()).output().expect("run uas")
}

/// A `uas build` in its own process group. Dropping it kills the whole group, so no child
/// outlives the test.
struct Holder {
    child: Child,
}

impl Holder {
    fn pgid(&self) -> String {
        self.child.id().to_string()
    }

    /// Whether any process is still in the group.
    fn group_alive(&self) -> bool {
        Command::new("kill")
            .args(["-0", "--", &format!("-{}", self.pgid())])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Read the first stderr line, which names the slot held.
    fn first_line(&mut self) -> String {
        let mut line = String::new();
        BufReader::new(self.child.stderr.as_mut().expect("stderr"))
            .read_line(&mut line)
            .expect("holder line");
        line
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", self.pgid())])
            .stderr(Stdio::null())
            .status();
        let _ = self.child.wait();
    }
}

/// Start `uas build` in its own process group with piped stderr.
fn spawn_group(mut c: Command) -> Holder {
    let child = c
        .process_group(0)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn holder");
    Holder { child }
}

/// Start `uas build` running the shell `script`. Returns once it reports the slot it holds.
fn hold(dir: &Path, slots: usize, script: &str) -> Holder {
    let mut holder = spawn_group(uas_build(dir, slots, &[], &["sh", "-c", script]));
    let line = holder.first_line();
    assert!(line.contains("holding host slot"), "holder said: {line}");
    holder
}

/// Hold the only slot until the test ends and the group is killed.
fn hold_long(dir: &Path) -> Holder {
    hold(dir, 1, "sleep 60")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn cannot_run_lines(o: &Output) -> usize {
    stderr(o)
        .lines()
        .filter(|l| l.contains("cannot run"))
        .count()
}

#[test]
fn command_does_not_start_until_a_slot_is_held() {
    // REQ-0059-01: the marker must not appear while the only slot is taken.
    let t = Tmp::new("start");
    let marker = t.0.join("started");
    let release = t.0.join("release");
    let script = format!("while [ ! -f {} ]; do sleep 0.1; done", release.display());
    let _holder = hold(&t.slots(), 1, &script);
    let m = marker.to_str().expect("utf8");
    let waiter = uas_build(&t.slots(), 1, &["--wait", "120"], &["touch", m])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn waiter");
    // A negative window: a late waiter still passes, so load cannot make this red.
    std::thread::sleep(Duration::from_millis(1000));
    assert!(!marker.exists(), "command started before a slot was held");
    std::fs::write(&release, "").expect("release");
    let out = waiter.wait_with_output().expect("wait");
    assert!(out.status.success());
    assert!(marker.exists(), "command never ran after the slot freed");
}

#[test]
fn second_build_waits_then_gets_the_slot() {
    // REQ-0059-02: the second command runs only after the first has finished.
    let t = Tmp::new("wait");
    let done = t.0.join("done");
    let script = format!("sleep 2; touch {}", done.display());
    let mut holder = hold(&t.slots(), 1, &script);
    let check = format!("test -f {}", done.display());
    let out = run(uas_build(
        &t.slots(),
        1,
        &["--wait", "120"],
        &["sh", "-c", &check],
    ));
    assert_eq!(
        out.status.code(),
        Some(0),
        "second build ran before the first finished: {}",
        stderr(&out)
    );
    assert!(holder.child.wait().expect("holder").success());
    // The slot is free again: a try now succeeds.
    let out = run(uas_build(&t.slots(), 1, &["--try"], &["true"]));
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

#[test]
fn exit_status_is_that_of_the_child() {
    // REQ-0059-03.
    let t = Tmp::new("exit");
    let out = run(uas_build(&t.slots(), 1, &[], &["sh", "-c", "exit 3"]));
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    let out = run(uas_build(
        &t.slots(),
        1,
        &[],
        &["sh", "-c", "kill -TERM $$"],
    ));
    assert_eq!(out.status.code(), Some(143), "{}", stderr(&out));
}

#[test]
fn full_host_with_short_wait_exits_75_and_never_runs() {
    // REQ-0059-04.
    let t = Tmp::new("full");
    let marker = t.0.join("started");
    let _holder = hold_long(&t.slots());
    let m = marker.to_str().expect("utf8");
    let out = run(uas_build(&t.slots(), 1, &["--wait", "1"], &["touch", m]));
    assert_eq!(out.status.code(), Some(75), "{}", stderr(&out));
    assert!(!marker.exists(), "command ran without a slot");
}

#[test]
fn wait_zero_on_a_full_host_exits_75() {
    let t = Tmp::new("wait0");
    let marker = t.0.join("started");
    let _holder = hold_long(&t.slots());
    let m = marker.to_str().expect("utf8");
    let out = run(uas_build(&t.slots(), 1, &["--wait", "0"], &["touch", m]));
    assert_eq!(out.status.code(), Some(75), "{}", stderr(&out));
    assert!(!marker.exists());
}

#[test]
fn try_on_a_full_host_exits_75_without_waiting() {
    // REQ-0059-05. The holder lasts 60 s; a --try that waited would come back after that.
    let t = Tmp::new("try");
    let marker = t.0.join("started");
    let _holder = hold_long(&t.slots());
    let m = marker.to_str().expect("utf8");
    let start = Instant::now();
    let out = run(uas_build(&t.slots(), 1, &["--try"], &["touch", m]));
    assert_eq!(out.status.code(), Some(75), "{}", stderr(&out));
    assert!(start.elapsed() < Duration::from_secs(20), "--try waited");
    assert!(!marker.exists());
}

#[test]
fn try_with_wait_is_refused_by_clap() {
    let t = Tmp::new("conflict");
    let out = run(uas_build(
        &t.slots(),
        1,
        &["--try", "--wait", "5"],
        &["true"],
    ));
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("cannot be used with"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn zero_host_slots_is_a_clear_error() {
    let t = Tmp::new("zero");
    let out = run(uas_build(&t.slots(), 0, &[], &["true"]));
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("invalid --host-slots"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn cargo_build_jobs_defaults_to_2_and_keeps_the_caller_value() {
    // REQ-0059-06: unset and empty become 2; 5 stays 5.
    let t = Tmp::new("jobs");
    let probe = ["sh", "-c", "echo jobs=$CARGO_BUILD_JOBS"];
    let out = run(uas_build(&t.slots(), 1, &[], &probe));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "jobs=2");
    for (given, want) in [("5", "jobs=5"), ("", "jobs=2")] {
        let mut c = uas_build(&t.slots(), 1, &[], &probe);
        c.env("CARGO_BUILD_JOBS", given);
        let out = run(c);
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), want);
    }
}

#[test]
fn stderr_names_the_slot_index() {
    // REQ-0059-07: with slot 0 held, the second build names slot 1.
    let t = Tmp::new("index");
    let slots = t.slots();
    let mut holder = spawn_group(uas_build(&slots, 2, &[], &["sleep", "60"]));
    let first = holder.first_line();
    assert!(first.contains("host slot 0 "), "first said: {first}");
    let out = run(uas_build(&slots, 2, &[], &["true"]));
    let err = stderr(&out);
    assert!(err.contains("host slot 1 "), "second said: {err}");
    assert!(err.contains("waited"), "no wait time in: {err}");
}

#[test]
fn missing_command_exits_127_and_unrunnable_file_exits_126() {
    // D5: a command that cannot start must not look like a child that exited 1.
    let t = Tmp::new("spawn");
    let out = run(uas_build(
        &t.slots(),
        1,
        &[],
        &["/nonexistent/uas-no-such-cmd"],
    ));
    assert_eq!(out.status.code(), Some(127), "{}", stderr(&out));
    assert_eq!(cannot_run_lines(&out), 1, "{}", stderr(&out));
    let plain = t.0.join("plain.txt");
    std::fs::write(&plain, "x").expect("write");
    std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    let out = run(uas_build(
        &t.slots(),
        1,
        &[],
        &[plain.to_str().expect("utf8")],
    ));
    assert_eq!(out.status.code(), Some(126), "{}", stderr(&out));
    assert_eq!(cannot_run_lines(&out), 1, "{}", stderr(&out));
}

#[test]
fn killing_uas_build_frees_the_slot_while_the_command_runs() {
    // D6 and spec 0036 REQ-04: the child never inherits the lock descriptor.
    let t = Tmp::new("leak");
    let started = t.0.join("started");
    let script = format!("touch {}; sleep 60", started.display());
    let mut holder = hold(&t.slots(), 1, &script);
    // The slot line prints before the command starts: wait until the command runs.
    let deadline = Instant::now() + Duration::from_secs(60);
    while !started.exists() {
        assert!(Instant::now() < deadline, "command never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    holder.child.kill().expect("SIGKILL uas build");
    holder.child.wait().expect("reap");
    assert!(holder.group_alive(), "the command should outlive uas build");
    let out = run(uas_build(&t.slots(), 1, &["--try"], &["true"]));
    assert_eq!(
        out.status.code(),
        Some(0),
        "slot still held after uas build died: {}",
        stderr(&out)
    );
}

/// Tokens that mean a write to a limit file or a resource limit. The build module holds none.
const BANNED: &[&str] = &[
    "File::create",
    "fs::write",
    "OpenOptions",
    "create_new(",
    ".write(",
    ".write_all(",
    "set_permissions",
    "prlimit",
    "setrlimit",
    "rlimit",
    "cgroup",
    "/sys/fs",
    "memory.max",
    "memory.high",
    "cpu.max",
    "pids.max",
];

fn violations(src: &str) -> Vec<&'static str> {
    BANNED.iter().copied().filter(|b| src.contains(b)).collect()
}

#[test]
fn build_module_has_no_limit_writes() {
    // REQ-0059-08: a source scan of the build module.
    let src = include_str!("../src/build.rs");
    assert!(src.len() > 500, "scan read the wrong file");
    assert_eq!(violations(src), Vec::<&str>::new());
}

#[test]
fn build_module_scan_goes_red_on_planted_forms() {
    for planted in [
        "File::create(p)",
        "std::fs::write(p, b\"max\")",
        "OpenOptions::new()",
        "f.write(b\"\")",
        "prlimit(pid)",
        "setrlimit(r)",
        "// cgroup",
        "\"memory.max\"",
        "\"cpu.max\"",
        "\"pids.max\"",
    ] {
        let src = format!("{}\n{planted}", include_str!("../src/build.rs"));
        assert!(!violations(&src).is_empty(), "scan missed {planted}");
    }
}
