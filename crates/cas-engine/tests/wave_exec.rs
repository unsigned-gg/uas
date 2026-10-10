//! Wave executor tests. Every test puts fake `git`, `blackwall`, `claude` and `make` scripts
//! first on the child `PATH`. The scripts log their argv and return canned output, so no real
//! repository, Blackwall run, saga flow or model is touched. Each test's temp dir is removed when
//! its `Env` guard drops, also on panic.

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use cas_domain::{AuditStatus, EvidenceTier, LandingReceipt};
use cas_engine::{
    parse_critic_verdict, ActivityKind, AdmitError, CriticVerdict, HaltReason, NotReady, SagaStep,
    StepOutcome, WaveError, WaveExecConfig, WaveExecutor, WaveOutcome, WavePlan, WorkflowEvent,
};

const BASE: &str = "056d7c7abcdef";
const HEAD: &str = "f00dfeed0001";
const RUN_A: &str = "bwrun0000001";
const CHG_A: &str = "chg000000001";
const START_A: &str = "\u{2713} run haiku (ok): hash bwrun0000001 (new)\n\u{2713} changeset: chg000000001 (1 entries)\n";
const START_B: &str = "\u{2713} run haiku (ok): hash bwrun0000002 (new)\n\u{2713} changeset: chg000000002 (1 entries)\n";
const PR_OUT: &[u8] = b"opened https://github.com/cerebral-work/cas-substrate/pull/57\n";

/// One canned answer. The first rule whose pattern matches `$*` wins.
struct Rule {
    pat: &'static str,
    out: &'static [u8],
    err: &'static str,
    code: i32,
}

fn r(pat: &'static str, out: &'static [u8], err: &'static str, code: i32) -> Rule {
    Rule {
        pat,
        out,
        err,
        code,
    }
}

/// The default answers of one fake tool. A test puts its own rules before these.
fn defaults(name: &str) -> Vec<Rule> {
    match name {
        "git" => vec![
            r("rev-parse HEAD", b"056d7c7abcdef\n", "", 0),
            r("rev-parse blackwall/*", b"f00dfeed0001\n", "", 0),
            r("merge-base *", b"", "", 0),
            r("diff --name-only *", b"", "", 0),
            r("ls-remote *", b"", "", 2),
            r("branch -D *", b"", "", 0),
            r("*", b"", "", 0),
        ],
        "blackwall" => vec![
            r("start --task build-a *", b"", START_A, 0),
            r("start --task build-b *", b"", START_B, 0),
            r("settle *", b"", "", 0),
            r("reconcile *", b"", "", 0),
            r("pr *", PR_OUT, "", 0),
            r("*", b"", "", 0),
        ],
        "claude" => vec![r("*", b"VERDICT: NO DEFECTS\nreviewed\n", "", 0)],
        _ => vec![r("*", b"", "", 0)],
    }
}

/// A `case` pattern: each literal part in single quotes, each `*` a glob.
fn shell_pattern(pat: &str) -> String {
    pat.split('*')
        .map(|part| format!("'{part}'"))
        .collect::<Vec<_>>()
        .join("*")
}

/// A temp dir with a repo checkout, a state dir, a scratch dir and fake tools. Removed on drop.
struct Env {
    root: PathBuf,
    bin: PathBuf,
    log: PathBuf,
    repo: PathBuf,
    state: PathBuf,
    wt: PathBuf,
    lock: PathBuf,
}

impl Env {
    fn new(test: &str) -> Self {
        let root = std::env::temp_dir().join(format!("cas-wave-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let env = Self {
            bin: root.join("bin"),
            log: root.join("argv.log"),
            repo: root.join("repo"),
            state: root.join("state"),
            wt: root.join("wt"),
            lock: root.join("lock").join("wave.lock"),
            root,
        };
        fs::create_dir_all(&env.bin).unwrap();
        fs::create_dir_all(env.repo.join(".blackwall")).unwrap();
        fs::write(env.repo.join(".blackwall/init-head"), format!("{BASE}\n")).unwrap();
        fs::write(
            env.repo.join("blackwall.toml"),
            "[verify]\ncommand = \"make check JOBS=2\"\n",
        )
        .unwrap();
        for id in ["build-a", "build-b"] {
            env.task(id, Some("haiku"));
        }
        for name in ["git", "blackwall", "claude", "make", "gh"] {
            env.tool(name, Vec::new());
        }
        env
    }

    /// Write `tasks/<id>/prompt.md` and a `task.toml` with `model`, or none when `None`.
    fn task(&self, id: &str, model: Option<&str>) {
        let dir = self.repo.join("tasks").join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("prompt.md"), format!("Build {id}.\n")).unwrap();
        let toml = match model {
            Some(m) => format!("model = \"{m}\"\n"),
            None => "budget = 1\n".to_owned(),
        };
        fs::write(dir.join("task.toml"), toml).unwrap();
    }

    fn remove(&self, rel: &str) {
        let path = self.repo.join(rel);
        let _ = fs::remove_file(path);
    }

    /// Rewrite one fake tool. `custom` rules go first, then the defaults.
    fn tool(&self, name: &str, custom: Vec<Rule>) {
        self.tool_pre(name, "", custom);
    }

    /// Rewrite one fake tool with a shell line `extra` that runs on every call. The lock probe
    /// runs too: `reconcile`, `claude` and `make` log `LOCK NOT HELD` when the host lock is not
    /// held, because `flock -n` succeeds only on a free lock.
    fn tool_pre(&self, name: &str, extra: &str, custom: Vec<Rule>) {
        let mut rules = custom;
        rules.extend(defaults(name));
        let probe = format!(
            "if flock -n '{lock}' true; then echo 'LOCK NOT HELD' >> '{log}'; fi",
            lock = self.lock.display(),
            log = self.log.display()
        );
        let guard = match name {
            "claude" | "make" => format!("{probe}\n"),
            "blackwall" => format!("case \"$1\" in reconcile) {probe} ;; esac\n"),
            _ => String::new(),
        };
        let mut body = format!(
            "#!/bin/sh\necho \"{name} $*\" >> \"{}\"\n",
            self.log.display()
        );
        body += &guard;
        body += extra;
        body += "\n";
        if name == "claude" {
            body += &format!("cat > \"{}/claude.stdin\"\n", self.bin.display());
        }
        body += &format!(
            "case \"$*\" in *\"--body -\"*) cat > \"{}/{name}.body\" ;; esac\n",
            self.bin.display()
        );
        if name == "git" {
            body += "if [ \"$1\" = worktree ] && [ \"$2\" = add ]; then mkdir -p \"$4\"; fi\n";
        }
        body += "case \"$*\" in\n";
        for (i, rule) in rules.iter().enumerate() {
            let out = self.bin.join(format!("{name}.{i}.out"));
            let err = self.bin.join(format!("{name}.{i}.err"));
            fs::write(&out, rule.out).unwrap();
            fs::write(&err, rule.err).unwrap();
            body += &format!(
                "  {pat}) cat \"{out}\"; cat \"{err}\" >&2; exit {code} ;;\n",
                pat = shell_pattern(rule.pat),
                out = out.display(),
                err = err.display(),
                code = rule.code
            );
        }
        body += "  *) exit 0 ;;\nesac\n";
        let path = self.bin.join(name);
        fs::write(&path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn config(&self) -> WaveExecConfig {
        // The fakes shadow every tool the executor calls. The developer PATH is not reachable.
        let mut path = OsString::from(&self.bin);
        path.push(":/usr/bin:/bin");
        WaveExecConfig {
            path_env: Some(path),
            ..WaveExecConfig::new(&self.repo, &self.state, &self.wt, &self.lock)
        }
    }

    fn argv(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn step(
    id: &str,
    agent: Option<&str>,
    kind: ActivityKind,
    deps: &[&str],
    verify: &[&str],
) -> SagaStep {
    SagaStep {
        id: id.into(),
        title: id.into(),
        kind,
        deps: deps.iter().map(|d| (*d).to_owned()).collect(),
        verify_cmd: verify.iter().map(|v| (*v).to_owned()).collect(),
        gate: None,
        agent: agent.map(str::to_owned),
        duration: None,
    }
}

fn builder(id: &str) -> SagaStep {
    step(id, Some("builder"), ActivityKind::Impl, &[], &[])
}

fn critic(id: &str, on: &str) -> SagaStep {
    step(id, Some("critic"), ActivityKind::Verify, &[on], &[])
}

fn probe(id: &str) -> SagaStep {
    step(
        id,
        Some("probe"),
        ActivityKind::Verify,
        &[],
        &["make", "check"],
    )
}

fn executor(env: &Env, steps: &[SagaStep]) -> WaveExecutor {
    WaveExecutor::new(env.config(), WavePlan::plan(steps, 2).unwrap())
}

fn with_open_prs(env: &Env, steps: &[SagaStep]) -> WaveExecutor {
    let mut config = env.config();
    config.open_prs = true;
    WaveExecutor::new(config, WavePlan::plan(steps, 2).unwrap())
}

fn has_line(argv: &[String], line: &str) -> bool {
    argv.iter().any(|l| l == line)
}

// Admission.

#[test]
fn wave_admit_refuses_builder_without_critic() {
    let env = Env::new("wave_admit_refuses_builder_without_critic");
    let steps = [builder("build-a")];
    assert_eq!(
        executor(&env, &steps).admit(),
        Err(AdmitError::BuilderCriticCount {
            step: "build-a".into(),
            critics: 0
        })
    );
}

#[test]
fn wave_admit_refuses_builder_with_two_critics() {
    let env = Env::new("wave_admit_refuses_builder_with_two_critics");
    let steps = [
        builder("build-a"),
        critic("review-a", "build-a"),
        critic("review-b", "build-a"),
    ];
    assert_eq!(
        executor(&env, &steps).admit(),
        Err(AdmitError::BuilderCriticCount {
            step: "build-a".into(),
            critics: 2
        })
    );
}

#[test]
fn wave_admit_refuses_missing_task_prompt() {
    let env = Env::new("wave_admit_refuses_missing_task_prompt");
    env.remove("tasks/build-a/prompt.md");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    assert_eq!(
        executor(&env, &steps).admit(),
        Err(AdmitError::MissingTaskPrompt("build-a".into()))
    );
}

#[test]
fn wave_admit_refuses_task_model_absent() {
    let env = Env::new("wave_admit_refuses_task_model_absent");
    env.task("build-a", None);
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    assert_eq!(
        executor(&env, &steps).admit(),
        Err(AdmitError::TaskModelAbsent("build-a".into()))
    );
}

#[test]
fn wave_admit_refuses_task_model_mismatch() {
    let env = Env::new("wave_admit_refuses_task_model_mismatch");
    env.task("build-a", Some("sonnet"));
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    assert_eq!(
        executor(&env, &steps).admit(),
        Err(AdmitError::TaskModelMismatch {
            step: "build-a".into(),
            found: "sonnet".into()
        })
    );
}

#[test]
fn wave_admit_refuses_probe_without_verify_cmd() {
    let env = Env::new("wave_admit_refuses_probe_without_verify_cmd");
    let steps = [step(
        "probe-x",
        Some("probe"),
        ActivityKind::Verify,
        &[],
        &[],
    )];
    assert_eq!(
        executor(&env, &steps).admit(),
        Err(AdmitError::ProbeWithoutVerifyCmd("probe-x".into()))
    );
}

#[test]
fn wave_admit_refuses_probe_after_builder() {
    let env = Env::new("wave_admit_refuses_probe_after_builder");
    let steps = [
        builder("build-a"),
        critic("review-a", "build-a"),
        step(
            "probe-x",
            Some("probe"),
            ActivityKind::Verify,
            &["build-a"],
            &["make", "check"],
        ),
    ];
    assert_eq!(
        executor(&env, &steps).admit(),
        Err(AdmitError::ProbeAfterBuilder("probe-x".into()))
    );
}

#[test]
fn wave_admit_refuses_bad_slot_id() {
    let env = Env::new("wave_admit_refuses_bad_slot_id");
    let steps = [builder("bad id"), critic("review-a", "bad id")];
    assert_eq!(
        executor(&env, &steps).admit(),
        Err(AdmitError::BadSlotId("bad id".into()))
    );
}

#[test]
fn wave_admit_refuses_before_any_subprocess() {
    let env = Env::new("wave_admit_refuses_before_any_subprocess");
    let steps = [builder("build-a")];
    let err = executor(&env, &steps).run_local(1).unwrap_err();
    assert!(
        matches!(err, WaveError::Admit(AdmitError::BuilderCriticCount { .. })),
        "{err}"
    );
    assert!(env.argv().is_empty(), "{:?}", env.argv());
}

// Readiness.

#[test]
fn wave_not_ready_no_init_head() {
    let env = Env::new("wave_not_ready_no_init_head");
    env.remove(".blackwall/init-head");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let got = executor(&env, &steps).run_step("build-a", 1).unwrap();
    assert_eq!(got.outcome, StepOutcome::NotReady(NotReady::NoInitHead));
    assert!(got.event.is_none());
    assert!(env.argv().is_empty(), "{:?}", env.argv());
    assert!(!env.state.exists(), "a not-ready step wrote a record");
}

#[test]
fn wave_not_ready_head_differs() {
    let env = Env::new("wave_not_ready_head_differs");
    env.tool("git", vec![r("rev-parse HEAD", b"0000000000001\n", "", 0)]);
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let got = executor(&env, &steps).run_step("build-a", 1).unwrap();
    assert_eq!(got.outcome, StepOutcome::NotReady(NotReady::HeadDiffers));
    assert!(!env.argv().iter().any(|l| l.starts_with("blackwall start")));
}

#[test]
fn wave_not_ready_dep_record_missing() {
    let env = Env::new("wave_not_ready_dep_record_missing");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let got = executor(&env, &steps).run_step("review-a", 1).unwrap();
    assert_eq!(
        got.outcome,
        StepOutcome::NotReady(NotReady::DepRecordMissing("build-a".into()))
    );
}

#[test]
fn wave_not_ready_upstream_not_merged() {
    let env = Env::new("wave_not_ready_upstream_not_merged");
    // The upstream head is not an ancestor of development. The base check still passes.
    env.tool(
        "git",
        vec![r("merge-base --is-ancestor f00dfeed0001 *", b"", "", 1)],
    );
    let steps = [
        builder("build-a"),
        critic("review-a", "build-a"),
        step(
            "build-b",
            Some("builder"),
            ActivityKind::Impl,
            &["build-a"],
            &[],
        ),
        critic("review-b", "build-b"),
    ];
    let exec = executor(&env, &steps);
    assert_eq!(
        exec.run_step("build-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    // Case 1: a Builder whose upstream Builder is unmerged is not ready.
    assert_eq!(
        exec.run_step("build-b", 1).unwrap().outcome,
        StepOutcome::NotReady(NotReady::UpstreamNotMerged("build-a".into()))
    );
    // Case 2: a Critic whose own Builder passed but is unmerged is ready: claude runs.
    assert_eq!(
        exec.run_step("review-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    assert!(env.argv().iter().any(|l| l.starts_with("claude -p")));
}

// Builder.

#[test]
fn wave_builder_argv_exact() {
    let env = Env::new("wave_builder_argv_exact");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let got = executor(&env, &steps).run_step("build-a", 1).unwrap();
    assert_eq!(got.outcome, StepOutcome::Passed);
    let argv = env.argv();
    assert!(
        has_line(
            &argv,
            "blackwall start --task build-a --provider claude --model haiku"
        ),
        "{argv:?}"
    );
    assert!(
        has_line(&argv, "blackwall settle bwrun0000001 select"),
        "{argv:?}"
    );
    assert!(
        has_line(
            &argv,
            &format!("git diff --name-only -z --no-renames {BASE} {HEAD}")
        ),
        "{argv:?}"
    );
    assert!(
        !argv.iter().any(|l| l.starts_with("blackwall pr")),
        "{argv:?}"
    );
}

#[test]
fn wave_builder_fails_without_ok_line() {
    let env = Env::new("wave_builder_fails_without_ok_line");
    env.tool(
        "blackwall",
        vec![r(
            "start --task build-a *",
            b"",
            "\u{2713} run haiku (err): hash bwrun0000001 (new)\n",
            0,
        )],
    );
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let got = executor(&env, &steps).run_step("build-a", 1).unwrap();
    assert_eq!(
        got.outcome,
        StepOutcome::Halted(HaltReason::BuilderRunFailed)
    );
}

#[test]
fn wave_verify_failed_halts() {
    let env = Env::new("wave_verify_failed_halts");
    env.tool("blackwall", vec![r("reconcile *", b"", "", 3)]);
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let got = executor(&env, &steps).run_step("build-a", 1).unwrap();
    assert_eq!(got.outcome, StepOutcome::Halted(HaltReason::VerifyFailed));
    assert!(!env.argv().iter().any(|l| l.starts_with("claude")));
}

#[test]
fn wave_evaluator_touched_halts_before_critic() {
    let cases = [
        "Makefile",
        ".claude/agents/x.md",
        "CLAUDE.md",
        "crates/x/AGENTS.md",
        ".cargo/config.toml",
        ".mcp.json",
        "crates/x/Cargo.toml",
        "crates/cas-ebpf/build.rs",
        "crates/cas-ebpf/ebpf/cas-ebpf.o",
        "typos.toml",
    ];
    for (i, path) in cases.iter().enumerate() {
        let env = Env::new(&format!("wave_evaluator_touched_halts_before_critic_{i}"));
        let stdout: &'static [u8] = Box::leak(format!("{path}\0").into_bytes().into_boxed_slice());
        env.tool("git", vec![r("diff --name-only *", stdout, "", 0)]);
        let steps = [builder("build-a"), critic("review-a", "build-a")];
        let got = executor(&env, &steps).run_step("build-a", 1).unwrap();
        assert_eq!(
            got.outcome,
            StepOutcome::Halted(HaltReason::EvaluatorTouched),
            "{path}"
        );
        assert!(
            !env.argv().iter().any(|l| l.starts_with("claude")),
            "{path}"
        );
        let receipt = got.receipt.expect("the halt carries the tiered receipt");
        assert_eq!(receipt.evidence(), EvidenceTier::Reported, "{path}");
        assert!(receipt.facts().evaluator_touched, "{path}");
    }
}

#[test]
fn wave_evaluator_diff_non_ascii_add_halts() {
    // The path as git 2.43 prints it with -z: raw bytes, no C-quoting.
    let env = Env::new("wave_evaluator_diff_non_ascii_add_halts");
    env.tool(
        "git",
        vec![r(
            "diff --name-only *",
            b".github/workflows/c\xc3\xa9.yml\0",
            "",
            0,
        )],
    );
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let got = executor(&env, &steps).run_step("build-a", 1).unwrap();
    assert_eq!(
        got.outcome,
        StepOutcome::Halted(HaltReason::EvaluatorTouched)
    );

    // A second entry before the evaluator path: a split on newline reads it as one name.
    let env2 = Env::new("wave_evaluator_diff_non_ascii_add_halts_second");
    env2.tool(
        "git",
        vec![r(
            "diff --name-only *",
            b"README.md\0.github/workflows/c\xc3\xa9.yml\0",
            "",
            0,
        )],
    );
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let got = executor(&env2, &steps).run_step("build-a", 1).unwrap();
    assert_eq!(
        got.outcome,
        StepOutcome::Halted(HaltReason::EvaluatorTouched)
    );
}

#[test]
fn wave_evaluator_diff_rename_out_halts() {
    // `--no-renames` lists the removed Makefile as well as the new build.mk.
    let env = Env::new("wave_evaluator_diff_rename_out_halts");
    env.tool(
        "git",
        vec![r("diff --name-only *", b"Makefile\0build.mk\0", "", 0)],
    );
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let got = executor(&env, &steps).run_step("build-a", 1).unwrap();
    assert_eq!(
        got.outcome,
        StepOutcome::Halted(HaltReason::EvaluatorTouched)
    );
}

// Critic.

#[test]
fn wave_critic_prompt_on_stdin() {
    let env = Env::new("wave_critic_prompt_on_stdin");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = executor(&env, &steps);
    exec.run_step("build-a", 1).unwrap();
    assert_eq!(
        exec.run_step("review-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    let argv = env.argv();
    let claude = argv
        .iter()
        .find(|l| l.starts_with("claude "))
        .expect("claude ran");
    assert!(!claude.contains("Review the Builder step"), "{claude}");
    let stdin = fs::read_to_string(env.bin.join("claude.stdin")).unwrap();
    assert!(
        stdin.contains("Review the Builder step build-a as the critic step review-a"),
        "{stdin}"
    );
}

#[test]
fn wave_critic_no_defects_opens_pr() {
    let env = Env::new("wave_critic_no_defects_opens_pr");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = with_open_prs(&env, &steps);
    exec.run_step("build-a", 1).unwrap();
    assert_eq!(
        exec.run_step("review-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    let argv = env.argv();
    let claude = argv
        .iter()
        .find(|l| l.starts_with("claude "))
        .expect("claude ran");
    assert_eq!(
        claude,
        "claude -p --strict-mcp-config --agent cas-critic --allowed-tools Read Grep Glob Bash(git diff *) Bash(git log *) Bash(git show *) Bash(make *) Bash(cargo *)"
    );
    // No gh call and no git push: the executor never merges or pushes (spec 0025 D15).
    assert!(!argv.iter().any(|l| l.starts_with("gh ")), "{argv:?}");
    assert!(!argv.iter().any(|l| l.starts_with("git push")), "{argv:?}");
    assert!(
        has_line(
            &argv,
            &format!("git checkout --no-overlay {BASE} -- .claude")
        ),
        "{argv:?}"
    );
    assert!(
        argv.iter()
            .any(|l| l.starts_with("git worktree remove --force")),
        "{argv:?}"
    );
    assert!(
        has_line(
            &argv,
            "blackwall pr bwrun0000001 --base development --body -"
        ),
        "{argv:?}"
    );

    let body = fs::read_to_string(env.bin.join("blackwall.body")).unwrap();
    let json = body
        .split("```json\n")
        .nth(1)
        .and_then(|s| s.split("\n```").next())
        .expect("fenced JSON");
    let receipt: LandingReceipt = serde_json::from_str(json).unwrap();
    assert_eq!(receipt.facts().audit, AuditStatus::NotAudited);
    assert_eq!(
        receipt.facts().branch.as_deref(),
        Some("blackwall/bwrun0000001")
    );
    assert_eq!(receipt.facts().pr_number, None);
}

#[test]
fn wave_critic_pass_no_pr_when_off() {
    let env = Env::new("wave_critic_pass_no_pr_when_off");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = executor(&env, &steps);
    exec.run_step("build-a", 1).unwrap();
    assert_eq!(
        exec.run_step("review-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    assert!(!env.argv().iter().any(|l| l.starts_with("blackwall pr")));
    assert!(!env.bin.join("blackwall.body").exists());
}

#[test]
fn wave_critic_defects_halts_and_keeps_run() {
    let env = Env::new("wave_critic_defects_halts_and_keeps_run");
    env.tool(
        "claude",
        vec![r("*", b"VERDICT: DEFECTS\nthe test is missing\n", "", 0)],
    );
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = with_open_prs(&env, &steps);
    exec.run_step("build-a", 1).unwrap();
    let got = exec.run_step("review-a", 1).unwrap();
    assert_eq!(got.outcome, StepOutcome::Halted(HaltReason::CriticDefects));
    let argv = env.argv();
    assert!(
        !argv.iter().any(|l| l.starts_with("blackwall pr")),
        "{argv:?}"
    );
    // The run stays for inspection: no discard, release or settle other than select.
    assert!(
        !argv
            .iter()
            .any(|l| l.contains("discard") || l.contains("release")),
        "{argv:?}"
    );
    // The halt compensates the passed Builder: its unpushed branch is deleted.
    assert_eq!(got.compensation.len(), 1);
    assert!(got.compensation[0].success);
}

#[test]
fn wave_critic_unparsed_halts() {
    let env = Env::new("wave_critic_unparsed_halts");
    let mut text = String::from("no verdict on this line\n");
    for i in 0..15 {
        text.push_str(&format!("line {i}\n"));
    }
    let out: &'static [u8] = Box::leak(text.into_bytes().into_boxed_slice());
    env.tool("claude", vec![r("*", out, "", 0)]);
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = executor(&env, &steps);
    exec.run_step("build-a", 1).unwrap();
    assert_eq!(
        exec.run_step("review-a", 1).unwrap().outcome,
        StepOutcome::Halted(HaltReason::CriticUnparsed)
    );
}

#[test]
fn wave_critic_verdict_parses_only_the_first_line() {
    assert_eq!(
        parse_critic_verdict("VERDICT: NO DEFECTS\nok\n", Some(0)),
        CriticVerdict::NoDefects
    );
    assert_eq!(
        parse_critic_verdict("VERDICT: DEFECTS\nbad\n", Some(0)),
        CriticVerdict::Defects
    );
    assert_eq!(
        parse_critic_verdict("VERDICT: NO DEFECTS\n", Some(1)),
        CriticVerdict::Unparsed
    );
    assert_eq!(
        parse_critic_verdict("ok\nVERDICT: NO DEFECTS\n", Some(0)),
        CriticVerdict::Unparsed
    );
    assert_eq!(
        parse_critic_verdict("VERDICT: NO DEFECTS\n", None),
        CriticVerdict::Unparsed
    );
}

// Halt record and compensation.

#[test]
fn wave_halted_record_blocks_every_step() {
    let env = Env::new("wave_halted_record_blocks_every_step");
    env.tool("blackwall", vec![r("reconcile *", b"", "", 3)]);
    let steps = [
        builder("build-a"),
        critic("review-a", "build-a"),
        builder("build-b"),
        critic("review-b", "build-b"),
    ];
    let exec = executor(&env, &steps);
    assert_eq!(
        exec.run_step("build-a", 1).unwrap().outcome,
        StepOutcome::Halted(HaltReason::VerifyFailed)
    );
    let before = env.argv().len();
    for id in ["build-a", "review-a", "build-b", "review-b"] {
        assert_eq!(
            exec.run_step(id, 2).unwrap().outcome,
            StepOutcome::Halted(HaltReason::WaveHalted),
            "{id}"
        );
    }
    assert_eq!(env.argv().len(), before, "a blocked step ran a subprocess");
}

/// A plan that passes one Builder and its Critic, then halts on a failing Probe.
fn halting_plan() -> Vec<SagaStep> {
    vec![
        builder("build-a"),
        critic("review-a", "build-a"),
        probe("probe-x"),
    ]
}

#[test]
fn wave_compensate_deletes_unpushed() {
    let env = Env::new("wave_compensate_deletes_unpushed");
    env.tool("make", vec![r("*", b"", "check failed\n", 1)]);
    let report = executor(&env, &halting_plan()).run_local(1).unwrap();
    assert_eq!(report.halted, Some(HaltReason::ProbeFailed));
    assert_eq!(report.compensation.len(), 1);
    assert!(
        report.compensation[0].success,
        "{:?}",
        report.compensation[0]
    );
    assert!(
        has_line(&env.argv(), "git branch -D blackwall/bwrun0000001"),
        "{:?}",
        env.argv()
    );
}

#[test]
fn wave_compensate_keeps_pushed_without_pr() {
    let env = Env::new("wave_compensate_keeps_pushed_without_pr");
    env.tool("make", vec![r("*", b"", "", 1)]);
    env.tool("git", vec![r("ls-remote *", b"", "", 0)]);
    let report = executor(&env, &halting_plan()).run_local(1).unwrap();
    assert_eq!(report.compensation.len(), 1);
    assert!(report.compensation[0].success);
    assert!(!env.argv().iter().any(|l| l.starts_with("git branch -D")));
}

#[test]
fn wave_compensate_keeps_on_ls_remote_error() {
    let env = Env::new("wave_compensate_keeps_on_ls_remote_error");
    env.tool("make", vec![r("*", b"", "", 1)]);
    env.tool(
        "git",
        vec![r("ls-remote *", b"", "fatal: no remote\n", 128)],
    );
    let report = executor(&env, &halting_plan()).run_local(1).unwrap();
    assert_eq!(report.compensation.len(), 1);
    assert!(
        !report.compensation[0].success,
        "{:?}",
        report.compensation[0]
    );
    assert!(!env.argv().iter().any(|l| l.starts_with("git branch -D")));
}

#[test]
fn wave_compensate_continues_after_failure() {
    let env = Env::new("wave_compensate_continues_after_failure");
    env.tool("make", vec![r("*", b"", "", 1)]);
    env.tool(
        "git",
        vec![r("branch -D blackwall/bwrun0000002", b"", "locked\n", 1)],
    );
    let steps = [
        builder("build-a"),
        critic("review-a", "build-a"),
        builder("build-b"),
        critic("review-b", "build-b"),
        probe("probe-x"),
    ];
    let report = executor(&env, &steps).run_local(1).unwrap();
    assert_eq!(report.halted, Some(HaltReason::ProbeFailed));
    // Newest first: build-b, then build-a. The failure on build-b does not stop build-a.
    assert_eq!(report.compensation.len(), 2, "{:?}", report.compensation);
    assert!(!report.compensation[0].success);
    assert!(report.compensation[0].details.contains("locked"));
    assert!(report.compensation[1].success);
    assert!(
        has_line(&env.argv(), "git branch -D blackwall/bwrun0000001"),
        "{:?}",
        env.argv()
    );
}

// Probe and gate.

#[test]
fn wave_main_session_awaits_gate() {
    let env = Env::new("wave_main_session_awaits_gate");
    let mut gate = step("gate-x", None, ActivityKind::Gate, &["review-a"], &[]);
    gate.gate = Some("heimdal:approval".into());
    let steps = [builder("build-a"), critic("review-a", "build-a"), gate];
    let report = executor(&env, &steps).run_local(1).unwrap();
    assert_eq!(report.awaiting_gate.as_deref(), Some("gate-x"));
    assert!(report.halted.is_none());
    assert!(!env.argv().iter().any(|l| l.contains("saga")));
}

// Events.

#[test]
fn wave_events_carry_custody_facts() {
    let env = Env::new("wave_events_carry_custody_facts");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let report = executor(&env, &steps).run_local(7).unwrap();
    assert_eq!(report.events.len(), 2, "{:?}", report.events);
    match &report.events[0] {
        WorkflowEvent::WaveStep {
            sequence,
            step,
            role,
            persona,
            outcome,
            run,
            changeset,
            base,
            branch,
            head,
            timestamp,
            ..
        } => {
            assert_eq!(*sequence, 0);
            assert_eq!(step, "build-a");
            assert_eq!(*role, cas_engine::AgentRole::Builder);
            assert_eq!(persona.as_deref(), Some("cas-builder"));
            assert_eq!(*outcome, WaveOutcome::Passed);
            assert_eq!(run.as_deref(), Some(RUN_A));
            assert_eq!(changeset.as_deref(), Some(CHG_A));
            assert_eq!(base.as_deref(), Some(BASE));
            assert_eq!(branch.as_deref(), Some("blackwall/bwrun0000001"));
            assert_eq!(head.as_deref(), Some(HEAD));
            assert_eq!(*timestamp, 7);
        }
        other => panic!("not a wave step: {other:?}"),
    }
    match &report.events[1] {
        WorkflowEvent::WaveStep {
            sequence, persona, ..
        } => {
            assert_eq!(*sequence, 1);
            assert_eq!(persona.as_deref(), Some("cas-critic"));
        }
        other => panic!("not a wave step: {other:?}"),
    }
    // The step records carry the reconciled receipt, which the wave report also returns.
    assert_eq!(report.receipts.len(), 1);
    assert_eq!(
        report.receipts[0].facts().branch.as_deref(),
        Some("blackwall/bwrun0000001")
    );
}

#[test]
fn wave_admit_refuses_run_step_before_any_subprocess() {
    let env = Env::new("wave_admit_refuses_run_step_before_any_subprocess");
    let steps = [builder("build-a")];
    let err = executor(&env, &steps).run_step("build-a", 1).unwrap_err();
    assert!(
        matches!(err, WaveError::Admit(AdmitError::BuilderCriticCount { .. })),
        "{err}"
    );
    assert!(env.argv().is_empty(), "{:?}", env.argv());
}

#[test]
fn wave_halted_record_after_settle_halts_builder() {
    let env = Env::new("wave_halted_record_after_settle_halts_builder");
    let state = env.state.display().to_string();
    let record = r#"{"step":"other-step","role":"Builder","outcome":{"result":"halted","reason":"VerifyFailed"},"at":1,"seq":0}"#;
    env.tool_pre(
        "blackwall",
        &format!(
            "case \"$1\" in settle) mkdir -p '{state}'; printf '%s' '{record}' > '{state}/other-step.json' ;; esac"
        ),
        Vec::new(),
    );
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let got = executor(&env, &steps).run_step("build-a", 1).unwrap();
    assert_eq!(got.outcome, StepOutcome::Halted(HaltReason::WaveHalted));
    let argv = env.argv();
    assert!(
        has_line(&argv, "blackwall settle bwrun0000001 select"),
        "{argv:?}"
    );
    assert!(
        !argv.iter().any(|l| l.starts_with("blackwall reconcile")),
        "{argv:?}"
    );
}

#[test]
fn wave_halted_record_during_readiness_halts_critic() {
    let env = Env::new("wave_halted_record_during_readiness_halts_critic");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = executor(&env, &steps);
    assert_eq!(
        exec.run_step("build-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    // The first git call after the flag is the Critic's readiness read. It writes a Halted record
    // after the top-of-step check, so only the check under the lock can catch it.
    let flag = env.root.join("halt-now");
    fs::write(&flag, "").unwrap();
    let state = env.state.display().to_string();
    let record = r#"{"step":"other-step","role":"Builder","outcome":{"result":"halted","reason":"VerifyFailed"},"at":1,"seq":0}"#;
    env.tool_pre(
        "git",
        &format!(
            "if [ -f '{flag}' ]; then rm -f '{flag}'; mkdir -p '{state}'; printf '%s' '{record}' > '{state}/other-step.json'; fi",
            flag = flag.display()
        ),
        Vec::new(),
    );
    let got = exec.run_step("review-a", 1).unwrap();
    assert_eq!(got.outcome, StepOutcome::Halted(HaltReason::WaveHalted));
    assert!(
        !env.argv().iter().any(|l| l.starts_with("claude")),
        "{:?}",
        env.argv()
    );
}

#[test]
fn wave_halted_record_before_pr_halts_critic() {
    let env = Env::new("wave_halted_record_before_pr_halts_critic");
    let state = env.state.display().to_string();
    let record = r#"{"step":"other-step","role":"Builder","outcome":{"result":"halted","reason":"VerifyFailed"},"at":1,"seq":0}"#;
    env.tool_pre(
        "git",
        &format!(
            "case \"$*\" in \"worktree add\"*) mkdir -p '{state}'; printf '%s' '{record}' > '{state}/other-step.json' ;; esac"
        ),
        Vec::new(),
    );
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = with_open_prs(&env, &steps);
    assert_eq!(
        exec.run_step("build-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    let got = exec.run_step("review-a", 1).unwrap();
    assert_eq!(got.outcome, StepOutcome::Halted(HaltReason::WaveHalted));
    let argv = env.argv();
    // The Critic ran; the PR did not open.
    assert!(argv.iter().any(|l| l.starts_with("claude -p")), "{argv:?}");
    assert!(
        !argv.iter().any(|l| l.starts_with("blackwall pr")),
        "{argv:?}"
    );
}

#[test]
fn wave_critic_reads_rules_at_the_builder_base() {
    let env = Env::new("wave_critic_reads_rules_at_the_builder_base");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = executor(&env, &steps);
    assert_eq!(
        exec.run_step("build-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    // The wave base moves on: init-head and HEAD both name a new commit.
    fs::write(env.repo.join(".blackwall/init-head"), "1111111111111\n").unwrap();
    env.tool("git", vec![r("rev-parse HEAD", b"1111111111111\n", "", 0)]);
    assert_eq!(
        exec.run_step("review-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    let argv = env.argv();
    assert!(
        has_line(
            &argv,
            &format!("git checkout --no-overlay {BASE} -- .claude")
        ),
        "{argv:?}"
    );
    assert!(
        !argv
            .iter()
            .any(|l| l.starts_with("git checkout") && l.contains("1111111111111")),
        "{argv:?}"
    );
    // The prompt names the Builder's base, not the moved wave base.
    let stdin = fs::read_to_string(env.bin.join("claude.stdin")).unwrap();
    assert!(stdin.contains(&format!("Base: {BASE}")), "{stdin}");
    assert!(!stdin.contains("1111111111111"), "{stdin}");
}

#[test]
fn wave_compensate_newest_pass_first() {
    // build-b passes first, build-a second. Step ids alone would order build-b first.
    let env = Env::new("wave_compensate_newest_pass_first");
    env.tool("make", vec![r("*", b"", "", 1)]);
    let steps = [
        builder("build-b"),
        critic("review-b", "build-b"),
        step(
            "build-a",
            Some("builder"),
            ActivityKind::Impl,
            &["review-b"],
            &[],
        ),
        critic("review-a", "build-a"),
        // The probe waits for both Critics, so it runs after both Builders passed.
        step(
            "probe-x",
            Some("probe"),
            ActivityKind::Verify,
            &["review-a"],
            &["make", "check"],
        ),
    ];
    let report = executor(&env, &steps).run_local(1).unwrap();
    assert_eq!(report.halted, Some(HaltReason::ProbeFailed));
    let ids: Vec<&str> = report
        .compensation
        .iter()
        .map(|c| c.activity_id.as_str())
        .collect();
    assert_eq!(ids, ["build-a", "build-b"]);
}

#[test]
fn wave_critic_stores_pr_number_in_receipt() {
    let env = Env::new("wave_critic_stores_pr_number_in_receipt");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let report = with_open_prs(&env, &steps).run_local(1).unwrap();
    assert_eq!(report.receipts.len(), 1);
    assert_eq!(report.receipts[0].facts().pr_number, Some(57));
}

#[test]
fn wave_probe_runs_at_the_base_argv_exact() {
    let env = Env::new("wave_probe_runs_at_the_base_argv_exact");
    let steps = [probe("probe-x")];
    assert_eq!(
        executor(&env, &steps)
            .run_step("probe-x", 1)
            .unwrap()
            .outcome,
        StepOutcome::Passed
    );
    let argv = env.argv();
    assert!(
        has_line(
            &argv,
            &format!(
                "git worktree add --detach {}/probe-x {BASE}",
                env.wt.display()
            )
        ),
        "{argv:?}"
    );
    assert!(has_line(&argv, "make check"), "{argv:?}");
}

#[test]
fn wave_reconcile_claude_and_make_hold_the_lock() {
    let env = Env::new("wave_reconcile_claude_and_make_hold_the_lock");
    let steps = [
        builder("build-a"),
        critic("review-a", "build-a"),
        probe("probe-x"),
    ];
    let report = executor(&env, &steps).run_local(1).unwrap();
    assert_eq!(report.halted, None);
    assert!(
        !env.argv().iter().any(|l| l == "LOCK NOT HELD"),
        "{:?}",
        env.argv()
    );
}

#[test]
fn wave_lock_probe_detects_an_unheld_lock() {
    // Control for the lock probe: a reconcile with no executor lock must log the failure.
    let env = Env::new("wave_lock_probe_detects_an_unheld_lock");
    fs::create_dir_all(env.lock.parent().unwrap()).unwrap();
    fs::write(&env.lock, "").unwrap();
    let status = std::process::Command::new(env.bin.join("blackwall"))
        .args(["reconcile", "bwrun0000001"])
        .env("PATH", "/usr/bin:/bin")
        .status()
        .unwrap();
    assert!(status.success());
    assert!(
        env.argv().iter().any(|l| l == "LOCK NOT HELD"),
        "{:?}",
        env.argv()
    );
}

const HALT_RECORD: &str = r#"{"step":"other-step","role":"Builder","outcome":{"result":"halted","reason":"VerifyFailed"},"at":1,"seq":0}"#;

fn wait_for(env: &Env, prefix: &str) {
    for _ in 0..200 {
        if env.argv().iter().any(|l| l.starts_with(prefix)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("{prefix} never ran: {:?}", env.argv());
}

#[test]
fn wave_open_pr_failure_halts_and_keeps_pushed_branch() {
    let env = Env::new("wave_open_pr_failure_halts_and_keeps_pushed_branch");
    env.tool("blackwall", vec![r("pr *", b"", "pr failed\n", 1)]);
    // `blackwall pr` pushed the branch before it failed: compensation checks and keeps it.
    env.tool("git", vec![r("ls-remote *", b"abc\trefs/heads/x\n", "", 0)]);
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = with_open_prs(&env, &steps);
    assert_eq!(
        exec.run_step("build-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    let got = exec.run_step("review-a", 1).unwrap();
    assert_eq!(got.outcome, StepOutcome::Halted(HaltReason::PrFailed));
    assert_eq!(got.compensation.len(), 1, "{:?}", got.compensation);
    assert!(got.compensation[0].success, "{:?}", got.compensation);
    let argv = env.argv();
    assert!(
        has_line(
            &argv,
            "git ls-remote --exit-code --heads origin blackwall/bwrun0000001"
        ),
        "{argv:?}"
    );
    assert!(
        !argv.iter().any(|l| l.starts_with("git branch -D")),
        "{argv:?}"
    );
}

#[test]
fn wave_open_pr_failure_deletes_unpushed_branch() {
    let env = Env::new("wave_open_pr_failure_deletes_unpushed_branch");
    env.tool("blackwall", vec![r("pr *", b"", "pr failed\n", 1)]);
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = with_open_prs(&env, &steps);
    assert_eq!(
        exec.run_step("build-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    let got = exec.run_step("review-a", 1).unwrap();
    assert_eq!(got.outcome, StepOutcome::Halted(HaltReason::PrFailed));
    assert_eq!(got.compensation.len(), 1, "{:?}", got.compensation);
    assert!(
        has_line(&env.argv(), "git branch -D blackwall/bwrun0000001"),
        "{:?}",
        env.argv()
    );
}

#[test]
fn wave_builder_halt_during_lock_wait_skips_reconcile() {
    let env = Env::new("wave_builder_halt_during_lock_wait_skips_reconcile");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = executor(&env, &steps);
    fs::create_dir_all(env.lock.parent().unwrap()).unwrap();
    let held = fs::File::create(&env.lock).unwrap();
    held.lock().unwrap();
    let h = std::thread::spawn(move || exec.run_step("build-a", 1).unwrap());
    wait_for(&env, "blackwall settle");
    std::thread::sleep(std::time::Duration::from_millis(500));
    fs::create_dir_all(&env.state).unwrap();
    fs::write(env.state.join("other-step.json"), HALT_RECORD).unwrap();
    drop(held);
    let got = h.join().unwrap();
    assert!(
        !env.argv()
            .iter()
            .any(|l| l.starts_with("blackwall reconcile")),
        "{:?}",
        env.argv()
    );
    assert_eq!(got.outcome, StepOutcome::Halted(HaltReason::WaveHalted));
}

#[test]
fn wave_probe_halt_during_lock_wait_skips_verify() {
    let env = Env::new("wave_probe_halt_during_lock_wait_skips_verify");
    let steps = [probe("probe-x")];
    let exec = executor(&env, &steps);
    fs::create_dir_all(env.lock.parent().unwrap()).unwrap();
    let held = fs::File::create(&env.lock).unwrap();
    held.lock().unwrap();
    let h = std::thread::spawn(move || exec.run_step("probe-x", 1).unwrap());
    wait_for(&env, "git rev-parse HEAD");
    std::thread::sleep(std::time::Duration::from_millis(500));
    fs::create_dir_all(&env.state).unwrap();
    fs::write(env.state.join("other-step.json"), HALT_RECORD).unwrap();
    drop(held);
    let got = h.join().unwrap();
    assert!(!has_line(&env.argv(), "make check"), "{:?}", env.argv());
    assert_eq!(got.outcome, StepOutcome::Halted(HaltReason::WaveHalted));
}

#[test]
fn wave_critic_retry_does_not_reopen_pr() {
    let env = Env::new("wave_critic_retry_does_not_reopen_pr");
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = with_open_prs(&env, &steps);
    assert_eq!(
        exec.run_step("build-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    assert_eq!(
        exec.run_step("review-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    let brec = fs::read_to_string(env.state.join("build-a.json")).unwrap();
    assert!(brec.contains("\"pr\": 57"), "{brec}");
    // A crash lost the Critic record after the Builder record got its PR number.
    fs::remove_file(env.state.join("review-a.json")).unwrap();
    fs::remove_file(&env.log).unwrap();
    assert_eq!(
        exec.run_step("review-a", 2).unwrap().outcome,
        StepOutcome::Passed
    );
    let prs = env
        .argv()
        .iter()
        .filter(|l| l.starts_with("blackwall pr"))
        .count();
    assert_eq!(prs, 0, "{:?}", env.argv());
}

#[test]
fn wave_save_replaces_the_record_and_never_writes_in_place() {
    let env = Env::new("wave_save_replaces_the_record_and_never_writes_in_place");
    // A stale Passed record (other base) is rewritten. Its hard link keeps the old bytes only
    // when the writer replaces the file instead of truncating it in place.
    fs::create_dir_all(&env.state).unwrap();
    let old = r#"{"step":"build-a","role":"Builder","outcome":{"result":"passed"},"base":"stale","at":1,"seq":0}"#;
    fs::write(env.state.join("build-a.json"), old).unwrap();
    fs::hard_link(env.state.join("build-a.json"), env.root.join("alias")).unwrap();
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let got = executor(&env, &steps).run_step("build-a", 2).unwrap();
    assert_eq!(got.outcome, StepOutcome::Passed);
    assert_eq!(fs::read_to_string(env.root.join("alias")).unwrap(), old);
    let new = fs::read_to_string(env.state.join("build-a.json")).unwrap();
    assert!(new.contains(BASE), "{new}");
    // No temp file stays behind.
    let names: Vec<_> = fs::read_dir(&env.state)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["build-a.json"], "{names:?}");
}

#[test]
fn wave_interrupted_write_leaves_no_truncated_record() {
    let env = Env::new("wave_interrupted_write_leaves_no_truncated_record");
    // A crash between the temp write and the rename leaves a truncated temp file. Readers
    // ignore it, and the next save of that step replaces it.
    fs::create_dir_all(&env.state).unwrap();
    fs::write(env.state.join("build-z.tmp"), b"{\"step\":\"build-z\",\"ro").unwrap();
    fs::write(env.state.join("build-a.tmp"), b"{\"step\":\"build-a\",\"ro").unwrap();
    let steps = [builder("build-a"), critic("review-a", "build-a")];
    let exec = executor(&env, &steps);
    assert_eq!(
        exec.run_step("build-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    assert_eq!(
        exec.run_step("review-a", 1).unwrap().outcome,
        StepOutcome::Passed
    );
    let rec = fs::read_to_string(env.state.join("build-a.json")).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&rec).is_ok());
    assert!(!env.state.join("build-a.tmp").exists());
}

#[test]
fn wave_compensation_order_breaks_seq_ties_on_time() {
    let env = Env::new("wave_compensation_order_breaks_seq_ties_on_time");
    env.tool("make", vec![r("*", b"", "", 1)]);
    fs::create_dir_all(&env.state).unwrap();
    // Records with no seq field both read as seq 0: the later `at` (build-a) is newer.
    for (id, run, at) in [
        ("build-a", "bwrun0000001", 9),
        ("build-b", "bwrun0000002", 1),
    ] {
        fs::write(
            env.state.join(format!("{id}.json")),
            format!(
                r#"{{"step":"{id}","role":"Builder","outcome":{{"result":"passed"}},"run":"{run}","at":{at}}}"#
            ),
        )
        .unwrap();
    }
    let steps = [probe("probe-x")];
    let got = executor(&env, &steps).run_step("probe-x", 10).unwrap();
    let ids: Vec<&str> = got
        .compensation
        .iter()
        .map(|c| c.activity_id.as_str())
        .collect();
    assert_eq!(ids, ["build-a", "build-b"]);
}

/// Wait until the log holds `count` lines that start with `prefix`.
fn wait_for_count(env: &Env, prefix: &str, count: usize) {
    for _ in 0..400 {
        if env.argv().iter().filter(|l| l.starts_with(prefix)).count() >= count {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("{count} x {prefix} never ran: {:?}", env.argv());
}

/// A shell line that blocks a fake tool call until the file `gate` exists.
fn gate_line(gate: &std::path::Path) -> String {
    format!(
        "n=0; while [ ! -e '{}' ] && [ $n -lt 500 ]; do sleep 0.01; n=$((n+1)); done",
        gate.display()
    )
}

fn seqs_in(env: &Env) -> Vec<u64> {
    let mut seqs: Vec<u64> = fs::read_dir(&env.state)
        .unwrap()
        .map(|e| {
            let v: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(e.unwrap().path()).unwrap()).unwrap();
            v["seq"].as_u64().unwrap()
        })
        .collect();
    seqs.sort_unstable();
    seqs
}

fn many_builders(env: &Env, n: usize) -> (Vec<String>, Vec<SagaStep>) {
    let ids: Vec<String> = (0..n).map(|i| format!("build-{i}")).collect();
    let mut steps = Vec::new();
    for id in &ids {
        env.task(id, Some("haiku"));
        steps.push(builder(id));
        steps.push(critic(&format!("review-{id}"), id));
    }
    (ids, steps)
}

#[test]
fn wave_concurrent_halts_get_distinct_seq() {
    let env = Env::new("wave_concurrent_halts_get_distinct_seq");
    let gate = env.root.join("gate");
    // Every Builder fails at `start` at the same moment, so the halts race for a sequence number.
    env.tool_pre(
        "blackwall",
        &format!("case \"$1\" in start) {} ;; esac", gate_line(&gate)),
        vec![r("start *", b"", "no ok line\n", 1)],
    );
    let (ids, steps) = many_builders(&env, 8);
    let hs: Vec<_> = ids
        .into_iter()
        .map(|id| {
            let exec = executor(&env, &steps);
            std::thread::spawn(move || exec.run_step(&id, 1).unwrap())
        })
        .collect();
    wait_for_count(&env, "blackwall start", 8);
    fs::write(&gate, "").unwrap();
    for h in hs {
        let _ = h.join().unwrap();
    }
    let seqs = seqs_in(&env);
    let mut unique = seqs.clone();
    unique.dedup();
    assert_eq!(unique.len(), seqs.len(), "seq tie: {seqs:?}");
}

#[test]
fn wave_concurrent_passes_get_distinct_seq() {
    let env = Env::new("wave_concurrent_passes_get_distinct_seq");
    let gate = env.root.join("gate");
    // Every Builder reaches its pass at the same moment: the diff call waits for the gate.
    env.tool_pre(
        "git",
        &format!("case \"$1\" in diff) {} ;; esac", gate_line(&gate)),
        Vec::new(),
    );
    env.tool("blackwall", vec![r("start *", b"", START_A, 0)]);
    let (ids, steps) = many_builders(&env, 8);
    let hs: Vec<_> = ids
        .into_iter()
        .map(|id| {
            let exec = executor(&env, &steps);
            std::thread::spawn(move || exec.run_step(&id, 1).unwrap())
        })
        .collect();
    wait_for_count(&env, "git diff", 8);
    fs::write(&gate, "").unwrap();
    for h in hs {
        assert_eq!(h.join().unwrap().outcome, StepOutcome::Passed);
    }
    assert_eq!(seqs_in(&env), (0..8).collect::<Vec<u64>>());
}

/// A Halted record queued on the lock while the Critic session holds it must land before
/// `blackwall pr` runs. Thirty runs in parallel, because the window is narrow.
#[test]
fn wave_halt_queued_during_critic_session_stops_open_pr() {
    use std::io::Write as _;
    let hs: Vec<_> = (0..30)
        .map(|i| {
            std::thread::spawn(move || {
                let env = Env::new(&format!("wave_race_{i}"));
                env.tool_pre("claude", "sleep 0.3", Vec::new());
                let steps = [builder("build-a"), critic("review-a", "build-a")];
                let exec = with_open_prs(&env, &steps);
                assert_eq!(
                    exec.run_step("build-a", 1).unwrap().outcome,
                    StepOutcome::Passed
                );
                let (lock, state, log) = (env.lock.clone(), env.state.clone(), env.log.clone());
                let h = std::thread::spawn(move || exec.run_step("review-a", 1).unwrap());
                wait_for(&env, "claude -p");
                let f = fs::File::create(&lock).unwrap();
                f.lock().unwrap();
                fs::write(state.join("other-step.json"), HALT_RECORD).unwrap();
                fs::OpenOptions::new()
                    .append(true)
                    .open(&log)
                    .unwrap()
                    .write_all(b"HALT WRITTEN\n")
                    .unwrap();
                drop(f);
                h.join().unwrap();
                let argv = env.argv();
                let at = argv.iter().position(|l| l == "HALT WRITTEN").unwrap();
                argv.iter().skip(at).any(|l| l.starts_with("blackwall pr"))
            })
        })
        .collect();
    let mut bad = 0;
    for h in hs {
        if h.join().unwrap() {
            bad += 1;
        }
    }
    assert_eq!(bad, 0, "a PR opened after a Halted record was written");
}

/// `report.receipts` follows the fixed record order: oldest first. Six Builders make a
/// directory-order read differ from the record order, and a newest-first read fails too.
#[test]
fn wave_report_receipts_follow_the_record_order() {
    let env = Env::new("wave_report_receipts_follow_the_record_order");
    let (_, steps) = many_builders(&env, 6);
    let starts: Vec<(String, Vec<u8>)> = (0..6)
        .map(|i| {
            let text = format!(
                "\u{2713} run haiku (ok): hash bwrun00000{i}0 (new)\n\u{2713} changeset: chg00000000{i} (1 entries)\n"
            );
            (format!("start --task build-{i} *"), text.into_bytes())
        })
        .collect();
    let starts: Vec<(&'static str, &'static [u8], String)> = starts
        .into_iter()
        .map(|(pat, text)| {
            let pat: &'static str = Box::leak(pat.into_boxed_str());
            let err = String::from_utf8(text).unwrap();
            (pat, &[][..], err)
        })
        .collect();
    let rules = starts
        .into_iter()
        .map(|(pat, out, err)| r(pat, out, Box::leak(err.into_boxed_str()), 0))
        .collect();
    env.tool("blackwall", rules);
    let report = executor(&env, &steps).run_local(1).unwrap();
    assert_eq!(report.halted, None);
    let got: Vec<&str> = report
        .receipts
        .iter()
        .map(|r| r.facts().blackwall_run.as_str())
        .collect();
    let want: Vec<String> = (0..6).map(|i| format!("bwrun00000{i}0")).collect();
    assert_eq!(got, want);
}

/// The live run of spec 0025 P9c. It runs only with `CAS_WAVE_LIVE=1` and panics when an input
/// is missing: `CAS_WAVE_REPO` (the custody checkout) and `CAS_WAVE_PLAN` (a saga flow TOML).
#[test]
#[ignore = "live: needs CAS_WAVE_LIVE=1, CAS_WAVE_REPO and CAS_WAVE_PLAN"]
fn live_smoke() {
    assert_eq!(
        std::env::var("CAS_WAVE_LIVE").as_deref(),
        Ok("1"),
        "CAS_WAVE_LIVE=1 is required"
    );
    let repo = std::env::var("CAS_WAVE_REPO").expect("CAS_WAVE_REPO is required");
    let plan_path = std::env::var("CAS_WAVE_PLAN").expect("CAS_WAVE_PLAN is required");
    let flow = fs::read_to_string(&plan_path).expect("CAS_WAVE_PLAN is readable");
    let def = cas_engine::SagaFlowDef::from_toml(&flow).expect("CAS_WAVE_PLAN parses");
    let plan = WavePlan::plan(&def.to_steps(), 2).expect("the plan is acyclic");
    let scratch = std::env::temp_dir().join(format!("cas-wave-live-{}", std::process::id()));
    let config = WaveExecConfig {
        open_prs: std::env::var("CAS_WAVE_LIVE_PR").as_deref() == Ok("1"),
        ..WaveExecConfig::new(
            PathBuf::from(repo),
            scratch.join("state"),
            scratch.join("wt"),
            scratch.join("wave.lock"),
        )
    };
    let report = WaveExecutor::new(config, plan)
        .run_local(0)
        .expect("the run finishes");
    println!("{report:#?}");
    let _ = fs::remove_dir_all(&scratch);
}
