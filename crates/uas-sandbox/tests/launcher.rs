//! Spec 0010 launcher gate: `cargo test -p uas-sandbox -j 2 --test launcher`.
//!
//! The cases need a kernel with Landlock. Without one they print `skipped`
//! and pass, and the PR body says which kernel ran the gate.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use landlock::RulesetCreatedAttr;
use uas_sandbox::{launch, probe, LandlockRulesetBuilder, SandboxError};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const EPERM: i32 = 1;

fn scratch(name: &str) -> Result<PathBuf, std::io::Error> {
    fresh(Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id())))
}

/// A fresh directory under `$HOME`, which no worker preset rule reaches.
fn home_scratch(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    fresh(
        Path::new(&home)
            .join(".cache")
            .join("uas-sandbox-tests")
            .join(format!("{name}-{}", std::process::id())),
    )
    .ok()
}

fn fresh(dir: PathBuf) -> Result<PathBuf, std::io::Error> {
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn landlock_available() -> Result<bool, SandboxError> {
    let report = probe()?;
    if !report.is_available() {
        eprintln!("skipped: landlock not available ({:?})", report.status);
    }
    Ok(report.is_available())
}

/// `sh -c "<script>"` with no inherited stdin and captured stderr.
fn sh(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[test]
fn jailed_child_writes_inside_the_workspace_only() -> TestResult {
    if !landlock_available()? {
        return Ok(());
    }
    let Some(home_dir) = home_scratch("launcher-home") else {
        eprintln!("skipped: HOME is unset or not writable");
        return Ok(());
    };
    let ws = scratch("launcher-ws")?;
    let inside = ws.join("ok.txt");
    let outside = home_dir.join("nope.txt");

    // Positive control: the same shell write succeeds unjailed.
    let control = sh(&format!("echo control > '{}'", outside.display())).output()?;
    assert!(control.status.success(), "control write failed");
    fs::remove_file(&outside)?;

    let (ruleset, _) = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new()).build()?;
    let output = launch(
        ruleset,
        &mut sh(&format!("echo inside > '{}'", inside.display())),
    )?
    .wait_with_output()?;
    assert!(
        output.status.success(),
        "inside write: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&inside)?.trim(), "inside");

    let (ruleset, _) = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new()).build()?;
    let output = launch(
        ruleset,
        &mut sh(&format!("echo outside > '{}'", outside.display())),
    )?
    .wait_with_output()?;
    assert!(
        !output.status.success(),
        "outside write must fail inside the jail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Permission denied"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!outside.exists(), "no file may appear under HOME");
    fs::remove_dir_all(&home_dir)?;
    Ok(())
}

#[test]
fn jailed_child_has_no_new_privs() -> TestResult {
    if !landlock_available()? {
        return Ok(());
    }
    let ws = scratch("launcher-nnp")?;
    let control = sh("grep NoNewPrivs /proc/self/status").output()?;
    println!(
        "unjailed control: {}",
        String::from_utf8_lossy(&control.stdout).trim()
    );

    // The preset has no /proc rule (spec 0010); this test adds a read rule for it.
    let (ruleset, _) = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new())
        .allow_read("/proc")
        .build()?;
    let output =
        launch(ruleset, &mut sh("grep NoNewPrivs /proc/self/status"))?.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout.trim(),
        "NoNewPrivs:\t1",
        "jailed child must report NoNewPrivs 1"
    );
    // The parent is untouched: it still writes outside the workspace.
    if let Some(home_dir) = home_scratch("launcher-parent") {
        fs::write(home_dir.join("parent.txt"), b"parent unrestricted")?;
        fs::remove_dir_all(&home_dir)?;
    }
    Ok(())
}

#[test]
fn a_jailed_command_spawns_jailed_children_every_time() -> TestResult {
    if !landlock_available()? {
        return Ok(());
    }
    let Some(home_dir) = home_scratch("launcher-twice") else {
        eprintln!("skipped: HOME is unset or not writable");
        return Ok(());
    };
    let ws = scratch("launcher-twice-ws")?;
    let outside = home_dir.join("nope.txt");
    let (ruleset, _) = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new()).build()?;
    let mut command = sh(&format!("echo outside > '{}'", outside.display()));
    uas_sandbox::jail(&mut command, ruleset);
    // The ruleset lives in the parent's closure; each child takes its own copy.
    for attempt in 1..=3 {
        let output = command.spawn()?.wait_with_output()?;
        assert!(
            !output.status.success(),
            "attempt {attempt}: the child must be jailed"
        );
        assert!(
            !outside.exists(),
            "attempt {attempt}: no file may appear under HOME"
        );
    }
    fs::remove_dir_all(&home_dir)?;
    Ok(())
}

#[test]
fn a_refused_jail_never_runs_the_worker() -> TestResult {
    if !landlock_available()? {
        return Ok(());
    }
    let ws = scratch("launcher-refused")?;
    let (ruleset, _) = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new()).build()?;
    // Without no_new_privs, one of two things happens. An unprivileged process is
    // refused by the kernel: landlock_restrict_self fails with EPERM, and that errno
    // must reach the parent unchanged (ceres). A process that already has
    // no_new_privs set, or holds CAP_SYS_ADMIN, passes the kernel check and reaches
    // the launcher's own verdict (the CI runner, which has NoNewPrivs 1 and no caps).
    let ruleset = ruleset.no_new_privs(false);
    let marker = ws.join("ran.txt");
    let err = launch(
        ruleset,
        &mut sh(&format!("echo ran > '{}'", marker.display())),
    )
    .err();
    match &err {
        Some(SandboxError::Spawn(io)) => {
            println!("refusal path: kernel errno");
            assert_eq!(
                io.raw_os_error(),
                Some(EPERM),
                "kernel errno must pass through: {io}"
            );
        }
        Some(SandboxError::JailRefused) => println!("refusal path: launcher verdict"),
        other => panic!("got {other:?}"),
    }
    assert!(!marker.exists(), "the worker must not run");
    Ok(())
}
