// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Parity test for the wave smoke receipt (spec 0049 REQ-05, plan P2).
//!
//! Saga replays the smoke plan against the same fake tools and must produce the golden
//! `.saga/parity/wave-smoke-receipt.json`. This test runs `uas wave step` on the checked-in smoke
//! plan, with the fake tools built from `.saga/parity/fake-tools/*.toml` and the repository built
//! from `.saga/parity/fixture.toml`, and fails when the build record's receipt differs from the
//! golden. The receipt's `at` timestamp is the only field left out, because it is the wall-clock
//! time of the run and is never the same twice. The receipt holds no path, so nothing else is
//! rewritten.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const STEPS: [&str; 3] = ["wave-smoke-spec", "wave-smoke-build", "wave-smoke-critic"];
const TOOLS: [&str; 5] = ["blackwall", "git", "claude", "gh", "make"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn table(path: &Path) -> toml::Table {
    read(path)
        .parse()
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// A `case` pattern: each literal part in single quotes, each `*` a glob.
fn shell_pattern(pat: &str) -> String {
    pat.split('*')
        .map(|part| format!("'{part}'"))
        .collect::<Vec<_>>()
        .join("*")
}

/// Write one executable fake tool from its rule file. The first rule whose `pat` matches the
/// joined argv wins; `out` goes to stdout, `err` to stderr, `code` is the exit status.
fn write_tool(bin: &Path, tool: &str, rules_file: &Path) {
    let rules = table(rules_file);
    let rules = rules["rule"].as_array().expect("rule array");
    let mut body = String::from("#!/bin/sh\n");
    if tool == "git" {
        body += "if [ \"$1\" = worktree ] && [ \"$2\" = add ]; then mkdir -p \"$4\"; fi\n";
    }
    if tool == "claude" {
        body += "cat > /dev/null\n";
    }
    body += "case \"$*\" in\n";
    for (i, rule) in rules.iter().enumerate() {
        let field = |k: &str| rule[k].as_str().expect("string field").to_owned();
        let out = bin.join(format!("{tool}.{i}.out"));
        let err = bin.join(format!("{tool}.{i}.err"));
        fs::write(&out, field("out")).unwrap();
        fs::write(&err, field("err")).unwrap();
        body += &format!(
            "  {pat}) cat '{out}'; cat '{err}' >&2; exit {code} ;;\n",
            pat = shell_pattern(&field("pat")),
            out = out.display(),
            err = err.display(),
            code = rule["code"].as_integer().expect("integer code")
        );
    }
    body += "  *) exit 0 ;;\nesac\n";
    let path = bin.join(tool);
    fs::write(&path, body).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}

/// Run the three smoke steps on the fakes and return the build record's receipt without `at`.
fn run_smoke(scratch: &str) -> Value {
    let top = root();
    let parity = top.join(".saga/parity");
    let base =
        std::env::temp_dir().join(format!("uas-wave-parity-{}-{scratch}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let bin = base.join("bin");
    let repo = base.join("repo");
    let state = base.join("state");
    let home = base.join("home");
    for dir in [&bin, &repo.join(".blackwall"), &state, &home] {
        fs::create_dir_all(dir).unwrap();
    }
    let fixture = table(&parity.join("fixture.toml"));
    let init_head = fixture["init_head"].as_str().unwrap();
    fs::write(repo.join(".blackwall/init-head"), format!("{init_head}\n")).unwrap();
    fs::write(
        repo.join("blackwall.toml"),
        fixture["blackwall_toml"].as_str().unwrap(),
    )
    .unwrap();
    copy_dir(
        &top.join("tasks/wave-smoke-build"),
        &repo.join("tasks/wave-smoke-build"),
    );
    for tool in TOOLS {
        write_tool(&bin, tool, &parity.join(format!("fake-tools/{tool}.toml")));
    }
    let plan = top.join(".saga/plans/uas-wave-smoke.toml");
    for step in STEPS {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_uas"));
        // Hooks and a parent git invocation must not leak into the run.
        for (key, _) in std::env::vars() {
            if key.starts_with("GIT_") {
                cmd.env_remove(key);
            }
        }
        let out = cmd
            .current_dir(&top)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("HOME", &home)
            .env("XDG_STATE_HOME", home.join("xdg"))
            .env_remove("UAS_WAVE_STATE_DIR")
            .env_remove("UAS_WAVE_REPO")
            .env_remove("UAS_WAVE_OPEN_PRS")
            .args(["wave", "step", "--flow"])
            .arg(&plan)
            .args(["--step", step, "--repo"])
            .arg(&repo)
            .arg("--state-dir")
            .arg(&state)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{step} exited {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let record: Value = serde_json::from_str(&read(&state.join("wave-smoke-build.json"))).unwrap();
    let mut receipt = record["receipt"].clone();
    receipt
        .as_object_mut()
        .expect("receipt object")
        .remove("at");
    let _ = fs::remove_dir_all(&base);
    receipt
}

#[test]
fn wave_smoke_receipt_matches_the_golden() {
    let golden: Value =
        serde_json::from_str(&read(&root().join(".saga/parity/wave-smoke-receipt.json"))).unwrap();
    let receipt = run_smoke("golden");
    let (got, want) = (receipt.as_object().unwrap(), golden.as_object().unwrap());
    for key in want.keys().chain(got.keys()) {
        assert_eq!(got.get(key), want.get(key), "receipt key {key} drifted");
    }
}
