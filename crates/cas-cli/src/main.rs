//! `cas` command line entry point.

mod memory;

use anyhow::{Context, Result};
use cas_domain::{
    Attestation, Budget, EvidenceTier, Lease, LeaseId, LeaseLedger, LeaseToken, PrincipalRef, Slot,
    SlotId, SlotState, WorktreeRef,
};
use cas_engine::{
    Activity, ActivityKind, ActivityResult, SessionId, SessionState, WorkflowEvent, WorkflowSession,
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

        /// Task description or shell command to execute.
        #[arg(long)]
        task: String,

        /// Evidence tier (verified, reported, heuristic, asserted).
        #[arg(long, default_value = "verified")]
        evidence: String,

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
    attestation: Option<Attestation>,
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
    evidence_str: &str,
    signer: &str,
    dry_run: bool,
) -> Result<HarnessReport> {
    let slot_id = SlotId::new(slot).context("invalid --slot")?;
    let sess_name = session.unwrap_or_else(|| format!("sess-{}", slot_id.as_str()));
    let session_id = SessionId::new(sess_name).context("invalid session id")?;
    let lease_id =
        LeaseId::new(format!("lease-{}", slot_id.as_str())).context("invalid lease id")?;
    let signer = PrincipalRef::new(signer).context("invalid --signer")?;
    let evidence = parse_evidence_tier(evidence_str)?;
    let now = current_epoch_seconds();

    let mut workflow = WorkflowSession::new(session_id.clone());
    workflow.start(slot_id.clone(), lease_id.clone(), now)?;

    let activity = Activity::new("act-1", task, ActivityKind::Verify);
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
            attestation: Some(attestation),
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
                attestation: Some(attestation),
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
                attestation: None,
                failure_reason: Some(reason),
                events: workflow.events().to_vec(),
            })
        }
    }
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
                task,
                evidence,
                signer,
                dry_run,
            } => {
                let report = harness_run(&slot, session, &task, &evidence, &signer, dry_run)?;
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
    fn harness_run_dry_run_generates_verified_report() {
        let report = harness_run(
            "slot-test",
            None,
            "make spec-check",
            "verified",
            "cas/test",
            true,
        )
        .unwrap();
        assert!(report.done);
        assert_eq!(report.state, SessionState::Succeeded);
        assert_eq!(report.events.len(), 4);
        let att = report.attestation.unwrap();
        assert_eq!(att.evidence, EvidenceTier::Verified);
        assert_eq!(att.signer.as_str(), "cas/test");
    }

    #[test]
    fn harness_run_executes_real_command() {
        let report = harness_run(
            "slot-test",
            None,
            "echo hello-cas",
            "verified",
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
        let report =
            harness_run("slot-test", None, "exit 42", "verified", "cas/test", false).unwrap();
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
            "make spec-check",
            "verified",
            "cas/test",
            true,
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
}
