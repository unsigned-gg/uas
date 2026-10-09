//! `cas-ebpfd`: the privileged loader daemon (spec 0014 D8, ruling R3).
//!
//! It probes the host, loads the kernel program and serves the line protocol
//! on a Unix socket. On a host that cannot load the program it exits with
//! status 2 and one line saying why.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

/// Command line of `cas-ebpfd`.
#[derive(Debug, Parser)]
#[command(name = "cas-ebpfd", about = "CAS eBPF clamping and audit daemon")]
struct Args {
    /// Unix socket to serve `cas-pool` on.
    #[arg(long, default_value = "/run/cas/ebpf.sock")]
    socket: PathBuf,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    match run(Args::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("cas-ebpfd: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> anyhow::Result<ExitCode> {
    let report = cas_ebpf::probe::probe();
    let summary = report.summary();
    tracing::info!(%summary, "host probe");
    if !report.can_load() {
        eprintln!("cas-ebpfd: cannot load the kernel program on this host: {summary}");
        return Ok(ExitCode::from(2));
    }
    let probes = cas_ebpf::loader::Probes::load()?;
    cas_ebpf::daemon::run_with(probes, &args.socket)?;
    Ok(ExitCode::SUCCESS)
}
