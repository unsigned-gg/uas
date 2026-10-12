// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Integration tests for `cargo xtask pin-check` (spec 0034 D1-D10, plan 0034 P3).

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(spec_content: &str, makefile_content: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("uas-pin-check-fixture-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("specs")).expect("create specs dir");
        fs::create_dir_all(dir.join("crates/uas-ebpf/ebpf")).expect("create ebpf dir");
        fs::create_dir_all(dir.join(".github/workflows")).expect("create workflows dir");

        fs::write(
            dir.join("specs/0004-dependency-and-licence-policy.md"),
            spec_content,
        )
        .expect("write spec");
        fs::write(dir.join("Makefile"), makefile_content).expect("write makefile");
        fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = []\n")
            .expect("write root Cargo.toml");
        fs::write(dir.join("Cargo.lock"), "version = 3\n").expect("write root Cargo.lock");
        fs::write(
            dir.join("crates/uas-ebpf/ebpf/Cargo.toml"),
            "[package]\nname = \"ebpf\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write ebpf Cargo.toml");
        fs::write(dir.join("crates/uas-ebpf/ebpf/Cargo.lock"), "version = 3\n")
            .expect("write ebpf Cargo.lock");
        fs::write(
            dir.join(".github/workflows/ci.yml"),
            "name: ci\non: push\njobs:\n  secrets:\n    env:\n      GITLEAKS_VERSION: 8.30.1\n",
        )
        .expect("write ci.yml");
        fs::write(
            dir.join("lefthook.yml"),
            "pre-commit:\n  commands:\n    gitleaks:\n      run: echo install v8.30.1\n",
        )
        .expect("write lefthook.yml");

        Self { dir }
    }

    fn run(&self) -> (i32, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("pin-check")
            .arg("--root")
            .arg(&self.dir)
            .output()
            .expect("xtask pin-check runs");
        (
            out.status.code().expect("exit code"),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

const BASE_SPEC: &str = r#"
# 0004-dependency-and-licence-policy

| Crate | Version |
|---|---|

### CI tools

| Tool | Version | Licence | Use | Pin |
|---|---|---|---|---|
| `gitleaks` | 8.30.1 | MIT | use | env GITLEAKS_VERSION |
"#;

#[test]
fn pin_check_makefile_unpinned_cargo_fake_fails_and_row_passes() {
    let makefile = "target:\n\tcargo fake\n";
    let fix_fail = Fixture::new(BASE_SPEC, makefile);
    let (code, _out, err) = fix_fail.run();
    assert_eq!(code, 1);
    assert!(
        err.contains("cargo-fake"),
        "expected stderr to contain cargo-fake, got:\n{err}"
    );

    let spec_with_row = format!("{BASE_SPEC}| `cargo-fake` | 1.2.3 | MIT | use | 1.2.3 |\n");
    let fix_pass = Fixture::new(&spec_with_row, makefile);
    let (code, out, err) = fix_pass.run();
    assert_eq!(code, 0, "control failed: out={out}, err={err}");
    assert!(out.contains("pin-check: ok (1 tool references checked)"));
}

#[test]
fn pin_check_makefile_row_with_version_latest_fails() {
    let makefile = "target:\n\tcargo fake\n";
    let spec = format!("{BASE_SPEC}| `cargo-fake` | latest | MIT | use | latest |\n");
    let fix = Fixture::new(&spec, makefile);
    let (code, _out, err) = fix.run();
    assert_eq!(code, 1);
    assert!(
        err.contains("cargo-fake"),
        "expected stderr to contain cargo-fake, got:\n{err}"
    );
}

#[test]
fn pin_check_makefile_cargo_install_rev() {
    let makefile_no_rev = "target:\n\tcargo install --git https://github.com/foo/pgt pgt\n";
    let fix_fail = Fixture::new(BASE_SPEC, makefile_no_rev);
    let (code, _out, err) = fix_fail.run();
    assert_eq!(code, 1);
    assert!(
        err.contains("cargo install"),
        "expected stderr to contain cargo install, got:\n{err}"
    );

    let makefile_rev =
        "target:\n\tcargo install --git https://github.com/foo/pgt --rev 54d90f5 pgt\n";
    let fix_pass = Fixture::new(BASE_SPEC, makefile_rev);
    let (code, out, err) = fix_pass.run();
    assert_eq!(code, 0, "control failed: out={out}, err={err}");
}

#[test]
fn pin_check_makefile_builtins_only_passes() {
    let makefile = "target:\n\tcargo build\n\tcargo test\n\tcargo clippy\n\tcargo fmt\n";
    let fix = Fixture::new(BASE_SPEC, makefile);
    let (code, out, err) = fix.run();
    assert_eq!(code, 0, "out={out}, err={err}");
}

#[test]
fn pin_check_missing_lockfile_fails_with_cannot_read() {
    let makefile = "target:\n\tcargo fake\n";
    let fix = Fixture::new(BASE_SPEC, makefile);
    let _ = fs::remove_file(fix.dir.join("Cargo.lock"));
    let (code, _out, err) = fix.run();
    assert_eq!(code, 1);
    assert!(
        err.contains("cannot read"),
        "expected stderr to contain cannot read, got:\n{err}"
    );
    assert!(
        !err.contains("cargo-fake"),
        "should fail before checking makefile"
    );
}
