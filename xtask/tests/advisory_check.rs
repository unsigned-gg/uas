// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Integration test for `cargo xtask advisory-check` (spec 0045 D2, REQ-0045-04).

use std::path::Path;
use std::process::Command;

fn run(fixture: &str, extra: &[&str]) -> (i32, String, String) {
    let cfg = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/advisory")
        .join(fixture);
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["advisory-check", "--today", "2026-10-10", "--config"])
        .arg(cfg)
        .args(extra)
        .output()
        .expect("xtask runs");
    (
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn advisory_binary_passes_valid_and_fails_expired() {
    let (code, out, err) = run("valid.toml", &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("advisory-check: ok (1 ignore entries)"),
        "{out}"
    );
    let (code, _, err) = run("expired.toml", &[]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("advisory-check: RUSTSEC-2021-0139: "), "{err}");
}

#[test]
fn advisory_binary_ids_flag_and_missing_config() {
    let (code, out, _) = run("expired.toml", &["--ids"]);
    assert_eq!(code, 0);
    assert_eq!(out, "RUSTSEC-2021-0139\n");
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["advisory-check", "--config", "/nonexistent/deny.toml"])
        .output()
        .expect("xtask runs");
    assert_eq!(out.status.code(), Some(1));
}
