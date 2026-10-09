//! `cas` command line entry point.

mod memory;

use anyhow::{Context, Result};
use cas_domain::{
    Attestation, Budget, EvidenceTier, Lease, LeaseId, LeaseLedger, LeaseToken, PolicyTier,
    PrincipalRef, Slot, SlotId, SlotState, SomaClassification, WorktreeRef,
};
use cas_engine::{
    Activity, ActivityKind, ActivityResult, BranchPruneCompensator, CompensationReceipt,
    CompositeCompensator, InMemoryEventStore, LeaseReturnCompensator, SagaFlowCoordinator,
    SagaFlowDef, SessionId, SessionState, SomaAdmissionGate, WorkflowEvent, WorkflowSession,
    WorktreeCleanupCompensator,
};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

use memory::InMemoryLedger;

/// Version text: crate version plus the git revision stamped by `build.rs`.
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("CAS_BUILD_REV"), ")");

/// Fixed demo clock, in epoch seconds. The domain never reads a clock.
const DEMO_START: u64 = 1_000;
/// Demo lease length, in seconds.
const DEMO_TTL: u64 = 300;

/// Cerebral Agent Substrate command line tool.
#[derive(Debug, Parser)]
#[command(name = "cas", version = VERSION, about)]
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
        #[arg(long, default_value = "cas/harness")]
        signer: String,

        /// Dry-run mode: do not execute shell command, simulate success.
        #[arg(long)]
        dry_run: bool,
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
            })
        }
    }
}

fn harness_run_flow(
    slot: &str,
    session: Option<String>,
    flow_path: &str,
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

    let policy_tier = match policy_tier_opt {
        Some(raw) => parse_policy_tier(raw)?,
        None => default_tier,
    };

    let classification = SomaClassification::new(
        policy_tier,
        if dry_run { "simulation" } else { "execution" },
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
    let mut coordinator = SagaFlowCoordinator::new(steps, Box::new(coord_compensator));

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

    let mut attestations = Vec::new();

    while let Some(step) = coordinator.next_runnable_step().cloned() {
        let step_now = current_epoch_seconds();
        let activity = Activity::new(&step.id, &step.title, step.kind);

        if let Err(e) = admission_gate.admit_activity(&activity) {
            let reason = format!("policy admission rejected for step {}: {e}", step.id);
            workflow.fail_activity(&activity, reason.clone(), step_now)?;
            let mut receipts = workflow.compensation_receipts().to_vec();
            let rollback_receipts = coordinator.rollback(&step.id, &reason)?;
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
            });
        }

        workflow.start_activity(activity.clone(), step_now)?;

        if dry_run {
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
                let rollback_receipts = coordinator.rollback(&step.id, &reason)?;
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
        let receipts = coordinator.rollback("flow-dag", &reason)?;
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
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Demo { slot } => {
            let report = demo(&slot)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
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
            } => {
                let report = match (flow, task) {
                    (Some(flow_path), _) => harness_run_flow(
                        &slot,
                        session,
                        &flow_path,
                        evidence.as_deref(),
                        policy_tier.as_deref(),
                        &signer,
                        dry_run,
                    )?,
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
            "cas/test",
            true,
        )
        .unwrap();
        assert!(report.done);
        assert_eq!(report.state, SessionState::Succeeded);
        assert_eq!(report.events.len(), 4);
        let att = report.attestation.unwrap();
        assert_eq!(att.evidence, EvidenceTier::Heuristic);
        assert_eq!(att.signer.as_str(), "cas/test");
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
            "cas/test",
            true,
        );
        assert!(result.is_err());
    }

    #[test]
    fn real_run_defaults_to_verified() {
        let report = harness_run("slot-test", None, "true", None, None, "cas/test", false).unwrap();
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
            "cas/test",
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
            "echo hello-cas",
            Some("verified"),
            None,
            "cas/test",
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
            "cas/test",
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
            "cas/test",
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
        let report = harness_run_flow(
            "slot-test",
            None,
            flow_path.to_str().unwrap(),
            Some("heuristic"),
            None,
            "cas/test",
            true,
        )
        .unwrap();

        assert!(report.done);
        assert_eq!(report.state, SessionState::Succeeded);
        assert_eq!(report.attestations.len(), 4);
        assert_eq!(report.attestation.unwrap().signer.as_str(), "cas/test");
        assert!(report.compensation_receipts.is_empty());
        assert!(report.failure_reason.is_none());
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

        let report = harness_run_flow(
            "slot-test",
            None,
            path.to_str().unwrap(),
            Some("verified"),
            None,
            "cas/test",
            false,
        )
        .unwrap();

        assert!(!report.done);
        assert_eq!(report.state, SessionState::Failed);
        assert!(report.failure_reason.is_some());
        assert!(!report.compensation_receipts.is_empty());

        let _ = std::fs::remove_file(path);
    }
}
