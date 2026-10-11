// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Binary conventions of `uas-ebpfd` (spec 0042 D2, D3, D5).
//!
//! The tests assume nothing about the host: the daemon exits 2 where it cannot
//! load the program, and each test bounds the run to one second.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_uas-ebpfd");

/// Run the daemon for at most one second. Return (stdout, stderr).
fn run_bounded(rust_log: Option<&str>, log_format: Option<&str>) -> (String, String) {
    let dir = std::env::temp_dir().join(format!(
        "uas-ebpfd-conv-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("tmpdir");
    let mut cmd = Command::new(BIN);
    cmd.arg("--socket")
        .arg(dir.join("x.sock"))
        .env_remove("RUST_LOG")
        .env_remove("UAS_LOG_FORMAT")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(v) = rust_log {
        cmd.env("RUST_LOG", v);
    }
    if let Some(v) = log_format {
        cmd.env("UAS_LOG_FORMAT", v);
    }
    let mut child = cmd.spawn().expect("spawn uas-ebpfd");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(1) {
        if child.try_wait().expect("try_wait").is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    let mut out = String::new();
    let mut err = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    (out, err)
}

#[test]
fn conventions_ebpfd_version_matches() {
    let out = Command::new(BIN).arg("--version").output().expect("run");
    assert!(out.status.success(), "--version failed: {out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let line = text.trim_end();
    let rest = line
        .strip_prefix("uas-ebpfd ")
        .unwrap_or_else(|| panic!("bad prefix: {line:?}"));
    let (ver, rev) = rest
        .split_once(" (")
        .unwrap_or_else(|| panic!("no rev: {line:?}"));
    let rev = rev
        .strip_suffix(')')
        .unwrap_or_else(|| panic!("no close: {line:?}"));
    let parts: Vec<&str> = ver.split('.').collect();
    assert!(
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "bad version: {line:?}"
    );
    let hex_rev = |s: &str| {
        let s = s.strip_suffix("-dirty").unwrap_or(s);
        s.len() >= 7 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    };
    assert!(
        rev == "unknown" || hex_rev(rev) || (rev.starts_with('v') && !rev.contains(' ')),
        "bad revision: {line:?}"
    );
}

#[test]
fn conventions_ebpfd_stdout_is_empty() {
    let (out, _) = run_bounded(Some("info"), None);
    assert!(out.is_empty(), "stdout must be empty, got: {out:?}");
}

#[test]
fn conventions_ebpfd_default_level_is_info() {
    let (_, err) = run_bounded(None, None);
    assert!(err.contains("INFO"), "no INFO line on stderr: {err:?}");
}

#[test]
fn conventions_ebpfd_json_log() {
    let (_, err) = run_bounded(None, Some("json"));
    let first = err.lines().next().unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(first)
        .unwrap_or_else(|e| panic!("first stderr line is not JSON ({e}): {first:?}"));
}
