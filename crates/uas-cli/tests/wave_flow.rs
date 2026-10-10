//! Tests for the wave saga flows and the `uas wave step` shim (spec 0025 D2, the saga flows).
//!
//! The generator and the step config are included from the binary source, so their pure
//! functions run without a subprocess. Every other test runs the built `uas` binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use uas_engine::SagaFlowDef;

#[allow(dead_code)]
#[path = "../src/wave_flow.rs"]
mod wave_flow;

use wave_flow::{StepArgs, WaveEnv};

/// The checked-in smoke plan, as `uas wave flow --from` names it from the repo root.
const SMOKE: &str = ".saga/plans/uas-wave-smoke.toml";
/// The checked-in generated flows.
const FLOW_DIR: &str = ".saga/flows";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fresh directory for each call. `cargo test` runs the tests as threads of one process, so the
/// process id alone does not make the name unique.
fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("uas-wave-flow-{name}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// Drop the `GIT_*` variables. A pre-commit hook exports them, and they would point git at the
/// repository that runs the hook, not at the temporary repositories these tests create.
fn without_git_env(cmd: &mut Command) {
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
}

fn uas(cwd: &Path, home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_uas"));
    without_git_env(&mut cmd);
    cmd.current_dir(cwd)
        .env("HOME", home)
        .env("XDG_STATE_HOME", home.join("xdg"))
        .env_remove("UAS_WAVE_STATE_DIR")
        .env_remove("UAS_WAVE_REPO")
        .env_remove("UAS_WAVE_OPEN_PRS");
    cmd
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Run `uas wave flow` on `source`, and return the generated `(file name, text)` pairs, sorted.
fn generate(source: &Path, max: Option<&str>) -> Vec<(String, String)> {
    let out_dir = scratch("gen-out");
    let mut cmd = uas(&root(), &out_dir);
    cmd.args(["wave", "flow", "--from"])
        .arg(source)
        .arg("--out-dir")
        .arg(&out_dir);
    if let Some(n) = max {
        cmd.args(["--max-concurrency", n]);
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "generator failed: {}", stderr(&out));
    read_waves(&out_dir)
}

fn read_waves(dir: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| is_wave_file(p))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read_to_string(&p).unwrap())
        })
        .collect();
    out.sort();
    out
}

fn is_wave_file(p: &Path) -> bool {
    let name = p.file_name().unwrap().to_string_lossy();
    name.starts_with("uas-cpm-wave-w") && name.ends_with(".toml")
}

/// Write `text` as a plan under a fresh directory, and return its path.
fn plan(name: &str, text: &str) -> PathBuf {
    let path = scratch(name).join("plan.toml");
    write(&path, text);
    path
}

/// Three probes in a chain: three waves, one step each.
const CHAIN: &str = r#"
[flow]
name = "fixture"

[[step]]
id = "probe-a"
title = "A"
kind = "verify"
agent = "probe"
verify_cmd = ["true"]

[[step]]
id = "probe-b"
title = "B"
kind = "verify"
agent = "probe"
deps = ["probe-a"]
verify_cmd = ["true"]

[[step]]
id = "probe-c"
title = "C"
kind = "verify"
agent = "probe"
deps = ["probe-b"]
verify_cmd = ["true"]
"#;

/// A probe, a MainSession gate that depends on it, and a probe that depends on the gate.
const WITH_GATE: &str = r#"
[flow]
name = "fixture"

[[step]]
id = "probe-a"
title = "A"
kind = "verify"
agent = "probe"
verify_cmd = ["true"]

[[step]]
id = "approve"
title = "Approve"
kind = "gate"
gate = "heimdal:approval"
deps = ["probe-a"]

[[step]]
id = "probe-b"
title = "B"
kind = "verify"
agent = "probe"
deps = ["approve"]
verify_cmd = ["true"]
"#;

/// A probe, then a MainSession gate that no step depends on. The gate stays in its own wave.
const TERMINAL_GATE: &str = r#"
[flow]
name = "fixture"

[[step]]
id = "probe-a"
title = "A"
kind = "verify"
agent = "probe"
verify_cmd = ["true"]

[[step]]
id = "approve"
title = "Approve"
kind = "gate"
gate = "heimdal:approval"
deps = ["probe-a"]
"#;

/// One probe, for the `uas wave step` tests.
const ONE_PROBE: &str = r#"
[flow]
name = "fixture"

[[step]]
id = "probe-a"
title = "A"
kind = "verify"
agent = "probe"
verify_cmd = ["true"]
"#;

/// One gate, for the refusal test.
const ONLY_GATE: &str = r#"
[flow]
name = "fixture"

[[step]]
id = "g"
title = "Gate"
kind = "gate"
gate = "heimdal:approval"
"#;

/// `(id, kind, deps, gate, verify_cmd)` for every step of a generated flow.
type StepView = (String, String, Vec<String>, Option<String>, Vec<String>);

fn steps(text: &str) -> Vec<StepView> {
    SagaFlowDef::from_toml(text)
        .unwrap()
        .steps
        .into_iter()
        .map(|s| (s.id, s.kind, s.deps, s.gate, s.verify_cmd))
        .collect()
}

fn init_repo(dir: &Path) -> String {
    fs::create_dir_all(dir).unwrap();
    let git = |args: &[&str]| {
        let mut cmd = Command::new("git");
        without_git_env(&mut cmd);
        let out = cmd.current_dir(dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", stderr(&out));
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    git(&["init", "-q"]);
    git(&[
        "-c",
        "user.name=test",
        "-c",
        "user.email=test@example.com",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "init",
    ]);
    let sha = git(&["rev-parse", "HEAD"]);
    write(&dir.join(".blackwall/init-head"), &format!("{sha}\n"));
    sha
}

// ---------------------------------------------------------------- generator

#[test]
fn wave_flow_roundtrips_through_saga_flow_def() {
    for (name, text) in read_waves(&root().join(FLOW_DIR)) {
        let def = SagaFlowDef::from_toml(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            name,
            format!("{}.toml", def.flow.name),
            "file name follows the flow name"
        );
    }
    let fixture = generate(&plan("roundtrip", CHAIN), None);
    assert_eq!(fixture.len(), 3);
    for (name, text) in &fixture {
        let def = SagaFlowDef::from_toml(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(name, &format!("{}.toml", def.flow.name));
    }
}

#[test]
fn wave_flow_one_file_per_wave() {
    let files = generate(&plan("one-per-wave", CHAIN), None);
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "uas-cpm-wave-w0.toml",
            "uas-cpm-wave-w1.toml",
            "uas-cpm-wave-w2.toml"
        ]
    );
}

#[test]
fn wave_flow_no_gate_between_waves() {
    for (name, text) in generate(&plan("no-gate", CHAIN), None) {
        for (id, kind, _, _, _) in steps(&text) {
            assert_ne!(kind, "gate", "{name} holds gate step {id}");
        }
    }
    for (name, text) in read_waves(&root().join(FLOW_DIR)) {
        assert!(!text.contains("heimdal"), "{name} holds an approval gate");
    }
}

/// A wave whose only step is a gate that moved to its dependent's wave writes no file. The other
/// waves keep their plan indices, so the names skip wave 1 and do not renumber wave 2.
/// A terminal gate keeps its own wave: wave 1 holds the gate and no step, and names no agent.
#[test]
fn wave_flow_terminal_gate_keeps_its_own_wave() {
    let files = generate(&plan("terminal-gate", TERMINAL_GATE), None);
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        ["uas-cpm-wave-w0.toml", "uas-cpm-wave-w1.toml"],
        "the terminal gate writes its own wave file"
    );
    let w1 = &files[1].1;
    let view = steps(w1);
    assert_eq!(view.len(), 1, "wave 1 holds only the gate");
    assert_eq!(view[0].0, "approve");
    assert_eq!(view[0].1, "gate");
    assert_eq!(view[0].3.as_deref(), Some("heimdal:approval"));
    let def = SagaFlowDef::from_toml(w1).unwrap();
    assert_eq!(def.steps[0].agent, None, "a gate names no agent");
}

#[test]
fn wave_flow_skips_a_wave_whose_only_step_moved() {
    let files = generate(&plan("empty-wave", WITH_GATE), None);
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        ["uas-cpm-wave-w0.toml", "uas-cpm-wave-w2.toml"],
        "no file for the emptied wave 1, and wave 2 keeps its name"
    );
    for (name, text) in &files {
        assert!(!steps(text).is_empty(), "{name} holds no steps");
        assert!(
            !text.contains("step = []"),
            "{name} holds an empty step list"
        );
    }
}

#[test]
fn wave_flow_main_session_gate_in_dependent_wave() {
    let files = generate(&plan("gate-dep", WITH_GATE), None);
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        ["uas-cpm-wave-w0.toml", "uas-cpm-wave-w2.toml"],
        "wave 1 holds only the gate, which moved to wave 2, so it writes no file"
    );
    let by_name = |n: &str| files.iter().find(|(f, _)| f == n).unwrap().1.clone();
    assert!(
        !by_name("uas-cpm-wave-w0.toml").contains("approve"),
        "the gate is not in its own wave when it has a dependent elsewhere"
    );
    let w2 = steps(&by_name("uas-cpm-wave-w2.toml"));
    let gate = w2
        .iter()
        .find(|s| s.0 == "approve")
        .expect("gate in the dependent wave");
    assert_eq!(gate.1, "gate");
    assert_eq!(gate.3.as_deref(), Some("heimdal:approval"));
    let dependent = w2
        .iter()
        .find(|s| s.0 == "probe-b")
        .expect("dependent in w2");
    assert_eq!(
        dependent.2,
        vec!["approve".to_owned()],
        "the dependent waits on the gate in its file"
    );
}

#[test]
fn wave_flow_deps_stay_in_file() {
    for source in [CHAIN, WITH_GATE] {
        for (name, text) in generate(&plan("deps-in-file", source), None) {
            let all = steps(&text);
            let ids: Vec<String> = all.iter().map(|s| s.0.clone()).collect();
            for (id, _, deps, _, _) in &all {
                for dep in deps {
                    assert!(
                        ids.contains(dep),
                        "{name}: step {id} depends on {dep} outside the file"
                    );
                }
            }
        }
    }
    let chain_w1 = generate(&plan("deps-chain", CHAIN), None)
        .into_iter()
        .find(|(n, _)| n == "uas-cpm-wave-w1.toml")
        .unwrap()
        .1;
    assert!(
        steps(&chain_w1).iter().all(|s| s.2.is_empty()),
        "the cross-wave dep is dropped"
    );
}

/// Every shim step names `agent = "auto"`, as the shipped flows do. A gate names no agent, as
/// `.saga/flows/gates.toml` does. Saga fails a step with no agent as "unassigned agent".
#[test]
fn wave_flow_every_shim_step_names_agent_auto() {
    let files = generate(&plan("agent-auto", WITH_GATE), None);
    assert!(!files.is_empty());
    for (name, text) in &files {
        let def = SagaFlowDef::from_toml(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        for step in &def.steps {
            if step.kind == "gate" {
                assert_eq!(step.agent, None, "{name}: gate {} names an agent", step.id);
            } else {
                assert_eq!(
                    step.agent.as_deref(),
                    Some("auto"),
                    "{name}: step {} has no agent auto",
                    step.id
                );
            }
        }
    }
    for (name, text) in read_waves(&root().join(FLOW_DIR)) {
        let def = SagaFlowDef::from_toml(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            def.steps
                .iter()
                .all(|s| s.kind == "gate" || s.agent.as_deref() == Some("auto")),
            "{name}: a checked-in step has no agent auto"
        );
    }
}

#[test]
fn wave_flow_verify_cmd_is_literal_argv() {
    let files = read_waves(&root().join(FLOW_DIR));
    assert!(!files.is_empty(), "the checked-in wave flows exist");
    for (name, text) in files {
        assert!(!text.contains("{{"), "{name} has a template placeholder");
        for (id, kind, _, _, verify) in steps(&text) {
            if kind == "gate" {
                continue;
            }
            assert_eq!(
                verify,
                [
                    "uas",
                    "wave",
                    "step",
                    "--flow",
                    SMOKE,
                    "--step",
                    id.as_str()
                ],
                "{name}: step {id}"
            );
        }
    }
}

#[test]
fn wave_flow_limits_match_plan() {
    let limit = |files: &[(String, String)]| {
        let v: toml::Value = toml::from_str(&files[0].1).unwrap();
        v["limits"]["max_in_flight"].as_integer().unwrap()
    };
    assert_eq!(limit(&generate(&plan("limits-2", CHAIN), Some("2"))), 2);
    assert_eq!(
        limit(&generate(&plan("limits-0", CHAIN), None)),
        3,
        "0 means the default of 3"
    );
    assert_eq!(limit(&read_waves(&root().join(FLOW_DIR))), 3);
}

#[test]
fn wave_flow_checked_in_matches_generator() {
    let generated = {
        let out_dir = scratch("drift-out");
        let out = uas(&root(), &out_dir)
            .args(["wave", "flow", "--from", SMOKE, "--out-dir"])
            .arg(&out_dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "generator failed: {}", stderr(&out));
        // The header names the out dir as given. The checked-in flows name `.saga/flows`.
        let dir = out_dir.display().to_string();
        read_waves(&out_dir)
            .into_iter()
            .map(|(name, text)| (name, text.replace(&dir, FLOW_DIR)))
            .collect::<Vec<_>>()
    };
    let checked_in = read_waves(&root().join(FLOW_DIR));
    let names = |v: &[(String, String)]| v.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
    assert_eq!(
        names(&checked_in),
        names(&generated),
        "same wave files, no extra file"
    );
    assert!(!generated.is_empty(), "the smoke plan has waves");
    for ((name, want), (_, have)) in generated.iter().zip(&checked_in) {
        assert_eq!(
            have, want,
            "{name} differs from the generator; regenerate, do not edit"
        );
    }
}

/// Split a line into shell words: bare words, single-quoted spans, and `\'` outside quotes.
fn shell_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' => {
                if has {
                    words.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            '\'' => {
                has = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(x) => cur.push(x),
                        None => panic!("unterminated quote in {line}"),
                    }
                }
            }
            '\\' => {
                has = true;
                if let Some(x) = chars.next() {
                    cur.push(x);
                }
            }
            x => {
                has = true;
                cur.push(x);
            }
        }
    }
    if has {
        words.push(cur);
    }
    words
}

#[test]
fn wave_flow_header_command_parses_with_cli() {
    let source = plan("header with space", CHAIN);
    let out_dir = scratch("header out's dir");
    let out = uas(&root(), &out_dir)
        .args(["wave", "flow", "--from"])
        .arg(&source)
        .args(["--max-concurrency", "2", "--out-dir"])
        .arg(&out_dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "generator failed: {}", stderr(&out));
    let files = read_waves(&out_dir);
    let first = files.first().expect("the chain has waves");
    let header = first.1.lines().next().unwrap();
    let line = header
        .strip_prefix("# Generated by: ")
        .expect("the header names the command");
    let argv = shell_words(line);
    assert_eq!(argv[0], "uas");
    let out_str = out_dir.to_str().unwrap();
    assert!(
        argv.windows(2)
            .any(|w| w[0] == "--out-dir" && w[1] == out_str),
        "the header names the out dir as given: {line}"
    );
    // The header's own command line, split with shell rules, parses with the CLI and writes the same flows.
    let rerun = uas(&root(), &out_dir).args(&argv[1..]).output().unwrap();
    assert!(
        rerun.status.success(),
        "header command failed: {}",
        stderr(&rerun)
    );
    assert_eq!(
        read_waves(&out_dir),
        files,
        "the header command rewrites the same flows"
    );
}

// ---------------------------------------------------------------- uas wave step

#[test]
fn wave_step_idempotent_on_passed_record() {
    let home = scratch("idem");
    let repo = home.join("repo");
    let sha = init_repo(&repo);
    let state = home.join("state");
    let record = format!(
        r#"{{"step":"probe-a","role":"Probe","outcome":{{"result":"passed"}},"base":"{sha}","at":1,"seq":0}}"#
    );
    write(&state.join("probe-a.json"), &record);
    let src = plan("idem-plan", ONE_PROBE);
    let out = uas(&root(), &home)
        .args(["wave", "step", "--flow"])
        .arg(&src)
        .args(["--step", "probe-a", "--repo"])
        .arg(&repo)
        .arg("--state-dir")
        .arg(&state)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("\"probe-a\""),
        "prints the record: {}",
        stdout(&out)
    );
    assert_eq!(
        fs::read_to_string(state.join("probe-a.json")).unwrap(),
        record,
        "record untouched"
    );
    assert!(!state.join("scratch").exists(), "nothing ran");
}

#[test]
fn wave_step_any_halted_record_exits_1() {
    let home = scratch("halted");
    let repo = home.join("repo");
    init_repo(&repo);
    let state = home.join("state");
    write(
        &state.join("sibling.json"),
        r#"{"step":"sibling","role":"Builder","outcome":{"result":"halted","reason":"EvaluatorTouched"},"at":1,"seq":0}"#,
    );
    let src = plan("halted-plan", ONE_PROBE);
    let out = uas(&root(), &home)
        .args(["wave", "step", "--flow"])
        .arg(&src)
        .args(["--step", "probe-a", "--repo"])
        .arg(&repo)
        .arg("--state-dir")
        .arg(&state)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("EvaluatorTouched"),
        "prints the halted record"
    );
    assert!(
        !state.join("probe-a.json").exists(),
        "a sibling halt runs nothing"
    );
}

#[test]
fn wave_step_refuses_gate_step() {
    let home = scratch("gate");
    let state = home.join("state");
    let src = plan("gate-plan", ONLY_GATE);
    let out = uas(&root(), &home)
        .args(["wave", "step", "--flow"])
        .arg(&src)
        .args(["--step", "g", "--repo"])
        .arg(&home)
        .arg("--state-dir")
        .arg(&state)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("gate steps belong to saga"),
        "stderr: {}",
        stderr(&out)
    );
    assert!(!state.exists(), "no record for a gate");
}

#[test]
fn wave_step_no_init_head_exits_2() {
    let home = scratch("noinit");
    let repo = home.join("repo");
    fs::create_dir_all(&repo).unwrap();
    let state = home.join("state");
    let src = plan("noinit-plan", ONE_PROBE);
    let out = uas(&root(), &home)
        .args(["wave", "step", "--flow"])
        .arg(&src)
        .args(["--step", "probe-a", "--repo"])
        .arg(&repo)
        .arg("--state-dir")
        .arg(&state)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("NoInitHead"),
        "names the not-ready reason: {}",
        stderr(&out)
    );
    assert!(
        !state.join("probe-a.json").exists(),
        "not ready writes no record"
    );
}

#[test]
fn wave_step_state_dir_keyed_on_source() {
    let src = plan("keyed-plan", ONE_PROBE);
    // A halt under the source's own flow name blocks the step.
    let home = scratch("keyed-hit");
    let repo = home.join("repo");
    fs::create_dir_all(&repo).unwrap();
    write(
        &home.join("xdg/uas/wave/fixture/sib.json"),
        r#"{"step":"sib","role":"Builder","outcome":{"result":"halted","reason":"X"},"at":1,"seq":0}"#,
    );
    let hit = uas(&root(), &home)
        .args(["wave", "step", "--flow"])
        .arg(&src)
        .args(["--step", "probe-a", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert_eq!(hit.status.code(), Some(1), "stderr: {}", stderr(&hit));
    // A halt under another plan's name does not.
    let other = scratch("keyed-miss");
    let repo2 = other.join("repo");
    fs::create_dir_all(&repo2).unwrap();
    write(
        &other.join("xdg/uas/wave/other-plan/sib.json"),
        r#"{"step":"sib","role":"Builder","outcome":{"result":"halted","reason":"X"},"at":1,"seq":0}"#,
    );
    let miss = uas(&root(), &other)
        .args(["wave", "step", "--flow"])
        .arg(&src)
        .args(["--step", "probe-a", "--repo"])
        .arg(&repo2)
        .output()
        .unwrap();
    assert_eq!(miss.status.code(), Some(2), "stderr: {}", stderr(&miss));
}

#[test]
fn wave_step_state_dir_env_order() {
    let src = plan("order-plan", ONE_PROBE);
    let halted = r#"{"step":"sib","role":"Builder","outcome":{"result":"halted","reason":"X"},"at":1,"seq":0}"#;
    let run = |home: &Path, flag: Option<&Path>, env_dir: Option<&Path>| {
        let repo = home.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let mut cmd = uas(&root(), home);
        cmd.args(["wave", "step", "--flow"])
            .arg(&src)
            .args(["--step", "probe-a", "--repo"])
            .arg(&repo);
        if let Some(f) = flag {
            cmd.arg("--state-dir").arg(f);
        }
        if let Some(e) = env_dir {
            cmd.env("UAS_WAVE_STATE_DIR", e);
        }
        cmd.output().unwrap().status.code()
    };
    // The env var alone selects the directory.
    let a = scratch("order-env");
    write(&a.join("env/sib.json"), halted);
    assert_eq!(run(&a, None, Some(&a.join("env"))), Some(1));
    // The flag beats the env var: the flag dir is empty, so the step is not halted.
    let b = scratch("order-flag");
    write(&b.join("env/sib.json"), halted);
    fs::create_dir_all(b.join("flag")).unwrap();
    assert_eq!(
        run(&b, Some(&b.join("flag")), Some(&b.join("env"))),
        Some(2)
    );
    // With no flag and no env var, the default is under XDG_STATE_HOME.
    let c = scratch("order-default");
    write(&c.join("xdg/uas/wave/fixture/sib.json"), halted);
    assert_eq!(run(&c, None, None), Some(1));
}

#[test]
fn wave_step_open_prs_off_unless_env() {
    let cwd = scratch("open-prs");
    let args = StepArgs {
        flow: "unused".into(),
        step: "probe-a".into(),
        repo: None,
        state_dir: None,
        max_concurrency: 0,
    };
    let cfg = |open: Option<&str>| {
        let env = WaveEnv {
            open_prs: open.map(Into::into),
            xdg_state_home: Some("/xdg".into()),
            ..WaveEnv::default()
        };
        wave_flow::step_config(
            &args,
            &env,
            "fixture",
            &wave_flow::PlanMerge::default(),
            &cwd,
        )
    };
    assert!(!cfg(None).open_prs, "off by default");
    assert!(!cfg(Some("0")).open_prs);
    assert!(!cfg(Some("true")).open_prs, "only the value 1 turns it on");
    assert!(cfg(Some("1")).open_prs);
    assert_eq!(cfg(None).lock_path, PathBuf::from("/xdg/uas/build.lock"));
}

/// A Builder and its Critic. Admission passes with the task files, and then the step runs.
const BUILDER_PLAN: &str = r#"
[flow]
name = "fixture"

[[step]]
id = "wave-x-build"
title = "Build"
kind = "impl"
agent = "builder"

[[step]]
id = "wave-x-critic"
title = "Review"
kind = "verify"
agent = "critic"
deps = ["wave-x-build"]
"#;

/// A probe, and a Builder with no Critic. Admission refuses the plan.
const UNADMITTED: &str = r#"
[flow]
name = "fixture"

[[step]]
id = "probe-a"
title = "A"
kind = "verify"
agent = "probe"
verify_cmd = ["true"]

[[step]]
id = "build-x"
title = "Build"
kind = "impl"
agent = "builder"
"#;

#[test]
fn wave_flow_every_step_has_prompt() {
    let mut texts: Vec<(String, String)> = read_waves(&root().join(FLOW_DIR));
    texts.extend(generate(&plan("prompt-gate", WITH_GATE), None));
    for (name, text) in texts {
        let def = SagaFlowDef::from_toml(&text).unwrap();
        let prompts = text.lines().filter(|l| l.starts_with("prompt = ")).count();
        assert_eq!(
            prompts,
            def.steps.len(),
            "{name}: every step has a prompt line"
        );
        for s in &def.steps {
            let p = s.prompt.as_deref().unwrap_or("").trim();
            assert!(!p.is_empty(), "{name}: step {} has an empty prompt", s.id);
        }
    }
}

#[test]
fn wave_flow_max_concurrency_in_verify_cmd() {
    let src = plan("maxc", CHAIN);
    let files = generate(&src, Some("2"));
    for (name, text) in &files {
        assert!(
            text.contains("--max-concurrency 2"),
            "{name}: header names the flag"
        );
        for (id, kind, _, _, verify) in steps(text) {
            if kind == "gate" {
                continue;
            }
            assert_eq!(
                verify[verify.len() - 2..],
                ["--max-concurrency".to_owned(), "2".to_owned()],
                "{name}: step {id} carries the flag"
            );
        }
    }
    // The step command accepts the flag and plans with it: no init-head, so exit 2 by readiness.
    let home = scratch("maxc-step");
    let repo = home.join("repo");
    fs::create_dir_all(&repo).unwrap();
    let out = uas(&root(), &home)
        .args(["wave", "step", "--flow"])
        .arg(&src)
        .args(["--step", "probe-a", "--max-concurrency", "2", "--repo"])
        .arg(&repo)
        .arg("--state-dir")
        .arg(home.join("state"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("NoInitHead"),
        "reached readiness: {}",
        stderr(&out)
    );
}

#[test]
fn wave_step_stale_own_pass_exits_2() {
    let home = scratch("stale");
    let repo = home.join("repo");
    init_repo(&repo);
    let state = home.join("state");
    let record = r#"{"step":"probe-a","role":"Probe","outcome":{"result":"passed"},"base":"0000000000000000000000000000000000000000","at":1,"seq":0}"#;
    write(&state.join("probe-a.json"), record);
    let src = plan("stale-plan", ONE_PROBE);
    let out = uas(&root(), &home)
        .args(["wave", "step", "--flow"])
        .arg(&src)
        .args(["--step", "probe-a", "--repo"])
        .arg(&repo)
        .arg("--state-dir")
        .arg(&state)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("stale"),
        "says stale: {}",
        stderr(&out)
    );
    assert_eq!(
        fs::read_to_string(state.join("probe-a.json")).unwrap(),
        record,
        "record untouched"
    );
    assert!(!state.join("scratch").exists(), "nothing ran");
}

#[test]
fn wave_step_own_pass_before_admission() {
    let home = scratch("own-first");
    let repo = home.join("repo");
    let sha = init_repo(&repo);
    let state = home.join("state");
    let record = format!(
        r#"{{"step":"probe-a","role":"Probe","outcome":{{"result":"passed"}},"base":"{sha}","at":1,"seq":0}}"#
    );
    write(&state.join("probe-a.json"), &record);
    let src = plan("own-first-plan", UNADMITTED);
    let out = uas(&root(), &home)
        .args(["wave", "step", "--flow"])
        .arg(&src)
        .args(["--step", "probe-a", "--repo"])
        .arg(&repo)
        .arg("--state-dir")
        .arg(&state)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "a current pass exits 0 before admission: {}",
        stderr(&out)
    );
    assert!(
        stdout(&out).contains("\"probe-a\""),
        "prints the record: {}",
        stdout(&out)
    );
    assert_eq!(
        fs::read_to_string(state.join("probe-a.json")).unwrap(),
        record,
        "record untouched"
    );
}

#[test]
fn wave_step_executor_error_halts_and_exits_3() {
    let home = scratch("exec-err");
    let repo = home.join("repo");
    init_repo(&repo);
    write(
        &repo.join("tasks/wave-x-build/prompt.md"),
        "Build the thing.\n",
    );
    write(
        &repo.join("tasks/wave-x-build/task.toml"),
        "provider = \"claude\"\nmodel = \"sonnet\"\n",
    );
    // A PATH with git and no blackwall: the Builder's first call cannot start.
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let git = std::env::var_os("PATH")
        .and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("git"))
                .find(|g| g.is_file())
        })
        .expect("git on PATH");
    std::os::unix::fs::symlink(git, bin.join("git")).unwrap();
    let state = home.join("state");
    let src = plan("exec-err-plan", BUILDER_PLAN);
    let out = uas(&root(), &home)
        .env("PATH", &bin)
        .args(["wave", "step", "--flow"])
        .arg(&src)
        .args(["--step", "wave-x-build", "--repo"])
        .arg(&repo)
        .arg("--state-dir")
        .arg(&state)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("ExecutorError"),
        "prints the reason: {}",
        stderr(&out)
    );
    let rec: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(state.join("wave-x-build.json")).unwrap())
            .unwrap();
    assert_eq!(rec["outcome"]["result"], "halted");
    assert_eq!(rec["outcome"]["reason"], "ExecutorError");
}

// ------------------------------------------------------------ merge on green

/// A Builder and its Critic, with the merge keys under `[flow]`.
const MERGE_PLAN: &str = r#"
[flow]
name = "fixture"
merge_on_green = true
merge_wait_secs = 120

[[step]]
id = "wave-x-build"
title = "Build"
kind = "impl"
agent = "builder"

[[step]]
id = "wave-x-critic"
title = "Review"
kind = "verify"
agent = "critic"
deps = ["wave-x-build"]
"#;

fn merge_step_of(
    files: &[(String, String)],
) -> Vec<(String, String, Vec<String>, Option<String>, Vec<String>)> {
    files
        .iter()
        .flat_map(|(_, text)| steps(text))
        .filter(|s| s.0.starts_with("merge-"))
        .collect()
}

#[test]
fn wave_flow_merge_steps_emitted() {
    let src = plan("merge-emit", MERGE_PLAN);
    let argv = |wait: &str, extra: &[&str]| -> Vec<String> {
        let mut v: Vec<String> = [
            "timeout",
            "-k",
            "30",
            wait,
            "uas",
            "wave",
            "merge-gate",
            "--flow",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        v.push(src.display().to_string());
        v.extend(
            ["--builder", "wave-x-build"]
                .iter()
                .map(|s| (*s).to_owned()),
        );
        v.extend(extra.iter().map(|s| (*s).to_owned()));
        v
    };
    let files = generate(&src, None);
    let got = merge_step_of(&files);
    assert_eq!(got.len(), 1, "{got:?}");
    let (id, kind, deps, gate, verify) = &got[0];
    assert_eq!((id.as_str(), kind.as_str()), ("merge-wave-x-build", "test"));
    assert_eq!(deps, &["wave-x-critic"]);
    assert_eq!(gate, &None);
    assert_eq!(verify, &argv("180", &[]));
    // The step sits in the Critic's flow, and the shim prompt and agent are the same.
    let critic_flow = files
        .iter()
        .find(|(_, t)| t.contains("\"wave-x-critic\""))
        .unwrap();
    assert!(critic_flow.1.contains("id = \"merge-wave-x-build\""));
    assert!(critic_flow.1.matches("agent = \"auto\"").count() == 2);
    // --max-concurrency is repeated, as for a shim step.
    assert_eq!(
        merge_step_of(&generate(&src, Some("2")))[0].4,
        argv("180", &["--max-concurrency", "2"])
    );
    // The default wait is 900 s, so the outer timeout is 960.
    let no_wait = plan(
        "merge-emit-default",
        &MERGE_PLAN.replace("merge_wait_secs = 120\n", ""),
    );
    assert_eq!(merge_step_of(&generate(&no_wait, None))[0].4[3], "960");
}

#[test]
fn wave_flow_merge_id_collision_refused() {
    let text = format!(
        "{MERGE_PLAN}\n[[step]]\nid = \"merge-wave-x-build\"\ntitle = \"Clash\"\nkind = \"verify\"\nagent = \"probe\"\nverify_cmd = [\"true\"]\n"
    );
    let src = plan("merge-clash", &text);
    let out_dir = scratch("merge-clash-out");
    write(&out_dir.join("uas-cpm-wave-w9.toml"), "stale\n");
    let out = uas(&root(), &out_dir)
        .args(["wave", "flow", "--from"])
        .arg(&src)
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("merge-wave-x-build"),
        "{}",
        stderr(&out)
    );
    // It writes no file and removes none.
    assert_eq!(read_waves(&out_dir).len(), 1);
    assert!(out_dir.join("uas-cpm-wave-w9.toml").exists());
    // Control: with the flag off the same plan generates.
    let off = plan(
        "merge-clash-off",
        &text.replace("merge_on_green = true", "merge_on_green = false"),
    );
    assert!(!generate(&off, None).is_empty());
}

#[test]
fn wave_step_bad_merge_key_exits_2() {
    let src = plan(
        "merge-bad",
        &MERGE_PLAN.replace("merge_on_green = true", "merge_on_green = \"yes\""),
    );
    let home = scratch("merge-bad-home");
    let out = uas(&root(), &home)
        .args(["wave", "step", "--flow"])
        .arg(&src)
        .args(["--step", "wave-x-build", "--state-dir"])
        .arg(home.join("state"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("parse plan"), "{}", stderr(&out));
}

/// A state dir with a Passed Builder record (PR 57), a plan, and a `bin` with a `git` shim that
/// prints `log` as given and a fake `blackwall` that logs its argv, cwd and the automerge variable.
struct Gate {
    home: PathBuf,
    repo: PathBuf,
    bin: PathBuf,
    state: PathBuf,
    src: PathBuf,
}

fn gate_case(name: &str, log: &str, blackwall_exit: i32) -> Gate {
    let home = scratch(name);
    let (repo, bin, state) = (home.join("repo"), home.join("bin"), home.join("state"));
    fs::create_dir_all(&repo).unwrap();
    // The custody checkout: init-head, and the task files that admission reads.
    write(&repo.join(".blackwall/init-head"), "aaa\n");
    write(&repo.join("blackwall.toml"), "[merge]\nautomerge = false\n");
    write(&repo.join("tasks/wave-x-build/prompt.md"), "Build.\n");
    write(
        &repo.join("tasks/wave-x-build/task.toml"),
        "model = \"sonnet\"\n",
    );
    let real_git = std::env::var_os("PATH")
        .and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("git"))
                .find(|g| g.is_file())
        })
        .expect("git on PATH");
    let script = |body: String, name: &str| {
        write(&bin.join(name), &format!("#!/bin/sh\n{body}"));
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o755)).unwrap();
    };
    script(
        format!("if [ \"$1\" = log ] && [ \"$2\" = '--format=%G?' ]; then printf '{log}'; exit 0; fi\nexec '{}' \"$@\"\n", real_git.display()),
        "git",
    );
    script(
        format!(
            "{{ echo \"$*\"; pwd -P; echo \"AUTOMERGE=${{BLACKWALL_AUTOMERGE-absent}}\"; }} > '{}'\necho 'ready: 57'\nexit {blackwall_exit}\n",
            home.join("blackwall.log").display()
        ),
        "blackwall",
    );
    write(
        &state.join("wave-x-build.json"),
        r#"{"step":"wave-x-build","role":"Builder","outcome":{"result":"passed"},"base":"aaa","head":"bbb","pr":57,"at":1,"seq":0}"#,
    );
    let src = plan(&format!("{name}-plan"), MERGE_PLAN);
    Gate {
        home,
        repo,
        bin,
        state,
        src,
    }
}

fn run_gate(g: &Gate) -> Output {
    run_gate_with(g, true)
}

/// `open_prs` sets `UAS_WAVE_OPEN_PRS=1`, which the saga run env carries to a merge step.
fn run_gate_with(g: &Gate, open_prs: bool) -> Output {
    let mut cmd = uas(&root(), &g.home);
    if open_prs {
        cmd.env("UAS_WAVE_OPEN_PRS", "1");
    }
    cmd.env("PATH", &g.bin)
        .env("BLACKWALL_AUTOMERGE", "true")
        .args(["wave", "merge-gate", "--flow"])
        .arg(&g.src)
        .args(["--builder", "wave-x-build", "--repo"])
        .arg(&g.repo)
        .arg("--state-dir")
        .arg(&g.state)
        .output()
        .unwrap()
}

#[test]
fn wave_merge_gate_execs_blackwall() {
    let g = gate_case("gate-exec", "G\\nG\\n", 7);
    let out = run_gate(&g);
    assert_eq!(out.status.code(), Some(7), "stderr: {}", stderr(&out));
    let log = fs::read_to_string(g.home.join("blackwall.log")).unwrap();
    let want = format!(
        "merge-gate 57 --watch --timeout 120\n{}\nAUTOMERGE=absent\n",
        g.repo.canonicalize().unwrap().display()
    );
    assert_eq!(log, want);
}

#[test]
fn wave_merge_gate_ready_on_exit_0() {
    let g = gate_case("gate-ready", "G\\n", 0);
    let out = run_gate(&g);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("ready: 57"), "{}", stdout(&out));
}

#[test]
fn wave_merge_gate_refuses_unsigned_cli() {
    let g = gate_case("gate-unsigned", "G\\nN\\n", 0);
    let out = run_gate(&g);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("merge-gate refused"),
        "{}",
        stderr(&out)
    );
    assert!(!g.home.join("blackwall.log").exists(), "no blackwall ran");
}

#[test]
fn wave_step_merge_config_from_plan_and_admission() {
    let cwd = scratch("merge-cfg");
    let args = StepArgs {
        flow: "unused".into(),
        step: "wave-x-build".into(),
        repo: None,
        state_dir: None,
        max_concurrency: 0,
    };
    let cfg = |merge: &wave_flow::PlanMerge| {
        wave_flow::step_config(&args, &WaveEnv::default(), "fixture", merge, &cwd)
    };
    let off = cfg(&wave_flow::PlanMerge::default());
    assert!(!off.merge_on_green);
    assert_eq!(off.merge_wait, std::time::Duration::from_secs(900));
    let on = cfg(&wave_flow::plan_merge(MERGE_PLAN).unwrap());
    assert!(on.merge_on_green && !on.open_prs);
    assert_eq!(on.merge_wait, std::time::Duration::from_secs(120));
    // Through the binary: the plan alone asks for merge, and admission refuses it without
    // UAS_WAVE_OPEN_PRS. With the variable set the merge row no longer fires.
    let src = plan("merge-admit", MERGE_PLAN);
    let run = |open: bool| {
        let home = scratch("merge-admit-home");
        let mut cmd = uas(&root(), &home);
        cmd.args(["wave", "step", "--flow"])
            .arg(&src)
            .args(["--step", "wave-x-build", "--repo"])
            .arg(home.join("repo"))
            .arg("--state-dir")
            .arg(home.join("state"));
        if open {
            cmd.env("UAS_WAVE_OPEN_PRS", "1");
        }
        cmd.output().unwrap()
    };
    let refused = run(false);
    assert_eq!(refused.status.code(), Some(2), "{}", stderr(&refused));
    assert!(
        stderr(&refused).contains("merge_on_green needs open_prs"),
        "{}",
        stderr(&refused)
    );
    assert!(!stderr(&run(true)).contains("merge_on_green needs open_prs"));
}

/// A refusal: exit 1, `merge-gate refused:` on stderr, and no blackwall ran.
fn assert_gate_refused(g: &Gate, out: &Output, why: &str) {
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(out));
    let err = stderr(out);
    assert!(
        err.contains("merge-gate refused:") && err.contains(why),
        "{err}"
    );
    assert!(!g.home.join("blackwall.log").exists(), "no blackwall ran");
}

#[test]
fn wave_merge_gate_refuses_no_init_head() {
    let g = gate_case("gate-no-init", "G\\n", 0);
    fs::remove_file(g.repo.join(".blackwall/init-head")).unwrap();
    assert_gate_refused(&g, &run_gate(&g), "init-head");
}

#[test]
fn wave_merge_gate_refuses_failed_admission() {
    let g = gate_case("gate-no-prs", "G\\n", 0);
    assert_gate_refused(
        &g,
        &run_gate_with(&g, false),
        "merge_on_green needs open_prs",
    );
}

#[test]
fn wave_merge_gate_refuses_no_repo() {
    let g = gate_case("gate-no-repo", "G\\n", 0);
    // The cwd holds a valid custody tree, and neither --repo nor UAS_WAVE_REPO names it.
    let out = uas(&g.repo, &g.home)
        .env("UAS_WAVE_OPEN_PRS", "1")
        .env("PATH", &g.bin)
        .args(["wave", "merge-gate", "--flow"])
        .arg(&g.src)
        .args(["--builder", "wave-x-build", "--state-dir"])
        .arg(&g.state)
        .output()
        .unwrap();
    assert_gate_refused(&g, &out, "--repo");
    // Control: UAS_WAVE_REPO names the tree, so it runs.
    let ok = uas(&g.home, &g.home)
        .env("UAS_WAVE_OPEN_PRS", "1")
        .env("UAS_WAVE_REPO", &g.repo)
        .env("PATH", &g.bin)
        .args(["wave", "merge-gate", "--flow"])
        .arg(&g.src)
        .args(["--builder", "wave-x-build", "--state-dir"])
        .arg(&g.state)
        .output()
        .unwrap();
    assert_eq!(ok.status.code(), Some(0), "stderr: {}", stderr(&ok));
}

#[test]
fn wave_merge_gate_refuses_armed_toml() {
    let g = gate_case("gate-armed", "G\\n", 0);
    for armed in [
        "[merge]\nautomerge = true\n",
        "[merge]\nautomerge = \"true\"\n",
    ] {
        write(&g.repo.join("blackwall.toml"), armed);
        assert_gate_refused(&g, &run_gate(&g), "automerge");
    }
}

#[test]
fn wave_merge_gate_refuses_unparsable_toml() {
    let g = gate_case("gate-bad-toml", "G\\n", 0);
    write(
        &g.repo.join("blackwall.toml"),
        "[merge\nautomerge = = true\n",
    );
    assert_gate_refused(&g, &run_gate(&g), "blackwall.toml");
    fs::remove_file(g.repo.join("blackwall.toml")).unwrap();
    assert_gate_refused(&g, &run_gate(&g), "blackwall.toml");
}

#[test]
fn wave_merge_gate_refuses_absent_automerge() {
    let g = gate_case("gate-absent-key", "G\\n", 0);
    // The file parses but never says `automerge = false`: fail closed.
    for text in ["", "[merge]\n", "[verify]\ncommand = \"make check\"\n"] {
        write(&g.repo.join("blackwall.toml"), text);
        assert_gate_refused(&g, &run_gate(&g), "automerge");
    }
}
