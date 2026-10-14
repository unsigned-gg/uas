// SPDX-License-Identifier: Apache-2.0 OR MIT
//! The repo root comes from the working directory, not from a path compiled into the binary
//! (spec 0004 D3, CER-2800). Two clones that share one `CARGO_TARGET_DIR` must each check
//! their own files.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SPEC: &str = "specs/0004-dependency-and-licence-policy.md";

/// A temp repo whose single pin row says `pinned` and whose lock holds tokio 1.53.1.
fn temp_repo(name: &str, pinned: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-cwd-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let ebpf = dir.join("crates/uas-ebpf/ebpf");
    fs::create_dir_all(dir.join("xtask")).unwrap();
    fs::create_dir_all(dir.join("specs")).unwrap();
    fs::create_dir_all(dir.join(".github/workflows")).unwrap();
    fs::create_dir_all(&ebpf).unwrap();
    fs::write(
        dir.join(SPEC),
        format!(
            "| Crate | Version |\n|---|---|\n| `tokio` | {pinned} |\n\n| Tool | Version |\n|---|---|\n| `gitleaks` | 8.30.1 |\n"
        ),
    )
    .unwrap();
    fs::write(
        dir.join(".github/workflows/ci.yml"),
        "env:\n  GITLEAKS_VERSION: 8.30.1\n",
    )
    .unwrap();
    fs::write(dir.join("lefthook.yml"), "install v8.30.1\n").unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        "[workspace]\nmembers = [\"xtask\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("xtask/Cargo.toml"),
        "[dependencies]\ntokio = \"1\"\n",
    )
    .unwrap();
    fs::write(
        dir.join("Cargo.lock"),
        "[[package]]\nname = \"tokio\"\nversion = \"1.53.1\"\n",
    )
    .unwrap();
    fs::write(ebpf.join("Cargo.toml"), "[workspace]\n\n[dependencies]\n").unwrap();
    fs::write(ebpf.join("Cargo.lock"), "").unwrap();
    dir
}

fn pin_check_in(dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("pin-check")
        .current_dir(dir)
        .output()
        .unwrap()
}

#[test]
fn pin_check_reads_the_repo_it_runs_in() {
    // A row that disagrees with the lock fails, and the message names that repo's row.
    let bad = temp_repo("bad", "1.53.9");
    let out = pin_check_in(&bad);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("tokio pinned 1.53.9") && stderr.contains("Cargo.lock has 1.53.1"),
        "{stderr}"
    );
    // The same binary, run in a repo that agrees with its lock, passes.
    let good = temp_repo("good", "1.53.1");
    let out = pin_check_in(&good);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("ok (1 pinned crates checked)"), "{stdout}");
    // A subdirectory of the repo finds the same root.
    let out = pin_check_in(&bad.join("xtask"));
    assert_eq!(out.status.code(), Some(1));
    fs::remove_dir_all(bad).unwrap();
    fs::remove_dir_all(good).unwrap();
}

#[test]
fn xtask_outside_a_repo_exits_2() {
    let dir = std::env::temp_dir().join(format!("xtask-cwd-{}-none", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let out = pin_check_in(&dir);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("run from inside the repo"));
    fs::remove_dir_all(dir).unwrap();
}
