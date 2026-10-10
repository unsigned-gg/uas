//! Landing activity tests. Each test puts fake `git` and `blackwall` scripts first on `PATH`.
//! The scripts log their argv and return canned output, so no real repository is touched.
//! Each test's temp dir is removed when its `Scratch` guard drops, also on panic.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use uas_domain::{AuditStatus, CostSource, EvidenceTier, LandingReceipt, SlotId};
use uas_engine::{
    LandingActivity, LandingConfig, LandingError, LandingInput, LandingOutcome, ReconcileOutcome,
};

const BASE: &str = "056d7c7abcdef";
const RUN: &str = "bw-0123456789abcdef";
const BRANCH: &str = "blackwall/bw-012345678";
const VERIFY_CMD: &str = "make check JOBS=2 && make hygiene JOBS=2";

/// A temp dir for one test, removed when the guard drops.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("uas-landing-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Write `#!/bin/sh` script `name`. It appends its argv to `argv.log`, then prints
/// `<name>-<subcommand>.out` and exits with `<name>-<subcommand>.code` when those files exist.
fn fake_tool(dir: &Path, name: &str) {
    let body = format!(
        "#!/bin/sh\n\
         echo \"{name} $*\" >> \"{d}/argv.log\"\n\
         case \" $* \" in *\" --body - \"*) cat > \"{d}/{name}-$1.stdin\";; esac\n\
         f=\"{d}/{name}-$1\"\n\
         if [ -f \"$f.kill\" ]; then kill -9 $$; fi\n\
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

/// Make `<tool>-<subcommand>` kill itself with SIGKILL, so it exits with no code.
fn signaled(dir: &Path, key: &str) {
    fs::write(dir.join(format!("{key}.kill")), "").unwrap();
}

/// The repo's `blackwall.toml`, with the `[verify]` command that the repo sets.
fn write_blackwall_toml_with_verify(dir: &Path) {
    let body =
        format!("[defaults]\nprovider = \"claude\"\n\n[verify]\ncommand = \"{VERIFY_CMD}\"\n");
    fs::write(dir.join("blackwall.toml"), body).unwrap();
}

/// A `blackwall.toml` that has no `[verify]` section.
fn write_blackwall_toml_without_verify(dir: &Path) {
    fs::write(
        dir.join("blackwall.toml"),
        "[defaults]\nprovider = \"claude\"\n",
    )
    .unwrap();
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

fn try_landing(dir: &Path, input: &LandingInput) -> Result<LandingOutcome, LandingError> {
    fake_tool(dir, "git");
    fake_tool(dir, "blackwall");
    LandingActivity::new(config(dir)).land(input)
}

fn run_landing(dir: &Path, evaluator_touched: bool) -> LandingOutcome {
    try_landing(dir, &input(evaluator_touched)).unwrap()
}

/// The JSON inside the fenced block that the PR body carries (spec 0022 D11).
fn fenced_json(body: &str) -> &str {
    body.strip_prefix("```json\n")
        .and_then(|rest| rest.strip_suffix("\n```\n"))
        .unwrap_or_else(|| panic!("the PR body is not a fenced JSON block: {body:?}"))
}

#[test]
fn landing_stale_base_refuses() {
    let scratch = Scratch::new("landing_stale_base_refuses");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    canned(dir, "git-merge-base", Some(1), "");
    canned(dir, "git-rev-parse", None, "ffeedd0011\n");
    let outcome = run_landing(dir, false);

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
    assert_eq!(
        argv_log(dir),
        [
            format!("git merge-base --is-ancestor {BASE} origin/development"),
            "git rev-parse origin/development".to_owned(),
        ],
        "a stale base must not reach blackwall"
    );
}

#[test]
fn landing_stale_base_receipt_carries_development_head() {
    let scratch = Scratch::new("landing_stale_base_receipt_carries_development_head");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    canned(dir, "git-merge-base", Some(1), "");
    canned(dir, "git-rev-parse", None, "ffeedd0011\n");
    let outcome = run_landing(dir, false);

    let LandingOutcome::StaleBase {
        receipt,
        development_head,
    } = outcome
    else {
        panic!("expected StaleBase, got {outcome:?}");
    };
    assert_eq!(development_head, "ffeedd0011");
    assert_eq!(
        receipt.facts().development_head.as_deref(),
        Some(development_head.as_str()),
        "the receipt must record the head the base was checked against (spec 0022 D9)"
    );
    assert!(receipt.facts().stale_base);
}

#[test]
fn landing_nonzero_verify_opens_no_pr() {
    let scratch = Scratch::new("landing_nonzero_verify_opens_no_pr");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    // blackwall exits 3 when the verify gate fails. The soma lane reported this (spec 0022 F14, reported).
    canned(dir, "blackwall-reconcile", Some(3), "verify failed\n");
    let outcome = run_landing(dir, false);

    let LandingOutcome::VerifyFailed(receipt) = outcome else {
        panic!("expected VerifyFailed, got {outcome:?}");
    };
    assert_eq!(receipt.facts().verify_exit, Some(3));
    assert_eq!(receipt.evidence(), EvidenceTier::Reported);
    assert_eq!(receipt.facts().branch, None);
    assert_eq!(receipt.facts().pr_number, None);
    let log = argv_log(dir);
    assert!(
        !log.iter().any(|line| line.starts_with("blackwall pr")),
        "a failed reconcile must not open a PR: {log:?}"
    );
}

#[test]
fn landing_opens_pr_on_exit_zero() {
    let scratch = Scratch::new("landing_opens_pr_on_exit_zero");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    canned(
        dir,
        "blackwall-pr",
        Some(0),
        "opened https://github.com/unsigned-gg/uas/pull/57\n",
    );
    let outcome = run_landing(dir, false);

    let LandingOutcome::Opened(receipt) = outcome else {
        panic!("expected Opened, got {outcome:?}");
    };
    assert_eq!(receipt.facts().pr_number, Some(57));
    assert_eq!(receipt.facts().branch.as_deref(), Some(BRANCH));
    assert_eq!(receipt.facts().verify_exit, Some(0));
    assert_eq!(receipt.facts().development_head, None);
    assert_eq!(receipt.evidence(), EvidenceTier::Verified);
    assert_eq!(
        argv_log(dir),
        [
            format!("git merge-base --is-ancestor {BASE} origin/development"),
            format!("blackwall reconcile {RUN} --base {BASE} --branch {BRANCH}"),
            format!("blackwall pr {RUN} --base development --body -"),
        ]
    );
}

#[test]
fn landing_evaluator_touched_caps_tier() {
    let scratch = Scratch::new("landing_evaluator_touched_caps_tier");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    canned(
        dir,
        "blackwall-pr",
        Some(0),
        "https://github.com/unsigned-gg/uas/pull/58\n",
    );
    let outcome = run_landing(dir, true);

    let LandingOutcome::Opened(receipt) = outcome else {
        panic!("expected Opened, got {outcome:?}");
    };
    assert!(receipt.facts().evaluator_touched);
    assert_eq!(receipt.facts().verify_exit, Some(0));
    assert_eq!(receipt.evidence(), EvidenceTier::Reported);
    assert_eq!(receipt.facts().pr_number, Some(58));
}

#[test]
fn landing_missing_verify_is_reported() {
    // blackwall 1.2.0 exits 0 with only a warning when [verify] is absent. The custody
    // process is fail-closed, so the activity must not open a PR without a verify gate.
    let scratch = Scratch::new("landing_missing_verify_is_reported");
    let dir = scratch.path();
    write_blackwall_toml_without_verify(dir);
    canned(
        dir,
        "blackwall-pr",
        Some(0),
        "https://github.com/unsigned-gg/uas/pull/59\n",
    );
    let outcome = run_landing(dir, false);

    let LandingOutcome::NoVerifyGate(receipt) = outcome else {
        panic!("expected NoVerifyGate, got {outcome:?}");
    };
    assert_eq!(receipt.facts().verify_cmd, None);
    assert_eq!(receipt.facts().verify_exit, None);
    assert_eq!(receipt.facts().branch, None);
    assert_eq!(receipt.facts().pr_number, None);
    assert_eq!(receipt.evidence(), EvidenceTier::Reported);
    let log = argv_log(dir);
    assert!(
        !log.iter().any(|line| line.starts_with("blackwall pr")),
        "no verify gate must not open a PR: {log:?}"
    );
    assert!(
        !log.iter()
            .any(|line| line.starts_with("blackwall reconcile")),
        "no verify gate must not reconcile either (it would create a branch): {log:?}"
    );
    // The gate sits after the base check and before any blackwall call: the only
    // process the activity ran is merge-base.
    assert_eq!(
        log.iter()
            .map(|line| line
                .split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" "))
            .collect::<Vec<_>>(),
        vec!["git merge-base".to_string()],
        "exactly one merge-base call and nothing else: {log:?}"
    );
}

#[test]
fn landing_stale_base_wins_over_missing_verify() {
    // A stale base is refused before the verify gate is consulted, so the outcome
    // names the stale base, not the missing gate.
    let scratch = Scratch::new("landing_stale_base_wins_over_missing_verify");
    let dir = scratch.path();
    write_blackwall_toml_without_verify(dir);
    canned(dir, "git-merge-base", Some(1), "");
    canned(dir, "git-rev-parse", None, "ffeedd0011\n");
    let outcome = run_landing(dir, false);
    let LandingOutcome::StaleBase { receipt, .. } = outcome else {
        panic!("expected StaleBase, got {outcome:?}");
    };
    assert!(receipt.facts().stale_base);
    assert_eq!(receipt.facts().verify_cmd, None);
    let log = argv_log(dir);
    assert!(
        !log.iter().any(|line| line.starts_with("blackwall")),
        "a stale base must not reach blackwall: {log:?}"
    );
}

#[test]
fn landing_reconcile_exit_one_is_error() {
    let scratch = Scratch::new("landing_reconcile_exit_one_is_error");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    canned(dir, "blackwall-reconcile", Some(1), "reconcile failed\n");
    let result = try_landing(dir, &input(false));
    assert!(
        matches!(
            result,
            Err(LandingError::UnexpectedExit {
                step: "reconcile",
                code: Some(1),
                ..
            })
        ),
        "exit 1 is a LandingError with no receipt (spec 0022 D7): {result:?}"
    );
    let log = argv_log(dir);
    assert!(
        !log.iter().any(|line| line.starts_with("blackwall pr")),
        "a reconcile error must not open a PR: {log:?}"
    );
}

#[test]
fn landing_reconcile_exit_two_is_error() {
    let scratch = Scratch::new("landing_reconcile_exit_two_is_error");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    canned(dir, "blackwall-reconcile", Some(2), "usage error\n");
    let result = try_landing(dir, &input(false));
    assert!(
        matches!(
            result,
            Err(LandingError::UnexpectedExit {
                step: "reconcile",
                code: Some(2),
                ..
            })
        ),
        "exit 2 is a LandingError with no receipt (spec 0022 D7): {result:?}"
    );
    let log = argv_log(dir);
    assert!(
        !log.iter().any(|line| line.starts_with("blackwall pr")),
        "a usage error must not open a PR: {log:?}"
    );
}

#[test]
fn landing_pr_body_carries_receipt_on_stdin() {
    let scratch = Scratch::new("landing_pr_body_carries_receipt_on_stdin");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    canned(
        dir,
        "blackwall-pr",
        Some(0),
        "https://github.com/unsigned-gg/uas/pull/57\n",
    );
    let outcome = run_landing(dir, false);
    let LandingOutcome::Opened(receipt) = outcome else {
        panic!("expected Opened, got {outcome:?}");
    };
    let body = fs::read_to_string(dir.join("blackwall-pr.stdin")).unwrap();
    assert!(body.starts_with("```json\n"), "{body:?}");
    assert!(body.ends_with("\n```\n"), "{body:?}");
    let parsed: LandingReceipt = serde_json::from_str(fenced_json(&body)).unwrap();
    assert_eq!(parsed.facts().pr_number, None, "the PR does not exist yet");
    assert_eq!(parsed.facts().branch.as_deref(), Some(BRANCH));
    assert_eq!(parsed.facts().verify_exit, Some(0));
    assert_eq!(parsed.evidence(), EvidenceTier::Verified);
    assert_eq!(parsed.evidence(), receipt.evidence());
    assert_eq!(receipt.facts().pr_number, Some(57));
}

#[test]
fn landing_pr_body_shows_unattested_dropped_count() {
    let scratch = Scratch::new("landing_pr_body_shows_unattested_dropped_count");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    canned(
        dir,
        "blackwall-pr",
        Some(0),
        "https://github.com/unsigned-gg/uas/pull/60\n",
    );
    let mut unattested = input(false);
    unattested.audit = AuditStatus::Unattested { dropped_events: 3 };
    try_landing(dir, &unattested).unwrap();
    let body = fs::read_to_string(dir.join("blackwall-pr.stdin")).unwrap();
    assert!(
        body.contains("\"dropped_events\": 3"),
        "the raw PR body must show the dropped count: {body}"
    );
    let parsed: LandingReceipt = serde_json::from_str(fenced_json(&body)).unwrap();
    assert_eq!(
        parsed.facts().audit,
        AuditStatus::Unattested { dropped_events: 3 }
    );
}

#[test]
fn landing_reconcile_killed_by_signal_is_error() {
    let scratch = Scratch::new("landing_reconcile_killed_by_signal_is_error");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    signaled(dir, "blackwall-reconcile");
    let result = try_landing(dir, &input(false));
    assert!(
        matches!(
            result,
            Err(LandingError::UnexpectedExit {
                step: "reconcile",
                code: None,
                ..
            })
        ),
        "a signal is a LandingError with no receipt (spec 0022 D7): {result:?}"
    );
    let log = argv_log(dir);
    assert!(
        !log.iter().any(|line| line.starts_with("blackwall pr")),
        "a killed reconcile must not open a PR: {log:?}"
    );
}

#[test]
fn landing_reconcile_exit_four_is_error() {
    let scratch = Scratch::new("landing_reconcile_exit_four_is_error");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    canned(dir, "blackwall-reconcile", Some(4), "reconcile error\n");
    let result = try_landing(dir, &input(false));
    assert!(
        matches!(
            result,
            Err(LandingError::UnexpectedExit {
                step: "reconcile",
                code: Some(4),
                ..
            })
        ),
        "only exit 3 is the verify gate; exit 4 is a LandingError (spec 0022 D7): {result:?}"
    );
    let log = argv_log(dir);
    assert!(
        !log.iter().any(|line| line.starts_with("blackwall pr")),
        "exit 4 must not open a PR: {log:?}"
    );
}

#[test]
fn landing_pr_exit_three_is_error() {
    let scratch = Scratch::new("landing_pr_exit_three_is_error");
    let dir = scratch.path();
    write_blackwall_toml_with_verify(dir);
    canned(
        dir,
        "blackwall-pr",
        Some(3),
        "https://github.com/unsigned-gg/uas/pull/61\n",
    );
    let result = try_landing(dir, &input(false));
    assert!(
        matches!(
            result,
            Err(LandingError::UnexpectedExit {
                step: "pr",
                code: Some(3),
                ..
            })
        ),
        "exit 3 is the verify gate for reconcile only; pr exit 3 is a LandingError (spec 0022 D7): {result:?}"
    );
}

#[test]
fn landing_reconcile_then_open_pr_matches_land() {
    let whole = Scratch::new("landing_reconcile_then_open_pr_matches_land_whole");
    write_blackwall_toml_with_verify(whole.path());
    canned(
        whole.path(),
        "blackwall-pr",
        Some(0),
        "opened https://github.com/unsigned-gg/uas/pull/57\n",
    );
    let LandingOutcome::Opened(via_land) = run_landing(whole.path(), false) else {
        panic!("land did not open a PR");
    };

    let split = Scratch::new("landing_reconcile_then_open_pr_matches_land_split");
    write_blackwall_toml_with_verify(split.path());
    canned(
        split.path(),
        "blackwall-pr",
        Some(0),
        "opened https://github.com/unsigned-gg/uas/pull/57\n",
    );
    fake_tool(split.path(), "git");
    fake_tool(split.path(), "blackwall");
    let activity = LandingActivity::new(config(split.path()));
    let ReconcileOutcome::Reconciled(reconciled) = activity.reconcile(&input(false)).unwrap()
    else {
        panic!("reconcile did not return Reconciled");
    };
    assert_eq!(reconciled.facts().pr_number, None, "reconcile opens no PR");
    let opened = activity.open_pr(&reconciled).unwrap();

    assert_eq!(argv_log(split.path()), argv_log(whole.path()));
    let want = fs::read(whole.path().join("blackwall-pr.stdin")).unwrap();
    let got = fs::read(split.path().join("blackwall-pr.stdin")).unwrap();
    assert_eq!(got, want, "the PR body bytes differ");
    assert_eq!(opened, via_land);
}
