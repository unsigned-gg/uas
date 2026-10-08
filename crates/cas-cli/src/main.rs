//! `cas` command line entry point.

mod memory;

use anyhow::{Context, Result};
use cas_domain::{
    Budget, Lease, LeaseId, LeaseLedger, LeaseToken, PrincipalRef, Slot, SlotId, SlotState,
    WorktreeRef,
};
use clap::{Parser, Subcommand};
use serde::Serialize;
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
    ledger.issue(Lease::new(
        lease_id.clone(),
        slot_id.clone(),
        PrincipalRef::new("agent-demo")?,
        LeaseToken::new("demo-token-not-a-secret")?,
        DEMO_START,
        DEMO_START + DEMO_TTL,
        Budget::new(100_000, 600)?,
    )?)?;
    record("lease", &slot, &ledger);

    let slot = slot.suspend()?;
    record("suspend", &slot, &ledger);

    let slot = slot.resume()?;
    ledger.renew(&lease_id, DEMO_START + 100, DEMO_TTL)?;
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
}
