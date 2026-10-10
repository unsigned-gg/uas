//! The wave executor on the custody chain (spec 0025).
//!
//! `WaveExecutor` runs one step of a wave plan. A Builder is a jailed `blackwall start`, then
//! `settle select`, then the reconcile of `LandingActivity`. A Critic reviews the reconciled
//! head in a transient worktree. A Probe runs its command at the wave base. A MainSession step
//! waits for a saga gate. Every subprocess has a fixed argv (spec 0025 D3). The executor never
//! approves a gate and never discards a blackwall run.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use cas_domain::{
    is_critic_input_path, is_evaluator_path, AuditStatus, CostSource, LandingReceipt, SlotId,
};
use serde::{Deserialize, Serialize};

use crate::{
    validate_verdict, Activity, ActivityKind, AgentRole, CompensationReceipt, Compensator,
    EngineError, LandingActivity, LandingConfig, LandingError, LandingInput, ReconcileOutcome,
    WaveOutcome, WavePlan, WaveStep, WorkflowEvent,
};

/// The persona that every Builder runs as (spec 0025 D6).
const BUILDER_PERSONA: &str = "cas-builder";

/// Where the executor runs and which names it uses (spec 0025 D6, D8, D16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaveExecConfig {
    /// The custody checkout. It holds `.blackwall/init-head` and `tasks/`.
    pub repo_dir: PathBuf,
    /// The development ref that a merged Builder head is an ancestor of.
    pub development_ref: String,
    /// The base branch of each pull request.
    pub pr_base: String,
    /// The `PATH` for each child process. `None` inherits the parent `PATH`.
    pub path_env: Option<OsString>,
    /// The host-wide lock file (spec 0025 D8).
    pub lock_path: PathBuf,
    /// The directory of transient worktrees. It lies outside `repo_dir`.
    pub scratch_dir: PathBuf,
    /// The directory of step records. It lies outside `repo_dir`.
    pub state_dir: PathBuf,
    /// The model every Builder runs with (spec 0025 D6).
    pub builder_model: String,
    /// The agent that runs each Critic (spec 0025 D6).
    pub critic_agent: String,
    /// True when a passed Critic opens the Builder's pull request (spec 0025 D16).
    pub open_prs: bool,
    /// True when the plan asks for merge on green (spec 0025 D15). The executor never merges;
    /// the flag only needs `open_prs`, and `cas wave flow` emits the merge steps.
    pub merge_on_green: bool,
    /// How long a merge step waits for blackwall's merge-gate (spec 0025 D15).
    pub merge_wait: Duration,
}

impl WaveExecConfig {
    /// Defaults: `origin/development` as the base, `development` as the PR base, `sonnet` for
    /// Builders, `cas-critic` for Critics, and no pull requests.
    pub fn new(
        repo_dir: impl Into<PathBuf>,
        state_dir: impl Into<PathBuf>,
        scratch_dir: impl Into<PathBuf>,
        lock_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            repo_dir: repo_dir.into(),
            development_ref: "origin/development".into(),
            pr_base: "development".into(),
            path_env: None,
            lock_path: lock_path.into(),
            scratch_dir: scratch_dir.into(),
            state_dir: state_dir.into(),
            builder_model: "sonnet".into(),
            critic_agent: "cas-critic".into(),
            open_prs: false,
            merge_on_green: false,
            merge_wait: Duration::from_secs(900),
        }
    }
}

/// Why a plan is refused before its first subprocess call (spec 0025 admission table).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdmitError {
    /// A Builder must have exactly one Critic that depends on it directly (D10).
    #[error("builder {step} has {critics} critics that depend on it directly; need exactly one")]
    BuilderCriticCount {
        /// The Builder step.
        step: String,
        /// How many Critics depend on it.
        critics: usize,
    },
    /// A Builder has no `tasks/<id>/prompt.md`.
    #[error("builder {0} has no tasks/<id>/prompt.md")]
    MissingTaskPrompt(String),
    /// A Builder's `task.toml` has no `model` key, or no `task.toml` at all.
    #[error("builder {0} has no model in its task.toml")]
    TaskModelAbsent(String),
    /// A Builder's `task.toml` names another model than the configured one (F12).
    #[error("builder {step} task.toml sets model {found}")]
    TaskModelMismatch {
        /// The Builder step.
        step: String,
        /// The model the file names.
        found: String,
    },
    /// A Probe has no `verify_cmd`.
    #[error("probe {0} has no verify_cmd")]
    ProbeWithoutVerifyCmd(String),
    /// A Probe depends on a Builder (D18).
    #[error("probe {0} depends on a builder")]
    ProbeAfterBuilder(String),
    /// The step id does not make a valid `wave-<id>` slot id.
    #[error("step id {0} does not make a valid slot id")]
    BadSlotId(String),
    /// `merge_on_green` is set and `open_prs` is not (D15).
    #[error("merge_on_green needs open_prs")]
    MergeWithoutOpenPrs,
}

/// A failure of the executor itself, not a halt of the wave.
#[derive(Debug, thiserror::Error)]
pub enum WaveError {
    /// The plan failed an admission row.
    #[error("plan refused: {0}")]
    Admit(#[from] AdmitError),
    /// The plan has no step with this id.
    #[error("no step named {0} in the plan")]
    UnknownStep(String),
    /// A child process could not start, or its standard input failed.
    #[error("{program}: {source}")]
    Process {
        /// The program that failed.
        program: String,
        /// The operating system error.
        #[source]
        source: io::Error,
    },
    /// A file the executor reads or writes failed.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The operating system error.
        #[source]
        source: io::Error,
    },
    /// A step record does not parse, or cannot be written.
    #[error("record {path}: {source}")]
    Record {
        /// The record file.
        path: PathBuf,
        /// The JSON error.
        #[source]
        source: serde_json::Error,
    },
    /// The landing activity failed.
    #[error(transparent)]
    Landing(#[from] LandingError),
    /// `cas wave merge-gate` refuses to run (spec 0025 D15).
    #[error("merge-gate refused: {0}")]
    MergeGate(String),
}

/// A reason a step halts the wave (spec 0025 admission, readiness and halt table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaltReason {
    /// `start` printed no `(ok)` run line or no `changeset:` line, or settle or a head read failed.
    BuilderRunFailed,
    /// `blackwall reconcile` exited 3, the verify gate failed.
    VerifyFailed,
    /// `blackwall.toml` has no verify command.
    NoVerifyGate,
    /// The base is not an ancestor of the development head.
    StaleBase,
    /// The base-to-head diff touches an evaluator or critic input path (D17).
    EvaluatorTouched,
    /// The Critic's verdict is `VERDICT: DEFECTS`.
    CriticDefects,
    /// The Critic's output is not a verdict, or the Critic exited non-zero.
    CriticUnparsed,
    /// The Probe command exited non-zero.
    ProbeFailed,
    /// A `Halted` record exists, so no step runs.
    WaveHalted,
    /// `blackwall pr` failed after a Critic passed.
    PrFailed,
    /// The executor itself failed before an outcome: a missing tool, or an I/O or record error.
    ExecutorError,
}

impl HaltReason {
    /// The name that the record and the event carry.
    pub fn name(self) -> &'static str {
        match self {
            Self::BuilderRunFailed => "BuilderRunFailed",
            Self::VerifyFailed => "VerifyFailed",
            Self::NoVerifyGate => "NoVerifyGate",
            Self::StaleBase => "StaleBase",
            Self::EvaluatorTouched => "EvaluatorTouched",
            Self::CriticDefects => "CriticDefects",
            Self::CriticUnparsed => "CriticUnparsed",
            Self::ProbeFailed => "ProbeFailed",
            Self::WaveHalted => "WaveHalted",
            Self::PrFailed => "PrFailed",
            Self::ExecutorError => "ExecutorError",
        }
    }
}

/// A reason a step is not ready. It exits 2 and writes nothing (spec 0025).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotReady {
    /// `.blackwall/init-head` is absent.
    NoInitHead,
    /// `git rev-parse HEAD` differs from `init-head`.
    HeadDiffers,
    /// A dependency that is not MainSession has no `Passed` record.
    DepRecordMissing(String),
    /// A Builder's upstream Builder has not merged into development (D7).
    UpstreamNotMerged(String),
}

/// How one step ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    /// The step passed.
    Passed,
    /// The step is a MainSession gate. Only `saga gate approve` clears it.
    AwaitingGate,
    /// The step is not ready. Nothing ran.
    NotReady(NotReady),
    /// The step halted. Earlier passed records were compensated.
    Halted(HaltReason),
}

/// The result of one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    /// How the step ended.
    pub outcome: StepOutcome,
    /// The event of the step, when the step ran or halted (spec 0025 D5).
    pub event: Option<WorkflowEvent>,
    /// The Builder receipt, when the step produced one.
    pub receipt: Option<LandingReceipt>,
    /// The compensation receipts of a halt, newest passed record first.
    pub compensation: Vec<CompensationReceipt>,
}

impl StepResult {
    fn bare(outcome: StepOutcome) -> Self {
        Self {
            outcome,
            event: None,
            receipt: None,
            compensation: Vec::new(),
        }
    }
}

/// The result of running a whole plan, in plan order, until a step stops it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WaveReport {
    /// One event per step that ran or halted. Sequence numbers count from zero.
    pub events: Vec<WorkflowEvent>,
    /// The halt that stopped the wave, when one did.
    pub halted: Option<HaltReason>,
    /// The first step that was not ready, when one stopped the wave.
    pub not_ready: Option<NotReady>,
    /// The MainSession step that waits for a gate, when one stopped the wave.
    pub awaiting_gate: Option<String>,
    /// Every compensation receipt of a halt.
    pub compensation: Vec<CompensationReceipt>,
    /// One receipt per Builder that passed.
    pub receipts: Vec<LandingReceipt>,
}

/// The verdict of a Critic run (spec 0025 F10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriticVerdict {
    /// First line `VERDICT: NO DEFECTS`, a valid length and exit 0.
    NoDefects,
    /// First line `VERDICT: DEFECTS`, a valid length and exit 0.
    Defects,
    /// Anything else.
    Unparsed,
}

/// Read a Critic verdict from its standard output and exit code.
pub fn parse_critic_verdict(stdout: &str, exit: Option<i32>) -> CriticVerdict {
    if exit != Some(0) || validate_verdict(stdout).is_err() {
        return CriticVerdict::Unparsed;
    }
    match stdout.lines().next().map(str::trim_end) {
        Some("VERDICT: NO DEFECTS") => CriticVerdict::NoDefects,
        Some("VERDICT: DEFECTS") => CriticVerdict::Defects,
        _ => CriticVerdict::Unparsed,
    }
}

/// The custody facts of a step, as far as it got. Every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Facts {
    run: Option<String>,
    changeset: Option<String>,
    base: Option<String>,
    branch: Option<String>,
    head: Option<String>,
    pr: Option<u64>,
    #[serde(default)]
    merge_commit: Option<String>,
}

/// One step record in the state directory. A `Halted` record blocks every step.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StepRecord {
    step: String,
    role: AgentRole,
    outcome: WaveOutcome,
    #[serde(flatten)]
    facts: Facts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    receipt: Option<LandingReceipt>,
    at: u64,
    /// Monotonic per state dir: the pass or halt order. Compensation runs newest first on it.
    #[serde(default)]
    seq: u64,
}

/// What the Critic lock section found: a halt record, or the Critic's output.
enum Session {
    Halted,
    Ran(Option<Output>),
}

/// Runs the steps of one wave plan on the custody chain.
#[derive(Debug, Clone)]
pub struct WaveExecutor {
    config: WaveExecConfig,
    plan: WavePlan,
}

impl WaveExecutor {
    /// Build an executor for `plan`.
    pub fn new(config: WaveExecConfig, plan: WavePlan) -> Self {
        Self { config, plan }
    }

    /// Check every admission row. No subprocess runs here.
    pub fn admit(&self) -> Result<(), AdmitError> {
        // The plan asks for merge on green: the merge steps need the PRs to exist (D15).
        if self.config.merge_on_green && !self.config.open_prs {
            return Err(AdmitError::MergeWithoutOpenPrs);
        }
        let steps = self.all_steps();
        for (_, ws) in &steps {
            let id = ws.step.id.as_str();
            if SlotId::new(format!("wave-{id}")).is_err() {
                return Err(AdmitError::BadSlotId(id.to_owned()));
            }
            match ws.role {
                AgentRole::Builder => {
                    let critics = steps
                        .iter()
                        .filter(|(_, c)| {
                            c.role == AgentRole::Critic && c.step.deps.iter().any(|d| d == id)
                        })
                        .count();
                    if critics != 1 {
                        return Err(AdmitError::BuilderCriticCount {
                            step: id.to_owned(),
                            critics,
                        });
                    }
                    let prompt = self.task_dir(id).join("prompt.md");
                    if !prompt.is_file() {
                        return Err(AdmitError::MissingTaskPrompt(id.to_owned()));
                    }
                    self.admit_task_model(id)?;
                }
                AgentRole::Probe => {
                    if ws.step.verify_cmd.is_empty() {
                        return Err(AdmitError::ProbeWithoutVerifyCmd(id.to_owned()));
                    }
                    if ws
                        .step
                        .deps
                        .iter()
                        .any(|d| self.role_of(d) == Some(AgentRole::Builder))
                    {
                        return Err(AdmitError::ProbeAfterBuilder(id.to_owned()));
                    }
                }
                AgentRole::Critic | AgentRole::MainSession => {}
            }
        }
        Ok(())
    }

    /// The `blackwall merge-gate` argv for a Builder's PR (spec 0025 D15). It runs no `gh`.
    pub fn merge_gate_argv(&self, builder: &str) -> Result<Vec<String>, WaveError> {
        let refuse = |reason: String| Err(WaveError::MergeGate(reason));
        if self.any_halted()? {
            return refuse("the wave is halted (a Halted record exists)".into());
        }
        let rec = match self.load(builder)? {
            Some(rec) if rec.outcome == WaveOutcome::Passed => rec,
            _ => return refuse(format!("no passed record for {builder}")),
        };
        let Some(pr) = rec.facts.pr else {
            return refuse(format!("no PR on the record of {builder}"));
        };
        let (Some(base), Some(head)) = (&rec.facts.base, &rec.facts.head) else {
            return refuse(format!("the record of {builder} has no base or head"));
        };
        let repo = self.config.repo_dir.clone();
        let range = format!("{base}..{head}");
        let log = self.run(&repo, "git", &["log", "--format=%G?", &range], None)?;
        let text = String::from_utf8_lossy(&log.stdout);
        let signed = log.status.code() == Some(0)
            && text.lines().next().is_some()
            && text.lines().all(|l| l == "G");
        if !signed {
            return refuse(format!("unsigned commit in {range}"));
        }
        Ok([
            "blackwall",
            "merge-gate",
            &pr.to_string(),
            "--watch",
            "--timeout",
            &self.config.merge_wait.as_secs().to_string(),
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect())
    }

    /// Run one step by id. A step that is not ready writes no record and no event.
    pub fn run_step(&self, id: &str, now: u64) -> Result<StepResult, WaveError> {
        self.admit()?;
        let (wave, ws) = self
            .find(id)
            .ok_or_else(|| WaveError::UnknownStep(id.to_owned()))?;
        if self.any_halted()? {
            return Ok(StepResult::bare(StepOutcome::Halted(
                HaltReason::WaveHalted,
            )));
        }
        let base = match self.readiness(ws)? {
            Ok(base) => base,
            Err(reason) => return Ok(StepResult::bare(StepOutcome::NotReady(reason))),
        };
        if let Some(rec) = self.load(id)? {
            if rec.outcome == WaveOutcome::Passed && rec.facts.base.as_deref() == Some(&base) {
                return Ok(StepResult::bare(StepOutcome::Passed));
            }
        }
        match ws.role {
            AgentRole::Builder => self.run_builder(ws, wave, base, now),
            AgentRole::Critic => self.run_critic(ws, wave, base, now),
            AgentRole::Probe => self.run_probe(ws, wave, base, now),
            AgentRole::MainSession => Ok(StepResult {
                outcome: StepOutcome::AwaitingGate,
                event: Some(self.event(
                    ws,
                    wave,
                    WaveOutcome::AwaitingGate,
                    &Facts {
                        base: Some(base),
                        ..Facts::default()
                    },
                    now,
                )),
                receipt: None,
                compensation: Vec::new(),
            }),
        }
    }

    /// Run every step in plan order. The run stops at the first step that does not pass.
    pub fn run_local(&self, now: u64) -> Result<WaveReport, WaveError> {
        self.admit()?;
        let order: Vec<String> = self
            .all_steps()
            .into_iter()
            .map(|(_, ws)| ws.step.id.clone())
            .collect();
        let mut report = WaveReport::default();
        for id in order {
            let result = self.run_step(&id, now)?;
            if let Some(mut event) = result.event {
                set_sequence(&mut event, report.events.len() as u64);
                report.events.push(event);
            }
            report.compensation.extend(result.compensation);
            match result.outcome {
                StepOutcome::Passed => {}
                StepOutcome::AwaitingGate => {
                    report.awaiting_gate = Some(id);
                    break;
                }
                StepOutcome::NotReady(reason) => {
                    report.not_ready = Some(reason);
                    break;
                }
                StepOutcome::Halted(reason) => {
                    report.halted = Some(reason);
                    break;
                }
            }
        }
        report.receipts = self
            .records()?
            .into_iter()
            .filter(|r| r.role == AgentRole::Builder && r.outcome == WaveOutcome::Passed)
            .filter_map(|r| r.receipt)
            .collect();
        Ok(report)
    }

    fn run_builder(
        &self,
        ws: &WaveStep,
        wave: usize,
        base: String,
        now: u64,
    ) -> Result<StepResult, WaveError> {
        let id = ws.step.id.as_str();
        let repo = self.config.repo_dir.clone();
        let model = self.config.builder_model.as_str();
        let mut facts = Facts {
            base: Some(base.clone()),
            ..Facts::default()
        };

        let start = self.run(
            &repo,
            "blackwall",
            &[
                "start",
                "--task",
                id,
                "--provider",
                "claude",
                "--model",
                model,
            ],
            None,
        )?;
        let stderr = String::from_utf8_lossy(&start.stderr).into_owned();
        let (Some(run), Some(changeset)) = (
            token_after(&stderr, "(ok): hash "),
            token_after(&stderr, "changeset: "),
        ) else {
            return self.halt(
                ws,
                wave,
                HaltReason::BuilderRunFailed,
                facts,
                None,
                true,
                now,
            );
        };
        facts.run = Some(run.clone());
        facts.changeset = Some(changeset.clone());

        let settle = self.run(&repo, "blackwall", &["settle", &run, "select"], None)?;
        if settle.status.code() != Some(0) {
            return self.halt(
                ws,
                wave,
                HaltReason::BuilderRunFailed,
                facts,
                None,
                true,
                now,
            );
        }
        if self.any_halted()? {
            return self.halt(ws, wave, HaltReason::WaveHalted, facts, None, false, now);
        }

        let branch = format!("blackwall/{run}");
        facts.branch = Some(branch.clone());
        let input = LandingInput {
            slot_id: SlotId::new(format!("wave-{id}"))
                .map_err(|_| AdmitError::BadSlotId(id.to_owned()))?,
            run_hash: String::new(),
            audit: AuditStatus::NotAudited,
            cpu_cap: "uncapped".into(),
            blackwall_run: run.clone(),
            changeset_hash: changeset,
            base_sha: base.clone(),
            evaluator_touched: false,
            cost_source: CostSource::None,
            at: now,
        };
        // A Halted record can appear while this step waits for the lock: check again under it.
        let reconciled = {
            let _lock = self.lock()?;
            if self.any_halted()? {
                None
            } else {
                Some(self.landing().reconcile(&input)?)
            }
        };
        let Some(reconciled) = reconciled else {
            return self.halt(ws, wave, HaltReason::WaveHalted, facts, None, false, now);
        };
        let receipt = match reconciled {
            ReconcileOutcome::Reconciled(receipt) => receipt,
            ReconcileOutcome::VerifyFailed(_) => {
                return self.halt(ws, wave, HaltReason::VerifyFailed, facts, None, true, now)
            }
            ReconcileOutcome::NoVerifyGate(_) => {
                return self.halt(ws, wave, HaltReason::NoVerifyGate, facts, None, true, now)
            }
            ReconcileOutcome::StaleBase { .. } => {
                return self.halt(ws, wave, HaltReason::StaleBase, facts, None, true, now)
            }
        };

        let head = self.run(&repo, "git", &["rev-parse", &branch], None)?;
        if head.status.code() != Some(0) {
            return self.halt(
                ws,
                wave,
                HaltReason::BuilderRunFailed,
                facts,
                None,
                true,
                now,
            );
        }
        let head = String::from_utf8_lossy(&head.stdout).trim().to_owned();
        facts.head = Some(head.clone());

        let diff = self.run(
            &repo,
            "git",
            &["diff", "--name-only", "-z", "--no-renames", &base, &head],
            None,
        )?;
        if diff.status.code() != Some(0) {
            return self.halt(
                ws,
                wave,
                HaltReason::BuilderRunFailed,
                facts,
                None,
                true,
                now,
            );
        }
        let touched = split_nul(&diff.stdout);
        if touched
            .iter()
            .any(|p| is_evaluator_path(p) || is_critic_input_path(p))
        {
            let mut tiered = receipt.facts().clone();
            tiered.evaluator_touched = true;
            let tiered = LandingReceipt::new(tiered).map_err(LandingError::Receipt)?;
            return self.halt(
                ws,
                wave,
                HaltReason::EvaluatorTouched,
                facts,
                Some(tiered),
                true,
                now,
            );
        }
        self.passed(ws, wave, facts, Some(receipt), now)
    }

    fn run_critic(
        &self,
        ws: &WaveStep,
        wave: usize,
        base: String,
        now: u64,
    ) -> Result<StepResult, WaveError> {
        let id = ws.step.id.as_str();
        let repo = self.config.repo_dir.clone();
        let mut last = Facts {
            base: Some(base),
            ..Facts::default()
        };
        for builder in self.builder_deps(ws) {
            let Some(mut brec) = self.load(&builder)? else {
                return self.halt(ws, wave, HaltReason::CriticUnparsed, last, None, true, now);
            };
            let (Some(head), Some(receipt), Some(cbase)) = (
                brec.facts.head.clone(),
                brec.receipt.clone(),
                brec.facts.base.clone(),
            ) else {
                return self.halt(ws, wave, HaltReason::CriticUnparsed, last, None, true, now);
            };
            last = brec.facts.clone();
            // The Critic reads the rules at the base its Builder ran on (spec 0025 D17).
            let prompt = self.critic_prompt(id, &builder, &cbase, &brec.facts, &receipt)?;
            let tree = self.config.scratch_dir.join(format!("{id}-{builder}"));
            let session = {
                let _lock = self.lock()?;
                if self.any_halted()? {
                    Session::Halted
                } else {
                    let result = self.critic_session(&tree, &head, &cbase, &prompt);
                    let tree_arg = tree.to_string_lossy().into_owned();
                    let _ = self.run(
                        &repo,
                        "git",
                        &["worktree", "remove", "--force", &tree_arg],
                        None,
                    );
                    Session::Ran(result?)
                }
            };
            let out = match session {
                Session::Halted => {
                    return self.halt(ws, wave, HaltReason::WaveHalted, last, None, false, now)
                }
                Session::Ran(out) => out,
            };
            let verdict = match out {
                None => CriticVerdict::Unparsed,
                Some(out) => {
                    parse_critic_verdict(&String::from_utf8_lossy(&out.stdout), out.status.code())
                }
            };
            match verdict {
                CriticVerdict::Defects => {
                    return self.halt(ws, wave, HaltReason::CriticDefects, last, None, true, now)
                }
                CriticVerdict::Unparsed => {
                    return self.halt(ws, wave, HaltReason::CriticUnparsed, last, None, true, now)
                }
                CriticVerdict::NoDefects => {}
            }
            if self.config.open_prs && brec.facts.pr.is_none() {
                // The halt check, `blackwall pr` and the record save share one lock hold. A
                // Halted record queued on the lock lands first or waits for the save. The guard
                // drops before `halt`, which takes the lock again.
                let opened = {
                    let _lock = self.lock()?;
                    if self.any_halted()? {
                        None
                    } else {
                        let result = self.landing().open_pr(&receipt);
                        if let Ok(opened) = &result {
                            brec.facts.pr = opened.facts().pr_number;
                            brec.receipt = Some(opened.clone());
                            self.save(&brec)?;
                        }
                        Some(result)
                    }
                };
                match opened {
                    None => {
                        return self.halt(ws, wave, HaltReason::WaveHalted, last, None, false, now)
                    }
                    Some(Ok(_)) => last = brec.facts.clone(),
                    Some(Err(_)) => {
                        return self.halt(ws, wave, HaltReason::PrFailed, last, None, true, now)
                    }
                }
            }
        }
        self.passed(ws, wave, last, None, now)
    }

    /// Run the Critic in a transient worktree at `head`. The caller holds the lock and removes
    /// the worktree. `None` means the worktree or the base checkout failed.
    fn critic_session(
        &self,
        tree: &Path,
        head: &str,
        base: &str,
        prompt: &str,
    ) -> Result<Option<Output>, WaveError> {
        fs::create_dir_all(&self.config.scratch_dir).map_err(|source| WaveError::Io {
            path: self.config.scratch_dir.clone(),
            source,
        })?;
        let tree_arg = tree.to_string_lossy().into_owned();
        let repo = self.config.repo_dir.clone();
        let add = self.run(
            &repo,
            "git",
            &["worktree", "add", "--detach", &tree_arg, head],
            None,
        )?;
        if add.status.code() != Some(0) {
            return Ok(None);
        }
        let checkout = self.run(
            tree,
            "git",
            &["checkout", "--no-overlay", base, "--", ".claude"],
            None,
        )?;
        if checkout.status.code() != Some(0) {
            return Ok(None);
        }
        let claude = self.run(
            tree,
            "claude",
            &[
                "-p",
                "--strict-mcp-config",
                "--agent",
                self.config.critic_agent.as_str(),
                "--allowed-tools",
                "Read",
                "Grep",
                "Glob",
                "Bash(git diff *)",
                "Bash(git log *)",
                "Bash(git show *)",
                "Bash(make *)",
                "Bash(cargo *)",
            ],
            Some(prompt.as_bytes()),
        )?;
        Ok(Some(claude))
    }

    fn run_probe(
        &self,
        ws: &WaveStep,
        wave: usize,
        base: String,
        now: u64,
    ) -> Result<StepResult, WaveError> {
        let id = ws.step.id.as_str();
        let facts = Facts {
            base: Some(base.clone()),
            ..Facts::default()
        };
        let tree = self.config.scratch_dir.join(id);
        let passed = {
            let _lock = self.lock()?;
            if self.any_halted()? {
                None
            } else {
                let result = self.probe_session(&tree, &base, &ws.step.verify_cmd);
                let tree_arg = tree.to_string_lossy().into_owned();
                let repo = self.config.repo_dir.clone();
                let _ = self.run(
                    &repo,
                    "git",
                    &["worktree", "remove", "--force", &tree_arg],
                    None,
                );
                Some(result?)
            }
        };
        let Some(passed) = passed else {
            return self.halt(ws, wave, HaltReason::WaveHalted, facts, None, false, now);
        };
        if passed {
            self.passed(ws, wave, facts, None, now)
        } else {
            self.halt(ws, wave, HaltReason::ProbeFailed, facts, None, true, now)
        }
    }

    /// Run the Probe command in a transient worktree at `base`. The caller holds the lock.
    fn probe_session(&self, tree: &Path, base: &str, verify: &[String]) -> Result<bool, WaveError> {
        fs::create_dir_all(&self.config.scratch_dir).map_err(|source| WaveError::Io {
            path: self.config.scratch_dir.clone(),
            source,
        })?;
        let tree_arg = tree.to_string_lossy().into_owned();
        let repo = self.config.repo_dir.clone();
        let add = self.run(
            &repo,
            "git",
            &["worktree", "add", "--detach", &tree_arg, base],
            None,
        )?;
        if add.status.code() != Some(0) {
            return Ok(false);
        }
        let Some((program, args)) = verify.split_first() else {
            return Ok(false);
        };
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.run(tree, program, &args, None)?;
        Ok(out.status.code() == Some(0))
    }

    /// Check the readiness rows. `Ok(Ok(base))` when the step may run.
    fn readiness(&self, ws: &WaveStep) -> Result<Result<String, NotReady>, WaveError> {
        let init = self.config.repo_dir.join(".blackwall").join("init-head");
        let Ok(text) = fs::read_to_string(&init) else {
            return Ok(Err(NotReady::NoInitHead));
        };
        let base = text.trim().to_owned();
        let repo = self.config.repo_dir.clone();
        let head = self.run(&repo, "git", &["rev-parse", "HEAD"], None)?;
        if head.status.code() != Some(0) || String::from_utf8_lossy(&head.stdout).trim() != base {
            return Ok(Err(NotReady::HeadDiffers));
        }
        for dep in &ws.step.deps {
            if self.role_of(dep) == Some(AgentRole::MainSession) {
                continue;
            }
            match self.load(dep)? {
                Some(rec) if rec.outcome == WaveOutcome::Passed => {}
                _ => return Ok(Err(NotReady::DepRecordMissing(dep.clone()))),
            }
        }
        if ws.role == AgentRole::Builder {
            for up in self.builder_closure(&ws.step.deps) {
                let Some(mut rec) = self.load(&up)? else {
                    return Ok(Err(NotReady::DepRecordMissing(up)));
                };
                // Merged means: with a PR, a recorded merge commit in the base (D15, D7); with
                // no PR (D16 off), a head in the base.
                let (named, tip) = match rec.facts.pr {
                    None => (format!("{up} (no PR)"), rec.facts.head.clone()),
                    Some(pr) => {
                        if rec.facts.merge_commit.is_none() {
                            self.preread_merge_commit(&up, pr)?;
                            rec = self.load(&up)?.unwrap_or(rec);
                        }
                        (format!("{up} (PR {pr})"), rec.facts.merge_commit.clone())
                    }
                };
                // The reason carries git's exit code. Exit 128 means git does not hold the
                // commit: the checkout needs a fetch.
                let reason = match tip {
                    Some(tip) => {
                        let out = self.run(
                            &repo,
                            "git",
                            &["merge-base", "--is-ancestor", &tip, &base],
                            None,
                        )?;
                        match out.status.code() {
                            Some(0) => None,
                            Some(128) => Some(format!(
                                "{named}: git merge-base exit 128; run git fetch origin"
                            )),
                            Some(code) => Some(format!("{named}: git merge-base exit {code}")),
                            None => Some(format!("{named}: git merge-base ended by a signal")),
                        }
                    }
                    None => Some(named),
                };
                if let Some(reason) = reason {
                    return Ok(Err(NotReady::UpstreamNotMerged(reason)));
                }
            }
        }
        Ok(Ok(base))
    }

    /// Read the merge commit of an upstream Builder's PR (spec 0025 D15). Forty lowercase hex
    /// characters go to its record and receipt; anything else records nothing. The read runs
    /// outside the lock, and the load, set and save run under it.
    fn preread_merge_commit(&self, up: &str, pr: u64) -> Result<(), WaveError> {
        let repo = self.config.repo_dir.clone();
        let pr = pr.to_string();
        let out = self.run(
            &repo,
            "gh",
            &[
                "pr",
                "view",
                &pr,
                "--json",
                "mergeCommit",
                "--jq",
                ".mergeCommit.oid",
            ],
            None,
        )?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let hex = text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if out.status.code() != Some(0) || !hex {
            return Ok(());
        }
        let _lock = self.lock()?;
        if let Some(mut rec) = self.load(up)? {
            if rec.facts.merge_commit.is_none() {
                rec.facts.merge_commit = Some(text.clone());
                rec.receipt = rec.receipt.map(|r| r.with_merge_commit(text));
                self.save(&rec)?;
            }
        }
        Ok(())
    }

    /// Record a pass, and return the event. A Passed record also serves as the dependency record.
    fn passed(
        &self,
        ws: &WaveStep,
        wave: usize,
        facts: Facts,
        receipt: Option<LandingReceipt>,
        now: u64,
    ) -> Result<StepResult, WaveError> {
        {
            // The sequence number and the write share one lock hold, so two steps never tie.
            let _lock = self.lock()?;
            let rec = StepRecord {
                step: ws.step.id.clone(),
                role: ws.role,
                outcome: WaveOutcome::Passed,
                facts: facts.clone(),
                receipt: receipt.clone(),
                at: now,
                seq: self.next_seq()?,
            };
            self.save(&rec)?;
        }
        Ok(StepResult {
            outcome: StepOutcome::Passed,
            event: Some(self.event(ws, wave, WaveOutcome::Passed, &facts, now)),
            receipt,
            compensation: Vec::new(),
        })
    }

    /// Record a halt. When `compensate` is set, compensate the passed records, newest first.
    #[allow(clippy::too_many_arguments)]
    fn halt(
        &self,
        ws: &WaveStep,
        wave: usize,
        reason: HaltReason,
        facts: Facts,
        receipt: Option<LandingReceipt>,
        compensate: bool,
        now: u64,
    ) -> Result<StepResult, WaveError> {
        let outcome = WaveOutcome::Halted {
            reason: reason.name().to_owned(),
        };
        {
            let _lock = self.lock()?;
            let rec = StepRecord {
                step: ws.step.id.clone(),
                role: ws.role,
                outcome: outcome.clone(),
                facts: facts.clone(),
                receipt: receipt.clone(),
                at: now,
                seq: self.next_seq()?,
            };
            self.save(&rec)?;
        }
        let compensation = if compensate {
            self.compensate_passed(reason.name(), now)?
        } else {
            Vec::new()
        };
        Ok(StepResult {
            outcome: StepOutcome::Halted(reason),
            event: Some(self.event(ws, wave, outcome, &facts, now)),
            receipt,
            compensation,
        })
    }

    fn compensate_passed(
        &self,
        reason: &str,
        now: u64,
    ) -> Result<Vec<CompensationReceipt>, WaveError> {
        let mut passed: Vec<StepRecord> = self
            .records()?
            .into_iter()
            .filter(|r| r.outcome == WaveOutcome::Passed)
            .collect();
        passed.sort_by(|a, b| (b.seq, b.at, &b.step).cmp(&(a.seq, a.at, &a.step)));
        let compensator = WaveCompensator {
            config: self.config.clone(),
            records: passed.clone(),
            now,
        };
        let mut out = Vec::new();
        for rec in &passed {
            let activity = Activity::new(rec.step.clone(), rec.step.clone(), ActivityKind::Impl);
            match compensator.compensate(&activity, reason) {
                Ok(mut receipts) => out.append(&mut receipts),
                Err(err) => out.push(CompensationReceipt::new(
                    rec.step.clone(),
                    "wave_builder",
                    false,
                    err.to_string(),
                    now,
                )),
            }
        }
        Ok(out)
    }

    fn event(
        &self,
        ws: &WaveStep,
        wave: usize,
        outcome: WaveOutcome,
        facts: &Facts,
        now: u64,
    ) -> WorkflowEvent {
        let persona = match ws.role {
            AgentRole::Builder => Some(BUILDER_PERSONA.to_owned()),
            AgentRole::Critic => Some(self.config.critic_agent.clone()),
            AgentRole::Probe | AgentRole::MainSession => None,
        };
        WorkflowEvent::WaveStep {
            sequence: 0,
            step: ws.step.id.clone(),
            wave: u32::try_from(wave).unwrap_or(u32::MAX),
            role: ws.role,
            persona,
            outcome,
            run: facts.run.clone(),
            changeset: facts.changeset.clone(),
            base: facts.base.clone(),
            branch: facts.branch.clone(),
            head: facts.head.clone(),
            pr: facts.pr,
            timestamp: now,
        }
    }

    fn critic_prompt(
        &self,
        critic: &str,
        builder: &str,
        base: &str,
        facts: &Facts,
        receipt: &LandingReceipt,
    ) -> Result<String, WaveError> {
        let path = self.task_dir(builder).join("prompt.md");
        let task = fs::read_to_string(&path).map_err(|source| WaveError::Io {
            path: path.clone(),
            source,
        })?;
        let json = serde_json::to_string_pretty(receipt).map_err(LandingError::Serialize)?;
        Ok(format!(
            "Review the Builder step {builder} as the critic step {critic}.\n\
             Base: {base}\n\
             Branch: {branch}\n\
             Head: {head}\n\
             Run: {run}\n\
             Reconcile receipt:\n{json}\n\
             Task prompt, from tasks/{builder}/prompt.md:\n{task}\n\
             Reply with VERDICT: NO DEFECTS or VERDICT: DEFECTS on the first line.\n",
            branch = facts.branch.as_deref().unwrap_or(""),
            head = facts.head.as_deref().unwrap_or(""),
            run = facts.run.as_deref().unwrap_or(""),
        ))
    }

    fn admit_task_model(&self, id: &str) -> Result<(), AdmitError> {
        #[derive(Deserialize)]
        struct TaskFile {
            model: Option<String>,
        }
        let path = self.task_dir(id).join("task.toml");
        let text = fs::read_to_string(&path).map_err(|_| AdmitError::TaskModelAbsent(id.into()))?;
        let task: TaskFile =
            toml::from_str(&text).map_err(|_| AdmitError::TaskModelAbsent(id.into()))?;
        match task.model {
            None => Err(AdmitError::TaskModelAbsent(id.into())),
            Some(found) if found != self.config.builder_model => {
                Err(AdmitError::TaskModelMismatch {
                    step: id.into(),
                    found,
                })
            }
            Some(_) => Ok(()),
        }
    }

    fn task_dir(&self, id: &str) -> PathBuf {
        self.config.repo_dir.join("tasks").join(id)
    }

    fn all_steps(&self) -> Vec<(usize, &WaveStep)> {
        self.plan
            .waves
            .iter()
            .flat_map(|w| w.steps.iter().map(move |s| (w.wave_index, s)))
            .collect()
    }

    fn find(&self, id: &str) -> Option<(usize, &WaveStep)> {
        self.all_steps().into_iter().find(|(_, s)| s.step.id == id)
    }

    fn role_of(&self, id: &str) -> Option<AgentRole> {
        self.find(id).map(|(_, s)| s.role)
    }

    fn builder_deps(&self, ws: &WaveStep) -> Vec<String> {
        ws.step
            .deps
            .iter()
            .filter(|d| self.role_of(d) == Some(AgentRole::Builder))
            .cloned()
            .collect()
    }

    /// Every Builder in the dependency closure of `deps`.
    fn builder_closure(&self, deps: &[String]) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<String> = deps.to_vec();
        let mut out = Vec::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some((_, ws)) = self.find(&id) else {
                continue;
            };
            if ws.role == AgentRole::Builder {
                out.push(id.clone());
            }
            stack.extend(ws.step.deps.iter().cloned());
        }
        out
    }

    fn landing(&self) -> LandingActivity {
        LandingActivity::new(LandingConfig {
            repo_dir: self.config.repo_dir.clone(),
            development_ref: self.config.development_ref.clone(),
            pr_base: self.config.pr_base.clone(),
            path_env: self.config.path_env.clone(),
        })
    }

    /// Take the host lock. The lock releases when the returned file drops.
    fn lock(&self) -> Result<File, WaveError> {
        if let Some(dir) = self.config.lock_path.parent() {
            fs::create_dir_all(dir).map_err(|source| WaveError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.config.lock_path)
            .map_err(|source| WaveError::Io {
                path: self.config.lock_path.clone(),
                source,
            })?;
        file.lock().map_err(|source| WaveError::Io {
            path: self.config.lock_path.clone(),
            source,
        })?;
        Ok(file)
    }

    fn run(
        &self,
        cwd: &Path,
        program: &str,
        args: &[&str],
        stdin: Option<&[u8]>,
    ) -> Result<Output, WaveError> {
        run_cmd(self.config.path_env.as_ref(), cwd, program, args, stdin)
    }

    fn state_path(&self, step: &str) -> PathBuf {
        self.config.state_dir.join(format!("{step}.json"))
    }

    fn save(&self, rec: &StepRecord) -> Result<(), WaveError> {
        fs::create_dir_all(&self.config.state_dir).map_err(|source| WaveError::Io {
            path: self.config.state_dir.clone(),
            source,
        })?;
        let path = self.state_path(&rec.step);
        let bytes = serde_json::to_vec_pretty(rec).map_err(|source| WaveError::Record {
            path: path.clone(),
            source,
        })?;
        // Write a sibling temp file, then rename it over the record. A reader sees the old
        // record or the new one, never a truncated one. `records` reads `.json` files only.
        let tmp = self.config.state_dir.join(format!("{}.tmp", rec.step));
        let write = File::create(&tmp).and_then(|mut f| {
            f.write_all(&bytes)?;
            f.sync_all()
        });
        write
            .and_then(|()| fs::rename(&tmp, &path))
            .map_err(|source| {
                let _ = fs::remove_file(&tmp);
                WaveError::Io { path, source }
            })
    }

    fn load(&self, step: &str) -> Result<Option<StepRecord>, WaveError> {
        let path = self.state_path(step);
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|source| WaveError::Record { path, source }),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(WaveError::Io { path, source }),
        }
    }

    fn records(&self) -> Result<Vec<StepRecord>, WaveError> {
        let entries = match fs::read_dir(&self.config.state_dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(WaveError::Io {
                    path: self.config.state_dir.clone(),
                    source,
                })
            }
        };
        let mut out = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|source| WaveError::Io {
                    path: self.config.state_dir.clone(),
                    source,
                })?
                .path();
            if path.extension().is_some_and(|e| e == "json") {
                let bytes = fs::read(&path).map_err(|source| WaveError::Io {
                    path: path.clone(),
                    source,
                })?;
                let rec: StepRecord = serde_json::from_slice(&bytes)
                    .map_err(|source| WaveError::Record { path, source })?;
                out.push(rec);
            }
        }
        // Directory order is arbitrary: read the records oldest first, ties broken on time and id.
        out.sort_by(|a, b| (a.seq, a.at, &a.step).cmp(&(b.seq, b.at, &b.step)));
        Ok(out)
    }

    /// One past the largest sequence in the state dir.
    fn next_seq(&self) -> Result<u64, WaveError> {
        Ok(self
            .records()?
            .iter()
            .map(|r| r.seq)
            .max()
            .map_or(0, |m| m + 1))
    }

    fn any_halted(&self) -> Result<bool, WaveError> {
        Ok(self
            .records()?
            .iter()
            .any(|r| matches!(r.outcome, WaveOutcome::Halted { .. })))
    }
}

/// Compensates the Builder records of a halted wave (spec 0025 Compensation, spec 0009).
pub(crate) struct WaveCompensator {
    config: WaveExecConfig,
    records: Vec<StepRecord>,
    now: u64,
}

impl Compensator for WaveCompensator {
    fn name(&self) -> &'static str {
        "wave_builder"
    }

    fn compensate(
        &self,
        activity: &Activity,
        reason: &str,
    ) -> Result<Vec<CompensationReceipt>, EngineError> {
        let Some(rec) = self.records.iter().find(|r| r.step == activity.id) else {
            return Ok(Vec::new());
        };
        if rec.role != AgentRole::Builder {
            return Ok(Vec::new());
        }
        let (success, details) = self.undo(rec);
        Ok(vec![CompensationReceipt::new(
            activity.id.clone(),
            self.name(),
            success,
            format!("{reason}: {details}"),
            self.now,
        )])
    }
}

impl WaveCompensator {
    /// The four Builder steps of spec 0025. Each failure is a receipt, never an error.
    fn undo(&self, rec: &StepRecord) -> (bool, String) {
        if let Some(pr) = rec.facts.pr {
            return (true, format!("kept PR {pr}"));
        }
        let Some(run) = rec.facts.run.as_deref() else {
            return (false, "no run recorded".into());
        };
        let branch = format!("blackwall/{run}");
        let repo = self.config.repo_dir.clone();
        let remote = run_cmd(
            self.config.path_env.as_ref(),
            &repo,
            "git",
            &["ls-remote", "--exit-code", "--heads", "origin", &branch],
            None,
        );
        match remote.map(|o| o.status.code()) {
            Ok(Some(0)) => (true, format!("kept {branch}: on origin")),
            Ok(Some(2)) => {
                match run_cmd(
                    self.config.path_env.as_ref(),
                    &repo,
                    "git",
                    &["branch", "-D", &branch],
                    None,
                ) {
                    Ok(out) if out.status.code() == Some(0) => (true, format!("deleted {branch}")),
                    Ok(out) => (
                        false,
                        format!(
                            "branch -D {branch} failed: {}",
                            String::from_utf8_lossy(&out.stderr).trim()
                        ),
                    ),
                    Err(err) => (false, err.to_string()),
                }
            }
            Ok(code) => (false, format!("kept {branch}: ls-remote exited {code:?}")),
            Err(err) => (false, format!("kept {branch}: {err}")),
        }
    }
}

fn run_cmd(
    path_env: Option<&OsString>,
    cwd: &Path,
    program: &str,
    args: &[&str],
    stdin: Option<&[u8]>,
) -> Result<Output, WaveError> {
    let process = |source| WaveError::Process {
        program: program.to_owned(),
        source,
    };
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = path_env {
        cmd.env("PATH", path);
    }
    let mut child = cmd.spawn().map_err(process)?;
    if let (Some(bytes), Some(mut handle)) = (stdin, child.stdin.take()) {
        match handle.write_all(bytes) {
            Ok(()) => {}
            // A child that exits before it reads its prompt is judged by its exit code.
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {}
            Err(source) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(process(source));
            }
        }
        drop(handle);
    }
    child.wait_with_output().map_err(process)
}

/// The token of `width` characters that follows the first `marker` in `text`.
fn token_after(text: &str, marker: &str) -> Option<String> {
    let line = text.lines().find(|l| l.contains(marker))?;
    let start = line.find(marker)? + marker.len();
    let token: String = line[start..].chars().take(12).collect();
    (token.chars().count() == 12).then_some(token)
}

/// Split NUL-terminated `git diff -z` output into raw paths. Non-ASCII names stay bytes.
fn split_nul(out: &[u8]) -> Vec<PathBuf> {
    out.split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| PathBuf::from(OsStr::from_bytes(s)))
        .collect()
}

fn set_sequence(event: &mut WorkflowEvent, value: u64) {
    if let WorkflowEvent::WaveStep { sequence, .. } = event {
        *sequence = value;
    }
}
