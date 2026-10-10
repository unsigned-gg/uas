//! The landing activity: reconcile a settled changeset with Blackwall and open its pull request.
//!
//! The activity runs `git` and the `blackwall` CLI as subprocesses (spec 0022 D1). It
//! reads the verify command from the repository's `blackwall.toml`. It never pushes,
//! never passes `--force`, `--no-verify`, `--pr` or `--draft`, and it never calls
//! `blackwall` when the base is stale (spec 0022 D3). The receipt comes from `uas-domain`
//! and carries the evidence tier.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde::Deserialize;
use uas_domain::{
    AuditStatus, CostSource, LandingFacts, LandingReceipt, LandingReceiptError, SlotId,
};

/// Where the activity runs and which refs it uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandingConfig {
    /// The repository checkout that `git` and `blackwall` run in. Its `blackwall.toml`
    /// names the verify command.
    pub repo_dir: PathBuf,
    /// The development ref that a changeset must be based on.
    pub development_ref: String,
    /// The base branch of the pull request.
    pub pr_base: String,
    /// The `PATH` for each child process. `None` inherits the parent `PATH`.
    pub path_env: Option<OsString>,
}

impl LandingConfig {
    /// Defaults for `repo_dir`: `origin/development` as the base, `development` as the PR base.
    pub fn new(repo_dir: impl Into<PathBuf>) -> Self {
        Self {
            repo_dir: repo_dir.into(),
            development_ref: "origin/development".into(),
            pr_base: "development".into(),
            path_env: None,
        }
    }
}

/// The facts a caller supplies for one landing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandingInput {
    /// The slot whose changeset lands.
    pub slot_id: SlotId,
    /// The slot run hash from the spec 0014 audit receipt.
    pub run_hash: String,
    /// The audit status of the slot run.
    pub audit: AuditStatus,
    /// The CPU cap the slot ran under (spec 0020).
    pub cpu_cap: String,
    /// The Blackwall run hash of the settled run.
    pub blackwall_run: String,
    /// The BLAKE3 id of the changeset.
    pub changeset_hash: String,
    /// The base commit the slot forked from.
    pub base_sha: String,
    /// True when the changeset touched an evaluator file (spec 0023 D3).
    pub evaluator_touched: bool,
    /// Where the cost figures came from (spec 0023 D2).
    pub cost_source: CostSource,
    /// The time of the landing, in epoch seconds.
    pub at: u64,
}

/// The result of one landing attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LandingOutcome {
    /// The pull request was opened. The tier is `verified` only when reconcile exited zero
    /// and no evaluator file changed.
    Opened(LandingReceipt),
    /// `blackwall reconcile` exited 3, the verify gate failed. No pull request was opened (spec 0022 F14, reported).
    VerifyFailed(LandingReceipt),
    /// `blackwall.toml` has no `[verify]` command. Nothing was reconciled and no pull
    /// request was opened.
    NoVerifyGate(LandingReceipt),
    /// The base is not an ancestor of the development head. Nothing was applied.
    StaleBase {
        /// The receipt, with `stale_base` set.
        receipt: LandingReceipt,
        /// The commit that the development ref names now.
        development_head: String,
    },
}

/// The result of reconciling one changeset, before its pull request (spec 0025 D1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOutcome {
    /// Reconcile exited zero. The receipt has `verify_exit` 0, a branch and no PR number.
    Reconciled(LandingReceipt),
    /// `blackwall reconcile` exited 3, the verify gate failed. Nothing was opened.
    VerifyFailed(LandingReceipt),
    /// `blackwall.toml` has no `[verify]` command. Nothing was reconciled.
    NoVerifyGate(LandingReceipt),
    /// The base is not an ancestor of the development head. Nothing was applied.
    StaleBase {
        /// The receipt, with `stale_base` set.
        receipt: LandingReceipt,
        /// The commit that the development ref names now.
        development_head: String,
    },
}

/// A landing attempt that could not finish.
#[derive(Debug, thiserror::Error)]
pub enum LandingError {
    /// A child process could not start.
    #[error("could not run {program}: {source}")]
    Spawn {
        /// The program that failed to start.
        program: &'static str,
        /// The operating system error.
        #[source]
        source: io::Error,
    },
    /// A step exited with a status the activity does not accept.
    #[error("{step} exited with {code:?}: {stderr}")]
    UnexpectedExit {
        /// The step that failed, for example `merge-base` or `pr`.
        step: &'static str,
        /// The exit code, or `None` when the process was killed by a signal.
        code: Option<i32>,
        /// The trimmed standard error of the step.
        stderr: String,
    },
    /// The standard input of a step could not be written.
    #[error("could not write stdin of {program}: {source}")]
    Stdin {
        /// The program whose standard input failed.
        program: &'static str,
        /// The operating system error.
        #[source]
        source: io::Error,
    },
    /// The landing receipt could not be serialized for the pull request body.
    #[error("could not serialize the landing receipt: {0}")]
    Serialize(#[from] serde_json::Error),
    /// The receipt rule refused the receipt that the landing built.
    #[error("landing receipt refused: {0}")]
    Receipt(#[from] LandingReceiptError),
    /// `blackwall pr` succeeded but printed no `/pull/<n>` link.
    #[error("blackwall pr printed no pull request number")]
    PrNumberUnparsed,
    /// `blackwall.toml` exists but could not be read.
    #[error("could not read {path}: {source}")]
    ReadConfig {
        /// The file that could not be read.
        path: PathBuf,
        /// The operating system error.
        #[source]
        source: io::Error,
    },
    /// `blackwall.toml` exists but is not valid TOML.
    #[error("could not parse {path}: {source}")]
    ParseConfig {
        /// The file that did not parse.
        path: PathBuf,
        /// The TOML error.
        #[source]
        source: toml::de::Error,
    },
}

/// Runs the landing steps for one changeset.
#[derive(Debug, Clone)]
pub struct LandingActivity {
    config: LandingConfig,
}

impl LandingActivity {
    /// Build an activity from its configuration.
    pub fn new(config: LandingConfig) -> Self {
        Self { config }
    }

    /// Land one changeset: reconcile it, then open its pull request (spec 0022 D1).
    pub fn land(&self, input: &LandingInput) -> Result<LandingOutcome, LandingError> {
        match self.reconcile(input)? {
            ReconcileOutcome::Reconciled(reconciled) => {
                let opened = self.open_pr(&reconciled)?;
                Ok(LandingOutcome::Opened(opened))
            }
            ReconcileOutcome::VerifyFailed(receipt) => Ok(LandingOutcome::VerifyFailed(receipt)),
            ReconcileOutcome::NoVerifyGate(receipt) => Ok(LandingOutcome::NoVerifyGate(receipt)),
            ReconcileOutcome::StaleBase {
                receipt,
                development_head,
            } => Ok(LandingOutcome::StaleBase {
                receipt,
                development_head,
            }),
        }
    }

    /// Reconcile one changeset with Blackwall, and open no pull request (steps 0 to 4).
    /// Read the verify command, check the base, refuse without a verify gate, then reconcile.
    pub fn reconcile(&self, input: &LandingInput) -> Result<ReconcileOutcome, LandingError> {
        let verify_cmd = read_verify_cmd(&self.config.repo_dir)?;
        let base = input.base_sha.as_str();
        let dev = self.config.development_ref.as_str();

        let ancestor = self.run("git", &["merge-base", "--is-ancestor", base, dev])?;
        match ancestor.status.code() {
            Some(0) => {}
            Some(1) => {
                let development_head = self.git_head()?;
                let parts = ReceiptParts {
                    development_head: Some(development_head.clone()),
                    ..ReceiptParts::default()
                };
                let receipt = self.receipt(input, verify_cmd, parts)?;
                return Ok(ReconcileOutcome::StaleBase {
                    receipt,
                    development_head,
                });
            }
            code => return Err(unexpected("merge-base", code, &ancestor)),
        }

        if verify_cmd.is_none() {
            // Fail closed: the custody process opens no PR without a verify gate,
            // and reconcile is skipped too so no `blackwall/` branch is created.
            let receipt = self.receipt(input, None, ReceiptParts::default())?;
            return Ok(ReconcileOutcome::NoVerifyGate(receipt));
        }

        let run = input.blackwall_run.as_str();
        let branch = format!("blackwall/{}", short_run(run));
        let reconcile = self.run(
            "blackwall",
            &["reconcile", run, "--base", base, "--branch", &branch],
        )?;
        match reconcile.status.code() {
            Some(0) => {}
            // blackwall exits 3 when the verify gate fails (spec 0022 D7; F14 is reported, soma lane).
            Some(3) => {
                let parts = ReceiptParts {
                    verify_exit: Some(3),
                    ..ReceiptParts::default()
                };
                let receipt = self.receipt(input, verify_cmd, parts)?;
                return Ok(ReconcileOutcome::VerifyFailed(receipt));
            }
            code => return Err(unexpected("reconcile", code, &reconcile)),
        }

        // The PR does not exist yet, so the receipt has no PR number (spec 0022 D11).
        let reconciled = self.receipt(
            input,
            verify_cmd,
            ReceiptParts {
                verify_exit: Some(0),
                branch: Some(branch),
                ..ReceiptParts::default()
            },
        )?;
        Ok(ReconcileOutcome::Reconciled(reconciled))
    }

    /// Open the pull request of a reconciled changeset (step 5). The body is the fenced JSON
    /// of `reconciled` (spec 0022 D11). Pass only the receipt that `reconcile` returned: verify
    /// exit 0, a branch and no PR number.
    pub fn open_pr(&self, reconciled: &LandingReceipt) -> Result<LandingReceipt, LandingError> {
        let run = reconciled.facts().blackwall_run.as_str();
        let body = format!(
            "```json\n{}\n```\n",
            serde_json::to_string_pretty(reconciled)?
        );
        let pr_base = self.config.pr_base.as_str();
        let pr = self.run_with_stdin(
            "blackwall",
            &["pr", run, "--base", pr_base, "--body", "-"],
            body.as_bytes(),
        )?;
        if pr.status.code() != Some(0) {
            return Err(unexpected("pr", pr.status.code(), &pr));
        }
        let stdout = String::from_utf8_lossy(&pr.stdout);
        let number = parse_pr_number(&stdout).ok_or(LandingError::PrNumberUnparsed)?;
        let mut facts = reconciled.facts().clone();
        facts.pr_number = Some(number);
        Ok(LandingReceipt::new(facts)?)
    }

    fn receipt(
        &self,
        input: &LandingInput,
        verify_cmd: Option<String>,
        parts: ReceiptParts,
    ) -> Result<LandingReceipt, LandingError> {
        let facts = LandingFacts {
            slot_id: input.slot_id.clone(),
            run_hash: input.run_hash.clone(),
            audit: input.audit.clone(),
            cpu_cap: input.cpu_cap.clone(),
            blackwall_run: input.blackwall_run.clone(),
            changeset_hash: input.changeset_hash.clone(),
            base_sha: input.base_sha.clone(),
            stale_base: parts.development_head.is_some(),
            development_head: parts.development_head,
            verify_cmd,
            verify_exit: parts.verify_exit,
            branch: parts.branch,
            pr_number: parts.pr_number,
            merge_commit: None,
            evaluator_touched: input.evaluator_touched,
            cost_source: input.cost_source,
            at: input.at,
        };
        Ok(LandingReceipt::new(facts)?)
    }

    fn git_head(&self) -> Result<String, LandingError> {
        let dev = self.config.development_ref.as_str();
        let out = self.run("git", &["rev-parse", dev])?;
        if out.status.code() != Some(0) {
            return Err(unexpected("rev-parse", out.status.code(), &out));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    fn run(&self, program: &'static str, args: &[&str]) -> Result<Output, LandingError> {
        let mut cmd = Command::new(program);
        cmd.args(args).current_dir(&self.config.repo_dir);
        if let Some(path) = &self.config.path_env {
            cmd.env("PATH", path);
        }
        cmd.output()
            .map_err(|source| LandingError::Spawn { program, source })
    }

    /// Run a step and write `stdin` to it. A child that exits before it reads all of
    /// `stdin` is not an error here: its exit code decides (`BrokenPipe` is ignored).
    fn run_with_stdin(
        &self,
        program: &'static str,
        args: &[&str],
        stdin: &[u8],
    ) -> Result<Output, LandingError> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&self.config.repo_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(path) = &self.config.path_env {
            cmd.env("PATH", path);
        }
        let mut child = cmd
            .spawn()
            .map_err(|source| LandingError::Spawn { program, source })?;
        if let Some(mut handle) = child.stdin.take() {
            match handle.write_all(stdin) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {}
                Err(source) => {
                    // Reap the child so it does not stay a zombie (the error path has no receipt).
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(LandingError::Stdin { program, source });
                }
            }
            drop(handle);
        }
        child
            .wait_with_output()
            .map_err(|source| LandingError::Spawn { program, source })
    }
}

/// The outcome fields of a receipt that depend on the step that ran.
#[derive(Debug, Default)]
struct ReceiptParts {
    /// The development head, `Some` only for a stale base (spec 0022 D9).
    development_head: Option<String>,
    /// The `blackwall reconcile` exit code, when reconcile ran.
    verify_exit: Option<i32>,
    /// The landing branch, when one was created.
    branch: Option<String>,
    /// The pull request number, when one was opened.
    pr_number: Option<u64>,
}

/// The subset of `blackwall.toml` the activity reads.
#[derive(Deserialize)]
struct BlackwallConfig {
    verify: Option<VerifySection>,
}

#[derive(Deserialize)]
struct VerifySection {
    command: Option<String>,
}

/// The `[verify] command` of `<repo_dir>/blackwall.toml`. `None` when the file is absent,
/// has no `[verify]` section, or sets an empty command.
fn read_verify_cmd(repo_dir: &Path) -> Result<Option<String>, LandingError> {
    let path = repo_dir.join("blackwall.toml");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(LandingError::ReadConfig { path, source }),
    };
    let parsed: BlackwallConfig =
        toml::from_str(&text).map_err(|source| LandingError::ParseConfig { path, source })?;
    Ok(parsed
        .verify
        .and_then(|verify| verify.command)
        .filter(|command| !command.trim().is_empty()))
}

fn unexpected(step: &'static str, code: Option<i32>, out: &Output) -> LandingError {
    LandingError::UnexpectedExit {
        step,
        code,
        stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
    }
}

/// The first twelve characters of a run hash, for the branch name.
fn short_run(run: &str) -> &str {
    run.char_indices().nth(12).map_or(run, |(i, _)| &run[..i])
}

/// The number after the last `/pull/` in `stdout`, if there is one.
fn parse_pr_number(stdout: &str) -> Option<u64> {
    const MARK: &str = "/pull/";
    let idx = stdout.rfind(MARK)?;
    let digits: String = stdout[idx + MARK.len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}
