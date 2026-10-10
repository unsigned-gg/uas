//! Binary conventions for `uas` (spec 0042 D1 to D4): version line, exit code, log format,
//! the wave env flags and the stdout/stderr split.

use std::process::{Command, Output};

fn uas(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_uas"));
    cmd.args(args)
        .env_remove("RUST_LOG")
        .env_remove("UAS_LOG_FORMAT");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("run uas")
}

/// `^uas [0-9]+\.[0-9]+\.[0-9]+ \((unknown|[0-9a-f]{7,}(-dirty)?|v[^ ]+)\)$`, without a regex crate.
fn version_line_ok(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("uas ") else {
        return false;
    };
    let Some((ver, tail)) = rest.split_once(' ') else {
        return false;
    };
    let parts: Vec<&str> = ver.split('.').collect();
    let semver = parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    let Some(inner) = tail.strip_prefix('(').and_then(|t| t.strip_suffix(')')) else {
        return false;
    };
    let hex = inner.strip_suffix("-dirty").unwrap_or(inner);
    let build = inner == "unknown"
        || (hex.len() >= 7 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        || (inner.starts_with('v') && !inner.contains(' '));
    semver && build
}

#[test]
fn conventions_version_matches() {
    let out = uas(&["--version"], &[]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        version_line_ok(text.trim_end_matches('\n')),
        "bad version line: {text:?}"
    );
}

#[test]
fn conventions_version_checker_rejects_junk() {
    assert!(!version_line_ok("uas 0.1 (unknown)"));
    assert!(!version_line_ok("uas 0.1.0 (abc)"));
    assert!(version_line_ok("uas 0.1.0 (abc1234-dirty)"));
}

#[test]
fn conventions_unknown_subcommand_exits_2() {
    let out = uas(&["nonsense"], &[]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn conventions_help_lists_wave_env() {
    let out = uas(&["wave", "step", "--help"], &[]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("[env: UAS_WAVE_REPO"), "{text}");
    assert!(text.contains("[env: UAS_WAVE_STATE_DIR"), "{text}");
}

#[test]
fn conventions_json_log() {
    let out = uas(
        &["demo", "--slot", "slot-a"],
        &[("UAS_LOG_FORMAT", "json"), ("RUST_LOG", "debug")],
    );
    let err = String::from_utf8(out.stderr).unwrap();
    let first = err.lines().next().expect("stderr has a line");
    serde_json::from_str::<serde_json::Value>(first)
        .unwrap_or_else(|e| panic!("first stderr line is not JSON ({e}): {first}"));
}

#[test]
fn conventions_stdout_is_result_only() {
    let out = uas(&["demo", "--slot", "slot-a"], &[("RUST_LOG", "debug")]);
    serde_json::from_slice::<serde_json::Value>(&out.stdout).expect("stdout is one JSON value");
    assert!(!out.stderr.is_empty(), "debug logs go to stderr");
}
