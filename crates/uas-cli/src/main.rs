// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `uas` command line entry point.

mod memory;
mod wave_flow;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;
use uas_domain::{
    Attestation, Budget, EvidenceTier, Lease, LeaseId, LeaseLedger, LeaseToken, PolicyTier,
    PrincipalRef, Slot, SlotId, SlotState, SomaClassification, WorktreeRef,
};
use uas_engine::{
    Activity, ActivityKind, ActivityResult, BranchPruneCompensator, CompensationReceipt,
    CompositeCompensator, CpmSchedule, EngineError, InMemoryEventStore, LeaseReturnCompensator,
    SagaFlowCoordinator, SagaFlowDef, SessionId, SessionState, SomaAdmissionGate, WaveCoordinator,
    WavePlan, WorkflowEvent, WorkflowSession, WorktreeCleanupCompensator,
    DEFAULT_MAX_WAVE_CONCURRENCY,
};

use memory::InMemoryLedger;

/// Version text: crate version plus the git revision stamped by `build.rs`.
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("UAS_BUILD_REV"), ")");

/// Fixed demo clock, in epoch seconds. The domain never reads a clock.
const DEMO_START: u64 = 1_000;
/// Demo lease length, in seconds.
const DEMO_TTL: u64 = 300;

/// Unsigned Agent Substrate command line tool.
#[derive(Debug, Parser)]
#[command(name = "uas", version = VERSION, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Walk a slot through its lifecycle with an in-memory ledger and print JSON.
    Demo {
        /// Id for the slot.
        #[arg(long, default_value = "slot-1")]
        slot: String,
    },
    /// Manage harness execution, session lifecycle, and receipts.
    Harness {
        #[command(subcommand)]
        command: HarnessCommand,
    },
    /// Plan saga flows per wave and run wave steps on the custody chain (spec 0025).
    #[command(after_help = DEPRECATION_NOTICE)]
    Wave {
        #[command(subcommand)]
        command: WaveCommand,
    },
}

/// The deprecation line for `uas wave` and the CPM flags of `harness run` (spec 0049 D10).
/// Shown in `uas wave --help` and written to stderr when a deprecated path runs.
const DEPRECATION_NOTICE: &str = "deprecated: moves to saga (spec 0049); fixes only";

/// `--open-prs` and `UAS_WAVE_OPEN_PRS`: only the value `1` is true (spec 0025 D16).
#[allow(clippy::unnecessary_wraps)]
fn open_prs_value(raw: &str) -> Result<bool, String> {
    Ok(raw == "1")
}

#[derive(Debug, Subcommand)]
enum WaveCommand {
    /// Write one saga flow per wave from a plan.
    #[command(after_help = DEPRECATION_NOTICE)]
    Flow {
        /// The plan: a saga flow file with `[[step]]` entries.
        #[arg(long)]
        from: String,
        /// The directory that receives `uas-cpm-wave-w<i>.toml`.
        #[arg(long)]
        out_dir: std::path::PathBuf,
        /// The most steps in flight. 0 means the default of 3.
        #[arg(long, default_value_t = 0)]
        max_concurrency: usize,
    },
    /// Run one step of a wave. Saga calls this as the `verify_cmd` of a generated step.
    #[command(after_help = DEPRECATION_NOTICE)]
    Step {
        /// The plan, as given to `wave flow --from`.
        #[arg(long)]
        flow: String,
        /// The step id.
        #[arg(long)]
        step: String,
        /// The custody checkout. Defaults to `UAS_WAVE_REPO`, then the working directory.
        #[arg(long, env = "UAS_WAVE_REPO")]
        repo: Option<std::path::PathBuf>,
        /// The state directory. Defaults to `UAS_WAVE_STATE_DIR`, then under `XDG_STATE_HOME`.
        #[arg(long, env = "UAS_WAVE_STATE_DIR")]
        state_dir: Option<std::path::PathBuf>,
        /// The most steps in flight, as given to `wave flow`. 0 means the default of 3.
        #[arg(long, default_value_t = 0)]
        max_concurrency: usize,
        /// Open a pull request per Builder (spec 0025 D16). `UAS_WAVE_OPEN_PRS=1` does the same;
        /// the flag wins.
        #[arg(
            long,
            env = "UAS_WAVE_OPEN_PRS",
            num_args = 0..=1,
            require_equals = true,
            default_missing_value = "1",
            value_name = "0|1",
            value_parser = open_prs_value,
        )]
        open_prs: Option<bool>,
    },
    /// Resolve a Builder's PR and run `blackwall merge-gate` on it. Saga calls this as the
    /// `verify_cmd` of a generated merge step.
    #[command(after_help = DEPRECATION_NOTICE)]
    MergeGate {
        /// The plan, as given to `wave flow --from`.
        #[arg(long)]
        flow: String,
        /// The Builder step id.
        #[arg(long)]
        builder: String,
        /// The custody checkout. Required unless `UAS_WAVE_REPO` is set; never the working directory.
        #[arg(long, env = "UAS_WAVE_REPO")]
        repo: Option<std::path::PathBuf>,
        /// The state directory. Defaults to `UAS_WAVE_STATE_DIR`, then under `XDG_STATE_HOME`.
        #[arg(long, env = "UAS_WAVE_STATE_DIR")]
        state_dir: Option<std::path::PathBuf>,
        /// The most steps in flight, as given to `wave flow`. 0 means the default of 3.
        #[arg(long, default_value_t = 0)]
        max_concurrency: usize,
    },
}

#[derive(Debug, Subcommand)]
enum HarnessCommand {
    /// Run an activity within a leased slot and emit an attestation receipt.
    Run {
        /// Slot identifier to allocate.
        #[arg(long, default_value = "slot-1")]
        slot: String,

        /// Session identifier (defaults to sess-<slot>).
        #[arg(long)]
        session: Option<String>,

        /// Path to a saga flow TOML file (.saga/flows/*.toml).
        #[arg(long)]
        flow: Option<String>,

        /// Task description or shell command to execute.
        #[arg(long)]
        task: Option<String>,

        /// Evidence tier (verified, reported, heuristic, asserted).
        /// Defaults to verified for real runs, heuristic for dry-run.
        #[arg(long)]
        evidence: Option<String>,

        /// Policy tier (tier-0, tier-1, tier-2).
        /// Defaults to tier-0 for dry-run, tier-1 for execution.
        #[arg(long)]
        policy_tier: Option<String>,

        /// Principal signer for the attestation.
        #[arg(long, default_value = "uas/harness")]
        signer: String,

        /// Dry-run mode: do not execute shell command, simulate success.
        #[arg(long)]
        dry_run: bool,

        /// Print or include Critical Path Method (CPM) schedule analysis for flow.
        /// Deprecated: moves to saga (spec 0049).
        #[arg(long)]
        show_cpm: bool,

        /// Execute flow organized by agent sprint waves.
        /// Deprecated: moves to saga (spec 0049).
        #[arg(long)]
        wave: bool,

        /// Maximum concurrent subagents per wave (defaults to 3).
        /// Deprecated: moves to saga (spec 0049).
        #[arg(long, default_value_t = DEFAULT_MAX_WAVE_CONCURRENCY)]
        max_concurrency: usize,
    },

    /// Verify a receipt file by replaying events and checking evidence tiers.
    Verify {
        /// Path to receipt JSON file.
        #[arg(long)]
        receipt: String,
    },
}

/// One step of the demo, as printed. The lease token is never printed.
#[derive(Debug, Serialize)]
struct Step {
    action: &'static str,
    slot_state: SlotState,
    lease_expires_at: Option<u64>,
}

/// The demo result: every step, the final state, and a completion flag.
#[derive(Debug, Serialize)]
struct Report {
    slot: String,
    steps: Vec<Step>,
    final_state: SlotState,
    done: bool,
}

/// Output report for a harness execution run.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HarnessReport {
    session_id: String,
    slot_id: String,
    lease_id: String,
    state: SessionState,
    done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    policy_tier: Option<PolicyTier>,
    attestation: Option<Attestation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    attestations: Vec<Attestation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    compensation_receipts: Vec<CompensationReceipt>,
    failure_reason: Option<String>,
    events: Vec<WorkflowEvent>,
    /// Deprecated: moves to saga (spec 0049); removed with the CPM surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cpm_schedule: Option<CpmSchedule>,
    /// Deprecated: moves to saga (spec 0049); removed with the wave surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wave_plan: Option<WavePlan>,
}

/// Output report for a harness receipt verification.
#[derive(Debug, Serialize, Deserialize)]
struct VerificationReport {
    valid: bool,
    session_id: String,
    slot_id: String,
    evidence: EvidenceTier,
    state: SessionState,
    event_count: usize,
    satisfies_gate: bool,
}

/// Keep the receipts of a rollback that had failed compensations, so the report still lists them.
fn keep_receipts(
    result: Result<Vec<CompensationReceipt>, EngineError>,
) -> Result<Vec<CompensationReceipt>, EngineError> {
    match result {
        Ok(receipts) => Ok(receipts),
        Err(EngineError::CompensationIncomplete { receipts, .. }) => Ok(receipts),
        Err(other) => Err(other),
    }
}

fn parse_evidence_tier(s: &str) -> Result<EvidenceTier> {
    match s.to_ascii_lowercase().as_str() {
        "verified" => Ok(EvidenceTier::Verified),
        "reported" => Ok(EvidenceTier::Reported),
        "heuristic" => Ok(EvidenceTier::Heuristic),
        "asserted" => Ok(EvidenceTier::Asserted),
        other => anyhow::bail!(
            "unknown evidence tier: '{other}'. Expected verified, reported, heuristic, or asserted"
        ),
    }
}

fn parse_policy_tier(s: &str) -> Result<PolicyTier> {
    match s.to_ascii_lowercase().as_str() {
        "tier-0" | "tier0" | "0" => Ok(PolicyTier::Tier0),
        "tier-1" | "tier1" | "1" => Ok(PolicyTier::Tier1),
        "tier-2" | "tier2" | "2" => Ok(PolicyTier::Tier2),
        other => {
            anyhow::bail!("unknown policy tier: '{other}'. Expected tier-0, tier-1, or tier-2")
        }
    }
}

fn current_epoch_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn demo(slot_id: &str) -> Result<Report> {
    let mut ledger = InMemoryLedger::default();
    let slot_id = SlotId::new(slot_id).context("invalid --slot")?;
    let lease_id = LeaseId::new("lease-1")?;
    let mut steps = Vec::new();
    let mut record = |action: &'static str, slot: &Slot, ledger: &InMemoryLedger| {
        steps.push(Step {
            action,
            slot_state: slot.state(),
            lease_expires_at: ledger.find(&lease_id).ok().map(|l| l.expires_at),
        });
    };

    let slot = Slot::new(slot_id.clone());
    record("create", &slot, &ledger);

    let slot = slot.warm(WorktreeRef::new(format!("worktrees/{slot_id}"))?)?;
    record("warm", &slot, &ledger);

    let slot = slot.lease()?;
    let token = LeaseToken::new("demo-token-not-a-secret")?;
    ledger.issue(Lease::new(
        lease_id.clone(),
        slot_id.clone(),
        PrincipalRef::new("agent-demo")?,
        token.clone(),
        DEMO_START,
        DEMO_START + DEMO_TTL,
        Budget::new(100_000, 600)?,
    )?)?;
    record("lease", &slot, &ledger);

    let slot = slot.suspend()?;
    record("suspend", &slot, &ledger);

    let slot = slot.resume()?;
    ledger.renew(&lease_id, &token, DEMO_START + 100, DEMO_TTL)?;
    record("resume", &slot, &ledger);

    let slot = slot.release()?;
    ledger.revoke(&lease_id)?;
    record("release", &slot, &ledger);

    tracing::debug!(steps = steps.len(), "demo finished");
    Ok(Report {
        slot: slot_id.to_string(),
        final_state: slot.state(),
        done: slot.state() == SlotState::Warm,
        steps,
    })
}

fn harness_run(
    slot: &str,
    session: Option<String>,
    task: &str,
    evidence_opt: Option<&str>,
    policy_tier_opt: Option<&str>,
    signer: &str,
    dry_run: bool,
) -> Result<HarnessReport> {
    let slot_id = SlotId::new(slot).context("invalid --slot")?;
    let sess_name = session.unwrap_or_else(|| format!("sess-{}", slot_id.as_str()));
    let session_id = SessionId::new(sess_name).context("invalid session id")?;
    let lease_id =
        LeaseId::new(format!("lease-{}", slot_id.as_str())).context("invalid lease id")?;
    let signer = PrincipalRef::new(signer).context("invalid --signer")?;

    let evidence = match (evidence_opt, dry_run) {
        (None, true) => EvidenceTier::Heuristic,
        (None, false) => EvidenceTier::Verified,
        (Some(raw), true) => {
            let tier = parse_evidence_tier(raw)?;
            if tier == EvidenceTier::Verified || tier == EvidenceTier::Reported {
                anyhow::bail!(
                    "a dry run executes nothing and cannot record {} tier evidence",
                    tier.as_str()
                );
            }
            tier
        }
        (Some(raw), false) => parse_evidence_tier(raw)?,
    };

    let policy_tier = match policy_tier_opt {
        Some(raw) => parse_policy_tier(raw)?,
        None => {
            if dry_run {
                PolicyTier::Tier0
            } else {
                PolicyTier::Tier1
            }
        }
    };

    let classification = SomaClassification::new(
        policy_tier,
        if dry_run { "simulation" } else { "execution" },
        "soma reflex admission check",
    )?;
    let admission_gate = SomaAdmissionGate::new(classification);

    let now = current_epoch_seconds();

    let compensator = CompositeCompensator::new()
        .with(WorktreeCleanupCompensator::new())
        .with(BranchPruneCompensator::new())
        .with(LeaseReturnCompensator::new());
    let mut workflow = WorkflowSession::with_store_and_compensator(
        session_id.clone(),
        InMemoryEventStore::default(),
        Box::new(compensator),
    );
    workflow.start(slot_id.clone(), lease_id.clone(), now)?;

    let activity_kind = if dry_run {
        ActivityKind::Verify
    } else {
        ActivityKind::Impl
    };
    let activity = Activity::new("act-1", task, activity_kind);

    if let Err(e) = admission_gate.admit_activity(&activity) {
        let reason = format!("policy admission rejected: {e}");
        workflow.fail_activity(&activity, reason.clone(), now)?;
        return Ok(HarnessReport {
            session_id: session_id.to_string(),
            slot_id: slot_id.to_string(),
            lease_id: lease_id.to_string(),
            state: workflow.state(),
            done: false,
            policy_tier: Some(policy_tier),
            attestation: None,
            attestations: Vec::new(),
            compensation_receipts: workflow.compensation_receipts().to_vec(),
            failure_reason: Some(reason),
            events: workflow.events().to_vec(),
            cpm_schedule: None,
            wave_plan: None,
        });
    }

    workflow.start_activity(activity.clone(), now)?;

    if dry_run {
        let attestation = Attestation {
            signer,
            evidence,
            receipt: format!("dry-run simulated: {task}"),
            at: now,
        };
        let result = ActivityResult {
            activity_id: activity.id.clone(),
            success: true,
            attestation: attestation.clone(),
            summary: Some("simulated dry-run execution".into()),
        };
        workflow.complete_activity(result, now)?;
        workflow.finish_success(now)?;

        Ok(HarnessReport {
            session_id: session_id.to_string(),
            slot_id: slot_id.to_string(),
            lease_id: lease_id.to_string(),
            state: workflow.state(),
            done: true,
            policy_tier: Some(policy_tier),
            attestation: Some(attestation.clone()),
            attestations: vec![attestation],
            compensation_receipts: Vec::new(),
            failure_reason: None,
            events: workflow.events().to_vec(),
            cpm_schedule: None,
            wave_plan: None,
        })
    } else {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(task)
            .output()
            .context("failed to execute harness task via shell")?;

        if output.status.success() {
            let receipt = format!(
                "command: {}, status: 0, stdout_bytes: {}, stderr_bytes: {}",
                task,
                output.stdout.len(),
                output.stderr.len()
            );
            let attestation = Attestation {
                signer,
                evidence,
                receipt,
                at: now,
            };
            let result = ActivityResult {
                activity_id: activity.id.clone(),
                success: true,
                attestation: attestation.clone(),
                summary: Some(String::from_utf8_lossy(&output.stdout).trim().into()),
            };
            workflow.complete_activity(result, now)?;
            workflow.finish_success(now)?;

            Ok(HarnessReport {
                session_id: session_id.to_string(),
                slot_id: slot_id.to_string(),
                lease_id: lease_id.to_string(),
                state: workflow.state(),
                done: true,
                policy_tier: Some(policy_tier),
                attestation: Some(attestation.clone()),
                attestations: vec![attestation],
                compensation_receipts: Vec::new(),
                failure_reason: None,
                events: workflow.events().to_vec(),
                cpm_schedule: None,
                wave_plan: None,
            })
        } else {
            let reason = format!(
                "exit code: {:?}, stderr: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
            workflow.fail_activity(&activity, reason.clone(), now)?;

            Ok(HarnessReport {
                session_id: session_id.to_string(),
                slot_id: slot_id.to_string(),
                lease_id: lease_id.to_string(),
                state: workflow.state(),
                done: false,
                policy_tier: Some(policy_tier),
                attestation: None,
                attestations: Vec::new(),
                compensation_receipts: workflow.compensation_receipts().to_vec(),
                failure_reason: Some(reason),
                events: workflow.events().to_vec(),
                cpm_schedule: None,
                wave_plan: None,
            })
        }
    }
}

/// Execution options for running a saga flow through the harness.
#[derive(Debug, Clone, Copy)]
pub struct FlowRunOptions<'a> {
    /// Evidence tier override.
    pub evidence: Option<&'a str>,
    /// Policy tier override.
    pub policy_tier: Option<&'a str>,
    /// Signer principal identifier.
    pub signer: &'a str,
    /// Whether to run in simulation dry-run mode.
    pub dry_run: bool,
    /// Whether to calculate and include the CPM schedule.
    pub show_cpm: bool,
    /// Whether to execute activities organized by concurrent waves.
    pub wave: bool,
    /// Maximum concurrent activities allowed per wave.
    pub max_concurrency: usize,
}

impl<'a> Default for FlowRunOptions<'a> {
    fn default() -> Self {
        Self {
            evidence: None,
            policy_tier: None,
            signer: "uas/test",
            dry_run: false,
            show_cpm: false,
            wave: false,
            max_concurrency: DEFAULT_MAX_WAVE_CONCURRENCY,
        }
    }
}

fn harness_run_flow(
    slot: &str,
    session: Option<String>,
    flow_path: &str,
    opts: &FlowRunOptions<'_>,
) -> Result<HarnessReport> {
    let slot_id = SlotId::new(slot).context("invalid --slot")?;
    let sess_name = session.unwrap_or_else(|| format!("sess-{}", slot_id.as_str()));
    let session_id = SessionId::new(sess_name).context("invalid session id")?;
    let lease_id =
        LeaseId::new(format!("lease-{}", slot_id.as_str())).context("invalid lease id")?;
    let signer = PrincipalRef::new(opts.signer).context("invalid --signer")?;

    let evidence = match (opts.evidence, opts.dry_run) {
        (None, true) => EvidenceTier::Heuristic,
        (None, false) => EvidenceTier::Verified,
        (Some(raw), true) => {
            let tier = parse_evidence_tier(raw)?;
            if tier == EvidenceTier::Verified || tier == EvidenceTier::Reported {
                anyhow::bail!(
                    "a dry run executes nothing and cannot record {} tier evidence",
                    tier.as_str()
                );
            }
            tier
        }
        (Some(raw), false) => parse_evidence_tier(raw)?,
    };

    let flow_content = std::fs::read_to_string(flow_path)
        .with_context(|| format!("failed to read flow file: {flow_path}"))?;
    let flow_def = SagaFlowDef::from_toml(&flow_content)
        .with_context(|| format!("failed to parse flow TOML from {flow_path}"))?;

    let steps = flow_def.to_steps();
    let default_tier = if steps.iter().any(|s| s.kind == ActivityKind::Gate) {
        PolicyTier::Tier2
    } else if steps.iter().any(|s| s.kind == ActivityKind::Impl) {
        PolicyTier::Tier1
    } else {
        PolicyTier::Tier0
    };

    let policy_tier = match opts.policy_tier {
        Some(raw) => parse_policy_tier(raw)?,
        None => default_tier,
    };

    let classification = SomaClassification::new(
        policy_tier,
        if opts.dry_run {
            "simulation"
        } else {
            "execution"
        },
        format!(
            "soma reflex admission check for flow {}",
            flow_def.flow.name
        ),
    )?;
    let admission_gate = SomaAdmissionGate::new(classification);

    let coord_compensator = CompositeCompensator::new()
        .with(WorktreeCleanupCompensator::new())
        .with(BranchPruneCompensator::new())
        .with(LeaseReturnCompensator::new());

    let sess_compensator = CompositeCompensator::new()
        .with(WorktreeCleanupCompensator::new())
        .with(BranchPruneCompensator::new())
        .with(LeaseReturnCompensator::new());
    let mut workflow = WorkflowSession::with_store_and_compensator(
        session_id.clone(),
        InMemoryEventStore::default(),
        Box::new(sess_compensator),
    );

    let now = current_epoch_seconds();
    workflow.start(slot_id.clone(), lease_id.clone(), now)?;

    let cpm_schedule = if opts.show_cpm || opts.wave {
        Some(CpmSchedule::compute(&steps)?)
    } else {
        None
    };

    let wave_plan = if opts.wave {
        Some(WavePlan::plan(&steps, opts.max_concurrency)?)
    } else {
        None
    };

    let mut attestations = Vec::new();

    if let Some(plan) = wave_plan.clone() {
        let mut wave_coordinator = WaveCoordinator::new(plan, Box::new(coord_compensator));

        while let Some(current_wave) = wave_coordinator.current_wave().cloned() {
            for wave_step in &current_wave.steps {
                let step = &wave_step.step;
                let step_now = current_epoch_seconds();
                let activity = Activity::new(&step.id, &step.title, step.kind);

                if let Err(e) = admission_gate.admit_activity(&activity) {
                    let reason = format!("policy admission rejected for step {}: {e}", step.id);
                    workflow.fail_activity(&activity, reason.clone(), step_now)?;
                    let mut receipts = workflow.compensation_receipts().to_vec();
                    let rollback_receipts =
                        keep_receipts(wave_coordinator.rollback(&step.id, &reason))?;
                    receipts.extend(rollback_receipts);

                    return Ok(HarnessReport {
                        session_id: session_id.to_string(),
                        slot_id: slot_id.to_string(),
                        lease_id: lease_id.to_string(),
                        state: workflow.state(),
                        done: false,
                        policy_tier: Some(policy_tier),
                        attestation: attestations.last().cloned(),
                        attestations,
                        compensation_receipts: receipts,
                        failure_reason: Some(reason),
                        events: workflow.events().to_vec(),
                        cpm_schedule,
                        wave_plan: Some(wave_coordinator.plan().clone()),
                    });
                }

                workflow.start_activity(activity.clone(), step_now)?;

                if opts.dry_run {
                    let receipt = format!(
                        "dry-run simulated step: {} ({}) [role={:?}, wave={}]",
                        step.id, step.title, wave_step.role, current_wave.wave_index
                    );
                    let attestation = Attestation {
                        signer: signer.clone(),
                        evidence,
                        receipt,
                        at: step_now,
                    };
                    let result = ActivityResult {
                        activity_id: activity.id.clone(),
                        success: true,
                        attestation: attestation.clone(),
                        summary: Some(format!("simulated dry-run: {}", step.title)),
                    };
                    workflow.complete_activity(result, step_now)?;
                    wave_coordinator.mark_step_completed(
                        &step.id,
                        Some(format!("Step {} completed", step.id)),
                    )?;
                    attestations.push(attestation);
                } else if !step.verify_cmd.is_empty() {
                    let output = std::process::Command::new(&step.verify_cmd[0])
                        .args(&step.verify_cmd[1..])
                        .output()
                        .with_context(|| {
                            format!("failed to execute verify_cmd for step {}", step.id)
                        })?;

                    if output.status.success() {
                        let receipt = format!(
                            "step: {}, command: {:?}, status: 0, stdout_bytes: {}, stderr_bytes: {}",
                            step.id,
                            step.verify_cmd,
                            output.stdout.len(),
                            output.stderr.len()
                        );
                        let attestation = Attestation {
                            signer: signer.clone(),
                            evidence,
                            receipt,
                            at: step_now,
                        };
                        let summary_raw =
                            String::from_utf8_lossy(&output.stdout).trim().to_string();
                        let result = ActivityResult {
                            activity_id: activity.id.clone(),
                            success: true,
                            attestation: attestation.clone(),
                            summary: Some(summary_raw.clone()),
                        };
                        workflow.complete_activity(result, step_now)?;
                        let verdict = if summary_raw.is_empty() {
                            format!("Step {} verified", step.id)
                        } else {
                            summary_raw.lines().take(15).collect::<Vec<_>>().join("\n")
                        };
                        wave_coordinator.mark_step_completed(&step.id, Some(verdict))?;
                        attestations.push(attestation);
                    } else {
                        let reason = format!(
                            "step {} failed with exit code: {:?}, stderr: {}",
                            step.id,
                            output.status.code(),
                            String::from_utf8_lossy(&output.stderr).trim()
                        );
                        workflow.fail_activity(&activity, reason.clone(), step_now)?;
                        let mut receipts = workflow.compensation_receipts().to_vec();
                        let rollback_receipts =
                            keep_receipts(wave_coordinator.rollback(&step.id, &reason))?;
                        receipts.extend(rollback_receipts);

                        return Ok(HarnessReport {
                            session_id: session_id.to_string(),
                            slot_id: slot_id.to_string(),
                            lease_id: lease_id.to_string(),
                            state: workflow.state(),
                            done: false,
                            policy_tier: Some(policy_tier),
                            attestation: attestations.last().cloned(),
                            attestations,
                            compensation_receipts: receipts,
                            failure_reason: Some(reason),
                            events: workflow.events().to_vec(),
                            cpm_schedule,
                            wave_plan: Some(wave_coordinator.plan().clone()),
                        });
                    }
                } else if step.kind == ActivityKind::Gate {
                    let receipt = format!(
                        "gate approved: {} ({})",
                        step.id,
                        step.gate.as_deref().unwrap_or("operator")
                    );
                    let attestation = Attestation {
                        signer: signer.clone(),
                        evidence,
                        receipt,
                        at: step_now,
                    };
                    let result = ActivityResult {
                        activity_id: activity.id.clone(),
                        success: true,
                        attestation: attestation.clone(),
                        summary: Some(format!("gate approved: {}", step.id)),
                    };
                    workflow.complete_activity(result, step_now)?;
                    wave_coordinator.mark_step_completed(
                        &step.id,
                        Some(format!("Gate {} approved", step.id)),
                    )?;
                    attestations.push(attestation);
                } else {
                    let receipt = format!("step executed: {} ({})", step.id, step.title);
                    let attestation = Attestation {
                        signer: signer.clone(),
                        evidence,
                        receipt,
                        at: step_now,
                    };
                    let result = ActivityResult {
                        activity_id: activity.id.clone(),
                        success: true,
                        attestation: attestation.clone(),
                        summary: Some(format!("step completed: {}", step.title)),
                    };
                    workflow.complete_activity(result, step_now)?;
                    wave_coordinator.mark_step_completed(
                        &step.id,
                        Some(format!("Step {} complete", step.id)),
                    )?;
                    attestations.push(attestation);
                }
            }

            wave_coordinator.advance_wave()?;
        }

        if !wave_coordinator.is_finished() {
            let reason = "wave flow terminated prematurely".to_string();
            let receipts = keep_receipts(wave_coordinator.rollback("wave-dag", &reason))?;
            return Ok(HarnessReport {
                session_id: session_id.to_string(),
                slot_id: slot_id.to_string(),
                lease_id: lease_id.to_string(),
                state: SessionState::Failed,
                done: false,
                policy_tier: Some(policy_tier),
                attestation: attestations.last().cloned(),
                attestations,
                compensation_receipts: receipts,
                failure_reason: Some(reason),
                events: workflow.events().to_vec(),
                cpm_schedule,
                wave_plan: Some(wave_coordinator.plan().clone()),
            });
        }

        let end_now = current_epoch_seconds();
        workflow.finish_success(end_now)?;

        return Ok(HarnessReport {
            session_id: session_id.to_string(),
            slot_id: slot_id.to_string(),
            lease_id: lease_id.to_string(),
            state: workflow.state(),
            done: true,
            policy_tier: Some(policy_tier),
            attestation: attestations.last().cloned(),
            attestations,
            compensation_receipts: Vec::new(),
            failure_reason: None,
            events: workflow.events().to_vec(),
            cpm_schedule,
            wave_plan: Some(wave_coordinator.plan().clone()),
        });
    }

    let mut coordinator = SagaFlowCoordinator::new(steps, Box::new(coord_compensator));

    while let Some(step) = coordinator.next_runnable_step().cloned() {
        let step_now = current_epoch_seconds();
        let activity = Activity::new(&step.id, &step.title, step.kind);

        if let Err(e) = admission_gate.admit_activity(&activity) {
            let reason = format!("policy admission rejected for step {}: {e}", step.id);
            workflow.fail_activity(&activity, reason.clone(), step_now)?;
            let mut receipts = workflow.compensation_receipts().to_vec();
            let rollback_receipts = keep_receipts(coordinator.rollback(&step.id, &reason))?;
            receipts.extend(rollback_receipts);

            return Ok(HarnessReport {
                session_id: session_id.to_string(),
                slot_id: slot_id.to_string(),
                lease_id: lease_id.to_string(),
                state: workflow.state(),
                done: false,
                policy_tier: Some(policy_tier),
                attestation: attestations.last().cloned(),
                attestations,
                compensation_receipts: receipts,
                failure_reason: Some(reason),
                events: workflow.events().to_vec(),
                cpm_schedule,
                wave_plan: None,
            });
        }

        workflow.start_activity(activity.clone(), step_now)?;

        if opts.dry_run {
            let receipt = format!("dry-run simulated step: {} ({})", step.id, step.title);
            let attestation = Attestation {
                signer: signer.clone(),
                evidence,
                receipt,
                at: step_now,
            };
            let result = ActivityResult {
                activity_id: activity.id.clone(),
                success: true,
                attestation: attestation.clone(),
                summary: Some(format!("simulated dry-run: {}", step.title)),
            };
            workflow.complete_activity(result, step_now)?;
            coordinator.mark_completed(&step.id);
            attestations.push(attestation);
        } else if !step.verify_cmd.is_empty() {
            let output = std::process::Command::new(&step.verify_cmd[0])
                .args(&step.verify_cmd[1..])
                .output()
                .with_context(|| format!("failed to execute verify_cmd for step {}", step.id))?;

            if output.status.success() {
                let receipt = format!(
                    "step: {}, command: {:?}, status: 0, stdout_bytes: {}, stderr_bytes: {}",
                    step.id,
                    step.verify_cmd,
                    output.stdout.len(),
                    output.stderr.len()
                );
                let attestation = Attestation {
                    signer: signer.clone(),
                    evidence,
                    receipt,
                    at: step_now,
                };
                let result = ActivityResult {
                    activity_id: activity.id.clone(),
                    success: true,
                    attestation: attestation.clone(),
                    summary: Some(String::from_utf8_lossy(&output.stdout).trim().into()),
                };
                workflow.complete_activity(result, step_now)?;
                coordinator.mark_completed(&step.id);
                attestations.push(attestation);
            } else {
                let reason = format!(
                    "step {} failed with exit code: {:?}, stderr: {}",
                    step.id,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                workflow.fail_activity(&activity, reason.clone(), step_now)?;
                let mut receipts = workflow.compensation_receipts().to_vec();
                let rollback_receipts = keep_receipts(coordinator.rollback(&step.id, &reason))?;
                receipts.extend(rollback_receipts);

                return Ok(HarnessReport {
                    session_id: session_id.to_string(),
                    slot_id: slot_id.to_string(),
                    lease_id: lease_id.to_string(),
                    state: workflow.state(),
                    done: false,
                    policy_tier: Some(policy_tier),
                    attestation: attestations.last().cloned(),
                    attestations,
                    compensation_receipts: receipts,
                    failure_reason: Some(reason),
                    events: workflow.events().to_vec(),
                    cpm_schedule,
                    wave_plan: None,
                });
            }
        } else if step.kind == ActivityKind::Gate {
            let receipt = format!(
                "gate approved: {} ({})",
                step.id,
                step.gate.as_deref().unwrap_or("operator")
            );
            let attestation = Attestation {
                signer: signer.clone(),
                evidence,
                receipt,
                at: step_now,
            };
            let result = ActivityResult {
                activity_id: activity.id.clone(),
                success: true,
                attestation: attestation.clone(),
                summary: Some(format!("gate approved: {}", step.id)),
            };
            workflow.complete_activity(result, step_now)?;
            coordinator.mark_completed(&step.id);
            attestations.push(attestation);
        } else {
            let receipt = format!("step executed: {} ({})", step.id, step.title);
            let attestation = Attestation {
                signer: signer.clone(),
                evidence,
                receipt,
                at: step_now,
            };
            let result = ActivityResult {
                activity_id: activity.id.clone(),
                success: true,
                attestation: attestation.clone(),
                summary: Some(format!("step completed: {}", step.title)),
            };
            workflow.complete_activity(result, step_now)?;
            coordinator.mark_completed(&step.id);
            attestations.push(attestation);
        }
    }

    if !coordinator.is_finished() {
        let reason =
            "flow terminated prematurely: unsatisfied dependencies or cyclic DAG".to_string();
        let receipts = keep_receipts(coordinator.rollback("flow-dag", &reason))?;
        return Ok(HarnessReport {
            session_id: session_id.to_string(),
            slot_id: slot_id.to_string(),
            lease_id: lease_id.to_string(),
            state: SessionState::Failed,
            done: false,
            policy_tier: Some(policy_tier),
            attestation: attestations.last().cloned(),
            attestations,
            compensation_receipts: receipts,
            failure_reason: Some(reason),
            events: workflow.events().to_vec(),
            cpm_schedule,
            wave_plan: None,
        });
    }

    let end_now = current_epoch_seconds();
    workflow.finish_success(end_now)?;

    Ok(HarnessReport {
        session_id: session_id.to_string(),
        slot_id: slot_id.to_string(),
        lease_id: lease_id.to_string(),
        state: workflow.state(),
        done: true,
        policy_tier: Some(policy_tier),
        attestation: attestations.last().cloned(),
        attestations,
        compensation_receipts: Vec::new(),
        failure_reason: None,
        events: workflow.events().to_vec(),
        cpm_schedule,
        wave_plan: None,
    })
}

fn harness_verify(receipt_path: &str) -> Result<VerificationReport> {
    let content = std::fs::read_to_string(receipt_path)
        .with_context(|| format!("failed to read receipt file: {receipt_path}"))?;
    let report: HarnessReport =
        serde_json::from_str(&content).context("failed to parse receipt JSON as HarnessReport")?;

    let session_id = SessionId::new(&report.session_id).context("invalid session_id in receipt")?;
    let _slot_id = SlotId::new(&report.slot_id).context("invalid slot_id in receipt")?;

    let attestation = report
        .attestation
        .as_ref()
        .context("receipt does not contain an attestation (activity was not successful)")?;

    if attestation.evidence == EvidenceTier::Verified && attestation.receipt.trim().is_empty() {
        anyhow::bail!("attestation claims Verified tier but receipt is empty");
    }

    let replayed = WorkflowSession::replay_from_events(session_id, &report.events)
        .context("event replay failed")?;

    if replayed.state() != report.state {
        anyhow::bail!(
            "replayed state {:?} does not match recorded state {:?}",
            replayed.state(),
            report.state
        );
    }

    Ok(VerificationReport {
        valid: true,
        session_id: report.session_id,
        slot_id: report.slot_id,
        evidence: attestation.evidence,
        state: report.state,
        event_count: report.events.len(),
        satisfies_gate: attestation.evidence.satisfies_gate(),
    })
}

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    if std::env::var("UAS_LOG_FORMAT").is_ok_and(|v| v == "json") {
        builder.json().init();
    } else {
        builder.init();
    }

    let cli = Cli::parse();
    let uses_cpm_flags = matches!(
        &cli.command,
        Command::Harness {
            command: HarnessCommand::Run { show_cpm: true, .. }
                | HarnessCommand::Run { wave: true, .. }
        }
    ) || matches!(
        &cli.command,
        Command::Harness {
            command: HarnessCommand::Run { max_concurrency, .. }
        } if *max_concurrency != DEFAULT_MAX_WAVE_CONCURRENCY
    );
    if matches!(cli.command, Command::Wave { .. }) || uses_cpm_flags {
        eprintln!("{DEPRECATION_NOTICE}");
    }
    match cli.command {
        Command::Demo { slot } => {
            let report = demo(&slot)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Wave { command } => match command {
            WaveCommand::Flow {
                from,
                out_dir,
                max_concurrency,
            } => wave_flow::run_flow(&from, &out_dir, max_concurrency)?,
            WaveCommand::Step {
                flow,
                step,
                repo,
                state_dir,
                max_concurrency,
                open_prs,
            } => {
                let args = wave_flow::StepArgs {
                    flow,
                    step,
                    repo,
                    state_dir,
                    max_concurrency,
                };
                let cwd = std::env::current_dir()?;
                let env = wave_flow::WaveEnv::from_process().with_open_prs_flag(open_prs);
                let code = wave_flow::run_step(&args, &env, &cwd);
                std::process::exit(code);
            }
            WaveCommand::MergeGate {
                flow,
                builder,
                repo,
                state_dir,
                max_concurrency,
            } => {
                let args = wave_flow::StepArgs {
                    flow,
                    step: builder,
                    repo,
                    state_dir,
                    max_concurrency,
                };
                let cwd = std::env::current_dir()?;
                let env = wave_flow::WaveEnv::from_process();
                std::process::exit(wave_flow::run_merge_gate(&args, &env, &cwd));
            }
        },
        Command::Harness { command } => match command {
            HarnessCommand::Run {
                slot,
                session,
                flow,
                task,
                evidence,
                policy_tier,
                signer,
                dry_run,
                show_cpm,
                wave,
                max_concurrency,
            } => {
                let report = match (flow, task) {
                    (Some(flow_path), _) => {
                        let opts = FlowRunOptions {
                            evidence: evidence.as_deref(),
                            policy_tier: policy_tier.as_deref(),
                            signer: &signer,
                            dry_run,
                            show_cpm,
                            wave,
                            max_concurrency,
                        };
                        harness_run_flow(&slot, session, &flow_path, &opts)?
                    }
                    (None, Some(task_cmd)) => harness_run(
                        &slot,
                        session,
                        &task_cmd,
                        evidence.as_deref(),
                        policy_tier.as_deref(),
                        &signer,
                        dry_run,
                    )?,
                    (None, None) => {
                        anyhow::bail!("either --flow <PATH> or --task <COMMAND> must be specified");
                    }
                };
                println!("{}", serde_json::to_string_pretty(&report)?);
                if !report.done {
                    anyhow::bail!("harness task failed: {:?}", report.failure_reason);
                }
            }
            HarnessCommand::Verify { receipt } => {
                let report = harness_verify(&receipt)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_ends_warm() {
        let report = demo("abc").unwrap();
        assert_eq!(report.steps.len(), 6);
        assert_eq!(report.final_state, SlotState::Warm);
        assert!(report.done);
        let states: Vec<_> = report.steps.iter().map(|s| s.slot_state).collect();
        use SlotState::{Cold, Leased, Suspended, Warm};
        assert_eq!(states, [Cold, Warm, Leased, Suspended, Leased, Warm]);
        assert_eq!(
            report.steps[4].lease_expires_at,
            Some(DEMO_START + 2 * DEMO_TTL)
        );
    }

    #[test]
    fn demo_rejects_bad_slot_id() {
        assert!(demo("Bad Id").is_err());
    }

    #[test]
    fn demo_json_has_no_token() {
        let json = serde_json::to_string(&demo("abc").unwrap()).unwrap();
        assert!(!json.contains("demo-token"));
    }

    #[test]
    fn dry_run_defaults_to_heuristic() {
        let report = harness_run(
            "slot-test",
            None,
            "make spec-check",
            None,
            None,
            "uas/test",
            true,
        )
        .unwrap();
        assert!(report.done);
        assert_eq!(report.state, SessionState::Succeeded);
        assert_eq!(report.events.len(), 4);
        let att = report.attestation.unwrap();
        assert_eq!(att.evidence, EvidenceTier::Heuristic);
        assert_eq!(att.signer.as_str(), "uas/test");
        assert_eq!(report.policy_tier, Some(PolicyTier::Tier0));
    }

    #[test]
    fn dry_run_rejects_verified() {
        let result = harness_run(
            "slot-test",
            None,
            "make spec-check",
            Some("verified"),
            None,
            "uas/test",
            true,
        );
        assert!(result.is_err());
    }

    #[test]
    fn real_run_defaults_to_verified() {
        let report = harness_run("slot-test", None, "true", None, None, "uas/test", false).unwrap();
        assert!(report.done);
        assert_eq!(report.state, SessionState::Succeeded);
        let att = report.attestation.unwrap();
        assert_eq!(att.evidence, EvidenceTier::Verified);
        assert_eq!(report.policy_tier, Some(PolicyTier::Tier1));
    }

    #[test]
    fn policy_tier0_rejects_real_execution() {
        let report = harness_run(
            "slot-test",
            None,
            "true",
            None,
            Some("tier-0"),
            "uas/test",
            false,
        )
        .unwrap();
        assert!(!report.done);
        assert_eq!(report.state, SessionState::Failed);
        assert!(report
            .failure_reason
            .unwrap()
            .contains("policy admission rejected"));
    }

    #[test]
    fn harness_run_executes_real_command() {
        let report = harness_run(
            "slot-test",
            None,
            "echo hello-uas",
            Some("verified"),
            None,
            "uas/test",
            false,
        )
        .unwrap();
        assert!(report.done);
        assert_eq!(report.state, SessionState::Succeeded);
        let att = report.attestation.unwrap();
        assert!(att.receipt.contains("status: 0"));
    }

    #[test]
    fn harness_run_captures_failure() {
        let report = harness_run(
            "slot-test",
            None,
            "exit 42",
            Some("verified"),
            None,
            "uas/test",
            false,
        )
        .unwrap();
        assert!(!report.done);
        assert_eq!(report.state, SessionState::Failed);
        assert!(report.attestation.is_none());
        assert!(report.failure_reason.is_some());
    }

    #[test]
    fn harness_verify_roundtrip() {
        let report = harness_run(
            "slot-test",
            None,
            "echo test-cmd",
            None,
            None,
            "uas/test",
            false,
        )
        .unwrap();
        let json = serde_json::to_string_pretty(&report).unwrap();
        let path = std::env::temp_dir().join("test-receipt-roundtrip.json");
        std::fs::write(&path, json).unwrap();

        let v_report = harness_verify(path.to_str().unwrap()).unwrap();
        assert!(v_report.valid);
        assert!(v_report.satisfies_gate);
        assert_eq!(v_report.session_id, "sess-slot-test");
        assert_eq!(v_report.event_count, 4);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn harness_run_flow_dry_run_executes_dag_and_attests() {
        let flow_path = if std::path::Path::new(".saga/flows/gates.toml").exists() {
            std::path::PathBuf::from(".saga/flows/gates.toml")
        } else {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../.saga/flows/gates.toml")
        };
        let opts = FlowRunOptions {
            evidence: Some("heuristic"),
            dry_run: true,
            ..Default::default()
        };
        let report =
            harness_run_flow("slot-test", None, flow_path.to_str().unwrap(), &opts).unwrap();

        assert!(report.done);
        assert_eq!(report.state, SessionState::Succeeded);
        assert_eq!(report.attestations.len(), 4);
        assert_eq!(report.attestation.unwrap().signer.as_str(), "uas/test");
        assert!(report.compensation_receipts.is_empty());
        assert!(report.failure_reason.is_none());
        assert!(report.cpm_schedule.is_none());
        assert!(report.wave_plan.is_none());
    }

    #[test]
    fn harness_run_flow_dry_run_with_cpm_and_wave() {
        let flow_path = if std::path::Path::new(".saga/flows/gates.toml").exists() {
            std::path::PathBuf::from(".saga/flows/gates.toml")
        } else {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../.saga/flows/gates.toml")
        };
        let opts = FlowRunOptions {
            evidence: Some("heuristic"),
            dry_run: true,
            show_cpm: true,
            wave: true,
            ..Default::default()
        };
        let report =
            harness_run_flow("slot-test", None, flow_path.to_str().unwrap(), &opts).unwrap();

        assert!(report.done);
        assert_eq!(report.state, SessionState::Succeeded);
        assert_eq!(report.attestations.len(), 4);
        assert!(report.cpm_schedule.is_some());
        let cpm = report.cpm_schedule.as_ref().unwrap();
        assert!(!cpm.critical_path.is_empty());
        assert!(report.wave_plan.is_some());
        let wave_plan = report.wave_plan.as_ref().unwrap();
        assert_eq!(wave_plan.waves.len(), 4);
    }

    #[test]
    fn harness_run_flow_failure_triggers_compensation() {
        let flow_content = r#"
[flow]
name = "failing-flow"
description = "A flow that fails and compensates"

[[step]]
id = "step-ok"
title = "Passing step"
kind = "test"
verify_cmd = ["echo", "step 1 ok"]

[[step]]
id = "step-fail"
title = "Failing step"
kind = "test"
deps = ["step-ok"]
verify_cmd = ["bash", "-c", "exit 1"]
"#;
        let path = std::env::temp_dir().join("test-failing-flow.toml");
        std::fs::write(&path, flow_content).unwrap();

        let opts = FlowRunOptions {
            evidence: Some("verified"),
            ..Default::default()
        };
        let report = harness_run_flow("slot-test", None, path.to_str().unwrap(), &opts).unwrap();

        assert!(!report.done);
        assert_eq!(report.state, SessionState::Failed);
        assert!(report.failure_reason.is_some());
        assert!(!report.compensation_receipts.is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn harness_run_flow_wave_execution_real_commands() {
        let flow_content = r#"
[flow]
name = "wave-flow"
description = "A multi-wave flow with real commands"

[[step]]
id = "probe-1"
title = "Read-only probe 1"
kind = "probe"
agent = "probe"
verify_cmd = ["echo", "probe 1 pass"]

[[step]]
id = "probe-2"
title = "Read-only probe 2"
kind = "probe"
agent = "probe"
verify_cmd = ["echo", "probe 2 pass"]

[[step]]
id = "critic"
title = "Critic evaluation"
kind = "test"
agent = "critic"
deps = ["probe-1", "probe-2"]
verify_cmd = ["echo", "critic pass"]
"#;
        let path = std::env::temp_dir().join("test-wave-flow.toml");
        std::fs::write(&path, flow_content).unwrap();

        let opts = FlowRunOptions {
            evidence: Some("verified"),
            show_cpm: true,
            wave: true,
            ..Default::default()
        };
        let report = harness_run_flow("slot-test", None, path.to_str().unwrap(), &opts).unwrap();

        assert!(report.done);
        assert_eq!(report.state, SessionState::Succeeded);
        assert_eq!(report.attestations.len(), 3);
        assert!(report.wave_plan.is_some());
        assert!(report.cpm_schedule.is_some());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn keep_receipts_returns_every_receipt_when_compensation_incomplete() {
        let receipts = vec![
            CompensationReceipt::new("s3", "recording", true, "ok", 0),
            CompensationReceipt::new("s2", "fail_on", false, "injected", 0),
            CompensationReceipt::new("s1", "recording", true, "ok", 0),
        ];
        let err = EngineError::CompensationIncomplete {
            failed: 1,
            receipts: receipts.clone(),
        };
        let kept = keep_receipts(Err(err)).unwrap();
        assert_eq!(kept, receipts);
        assert!(!kept[1].success);
    }

    #[test]
    fn keep_receipts_passes_other_errors_through() {
        let err = EngineError::StoreError("boom".into());
        match keep_receipts(Err(err)) {
            Err(EngineError::StoreError(msg)) => assert_eq!(msg, "boom"),
            other => panic!("expected StoreError pass-through, got {other:?}"),
        }
    }
}
