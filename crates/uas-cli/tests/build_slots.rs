// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Spec 0059: `uas build` against a temporary slot directory and real subprocesses of the
//! built `uas` binary. Every control fails loudly; none skips.

use std::io::{BufRead, BufReader};
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

/// Start a build that holds the only slot for `secs` seconds. Returns once it reports the slot.
fn hold_slot(dir: &Path, secs: u32) -> Child {
    let sleep = format!("sleep {secs}");
    let mut c = uas_build(dir, 1, &[], &["sh", "-c", &sleep]);
    let mut child = c
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn holder");
    let mut line = String::new();
    BufReader::new(child.stderr.as_mut().expect("stderr"))
        .read_line(&mut line)
        .expect("holder line");
    assert!(line.contains("holding host slot"), "holder said: {line}");
    child
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn command_does_not_start_until_a_slot_is_held() {
    // REQ-0059-01: the marker file must not appear while the only slot is taken.
    let t = Tmp::new("start");
    let marker = t.0.join("started");
    let mut holder = hold_slot(&t.slots(), 3);
    let m = marker.to_str().expect("utf8");
    let waiter = uas_build(&t.slots(), 1, &["--wait", "30"], &["touch", m])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn waiter");
    std::thread::sleep(Duration::from_millis(1000));
    assert!(!marker.exists(), "command started before a slot was held");
    let out = waiter.wait_with_output().expect("wait");
    assert!(out.status.success());
    assert!(marker.exists(), "command never ran after the slot freed");
    let _ = holder.wait();
}

#[test]
fn second_build_waits_then_gets_the_slot() {
    // REQ-0059-02: with one slot, the second build waits for the first to exit.
    let t = Tmp::new("wait");
    let mut holder = hold_slot(&t.slots(), 2);
    let start = Instant::now();
    let out = run(uas_build(&t.slots(), 1, &["--wait", "30"], &["true"]));
    assert!(
        out.status.success(),
        "second build failed: {}",
        stderr(&out)
    );
    assert!(
        start.elapsed() >= Duration::from_millis(1000),
        "second build did not wait for the first"
    );
    assert!(holder.wait().expect("holder").success());
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
    let mut holder = hold_slot(&t.slots(), 5);
    let m = marker.to_str().expect("utf8");
    let start = Instant::now();
    let out = run(uas_build(&t.slots(), 1, &["--wait", "1"], &["touch", m]));
    assert_eq!(out.status.code(), Some(75), "{}", stderr(&out));
    assert!(start.elapsed() >= Duration::from_millis(900));
    assert!(!marker.exists(), "command ran without a slot");
    let _ = holder.kill();
    let _ = holder.wait();
}

#[test]
fn try_on_a_full_host_exits_75_at_once() {
    // REQ-0059-05.
    let t = Tmp::new("try");
    let marker = t.0.join("started");
    let mut holder = hold_slot(&t.slots(), 5);
    let m = marker.to_str().expect("utf8");
    let start = Instant::now();
    let out = run(uas_build(&t.slots(), 1, &["--try"], &["touch", m]));
    assert_eq!(out.status.code(), Some(75), "{}", stderr(&out));
    assert!(start.elapsed() < Duration::from_millis(900), "--try waited");
    assert!(!marker.exists());
    let _ = holder.kill();
    let _ = holder.wait();
}

#[test]
fn cargo_build_jobs_defaults_to_2_and_keeps_the_caller_value() {
    // REQ-0059-06.
    let t = Tmp::new("jobs");
    let probe = ["sh", "-c", "echo jobs=$CARGO_BUILD_JOBS"];
    let out = run(uas_build(&t.slots(), 1, &[], &probe));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "jobs=2");
    let mut c = uas_build(&t.slots(), 1, &[], &probe);
    c.env("CARGO_BUILD_JOBS", "5");
    let out = run(c);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "jobs=5");
}

#[test]
fn stderr_names_the_slot_index() {
    // REQ-0059-07: with slot 0 held, the second build names slot 1.
    let t = Tmp::new("index");
    let slots = t.slots();
    let mut c = uas_build(&slots, 2, &[], &["sleep", "3"]);
    let mut holder = c.stderr(Stdio::piped()).spawn().expect("spawn");
    let mut first = String::new();
    BufReader::new(holder.stderr.as_mut().expect("stderr"))
        .read_line(&mut first)
        .expect("line");
    assert!(first.contains("host slot 0 "), "first said: {first}");
    let out = run(uas_build(&slots, 2, &[], &["true"]));
    let err = stderr(&out);
    assert!(err.contains("host slot 1 "), "second said: {err}");
    assert!(err.contains("waited"), "no wait time in: {err}");
    let _ = holder.kill();
    let _ = holder.wait();
}

#[test]
fn build_module_has_no_limit_writes() {
    // REQ-0059-08: a source scan of the build module.
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/build.rs"))
        .expect("read build.rs");
    assert!(src.len() > 500, "scan read the wrong file");
    for word in [
        "cgroup",
        "/sys/fs",
        "memory.max",
        "memory.high",
        "setrlimit",
        "rlimit",
        "fs::write",
    ] {
        assert!(!src.contains(word), "build.rs mentions {word}");
    }
}
