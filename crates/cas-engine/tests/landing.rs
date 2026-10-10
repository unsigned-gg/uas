//! Landing activity tests. Each test puts fake `git` and `blackwall` scripts first on `PATH`.
//! The scripts log their argv and return canned output, so no real repository is touched.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use cas_domain::{AuditStatus, CostSource, EvidenceTier, SlotId};
use cas_engine::{LandingActivity, LandingConfig, LandingInput, LandingOutcome};

const BASE: &str = "056d7c7abcdef";
const RUN: &str = "bw-0123456789abcdef";
const BRANCH: &str = "blackwall/bw-012345678";

/// A fresh temp dir for one test, holding the fake tools and their canned output.
fn scratch(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cas-landing-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write `#!/bin/sh` script `name`. It appends its argv to `argv.log`, then prints
/// `<name>-<subcommand>.out` and exits with `<name>-<subcommand>.code` when those files exist.
fn fake_tool(dir: &Path, name: &str) {
    let body = format!(
        "#!/bin/sh\n\
         echo \"{name} $*\" >> \"{d}/argv.log\"\n\
         f=\"{d}/{name}-$1\"\n\
         if [ -f \"$f.out\" ]; then cat \"$f.out\"; fi\n\
         if [ -f \"$f.code\" ]; then exit \"$(cat \"$f.code\")\"; fi\n\
         exit 0\n",
        d = dir.display()
    );
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    set_executable(&path);
}

#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) {}

/// Set the canned exit code and stdout for `<tool>-<subcommand>`.
fn canned(dir: &Path, key: &str, code: Option<i32>, stdout: &str) {
    if let Some(code) = code {
        fs::write(dir.join(format!("{key}.code")), code.to_string()).unwrap();
    }
    fs::write(dir.join(format!("{key}.out")), stdout).unwrap();
}

/// The config for one test: repo dir in the scratch dir, fake tools first on `PATH`.
fn config(dir: &Path) -> LandingConfig {
    let mut path = OsString::from(dir);
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    LandingConfig {
        path_env: Some(path),
        ..LandingConfig::new(dir)
    }
}

fn input(evaluator_touched: bool) -> LandingInput {
    LandingInput {
        slot_id: SlotId::new("slot-1").unwrap(),
        run_hash: "run-abc".into(),
        audit: AuditStatus::Attested,
        cpu_cap: "200000 100000".into(),
        blackwall_run: RUN.into(),
        changeset_hash: "blake3-deadbeef".into(),
        base_sha: BASE.into(),
        evaluator_touched,
        cost_source: CostSource::None,
        at: 1_000,
    }
}

fn argv_log(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("argv.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn run_landing(dir: &Path, evaluator_touched: bool) -> LandingOutcome {
    fake_tool(dir, "git");
    fake_tool(dir, "blackwall");
    LandingActivity::new(config(dir))
        .land(&input(evaluator_touched))
        .unwrap()
}

#[test]
fn landing_stale_base_refuses() {
    let dir = scratch("landing_stale_base_refuses");
    canned(&dir, "git-merge-base", Some(1), "");
    canned(&dir, "git-rev-parse", None, "ffeedd0011\n");
    let outcome = run_landing(&dir, false);

    let LandingOutcome::StaleBase {
        receipt,
        development_head,
    } = outcome
    else {
        panic!("expected StaleBase, got {outcome:?}");
    };
    assert_eq!(development_head, "ffeedd0011");
    assert!(receipt.facts().stale_base);
    assert_eq!(receipt.facts().verify_exit, None);
    assert_eq!(receipt.evidence(), EvidenceTier::Reported);
    let log = argv_log(&dir);
    assert_eq!(
        log,
        [
            format!("git merge-base --is-ancestor {BASE} origin/development"),
            "git rev-parse origin/development".to_owned(),
        ],
        "a stale base must not reach blackwall"
    );
}

#[test]
fn landing_nonzero_verify_opens_no_pr() {
    let dir = scratch("landing_nonzero_verify_opens_no_pr");
    canned(&dir, "blackwall-reconcile", Some(2), "verify failed\n");
    let outcome = run_landing(&dir, false);

    let LandingOutcome::VerifyFailed(receipt) = outcome else {
        panic!("expected VerifyFailed, got {outcome:?}");
    };
    assert_eq!(receipt.facts().verify_exit, Some(2));
    assert_eq!(receipt.evidence(), EvidenceTier::Reported);
    assert_eq!(receipt.facts().branch, None);
    assert_eq!(receipt.facts().pr_number, None);
    let log = argv_log(&dir);
    assert!(
        !log.iter().any(|line| line.starts_with("blackwall pr")),
        "a failed verify must not open a PR: {log:?}"
    );
}

#[test]
fn landing_opens_pr_on_exit_zero() {
    let dir = scratch("landing_opens_pr_on_exit_zero");
    canned(
        &dir,
        "blackwall-pr",
        Some(0),
        "opened https://github.com/cerebral-work/cas-substrate/pull/57\n",
    );
    let outcome = run_landing(&dir, false);

    let LandingOutcome::Opened(receipt) = outcome else {
        panic!("expected Opened, got {outcome:?}");
    };
    assert_eq!(receipt.facts().pr_number, Some(57));
    assert_eq!(receipt.facts().branch.as_deref(), Some(BRANCH));
    assert_eq!(receipt.facts().verify_exit, Some(0));
    assert_eq!(receipt.evidence(), EvidenceTier::Verified);
    assert_eq!(
        argv_log(&dir),
        [
            format!("git merge-base --is-ancestor {BASE} origin/development"),
            format!("blackwall reconcile {RUN} --base {BASE} --branch {BRANCH}"),
            format!("blackwall pr {RUN} --base development"),
        ]
    );
}

#[test]
fn landing_evaluator_touched_caps_tier() {
    let dir = scratch("landing_evaluator_touched_caps_tier");
    canned(
        &dir,
        "blackwall-pr",
        Some(0),
        "https://github.com/cerebral-work/cas-substrate/pull/58\n",
    );
    let outcome = run_landing(&dir, true);

    let LandingOutcome::Opened(receipt) = outcome else {
        panic!("expected Opened, got {outcome:?}");
    };
    assert!(receipt.facts().evaluator_touched);
    assert_eq!(receipt.facts().verify_exit, Some(0));
    assert_eq!(receipt.evidence(), EvidenceTier::Reported);
    assert_eq!(receipt.facts().pr_number, Some(58));
}
