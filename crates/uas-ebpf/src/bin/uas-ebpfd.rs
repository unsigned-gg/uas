//! `uas-ebpfd`: the privileged loader daemon (spec 0014 D8, ruling R3).
//!
//! It probes the host, loads the kernel program and serves the line protocol
//! on a Unix socket. On a host that cannot load the program it exits with
//! status 2 and one line saying why.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

/// Command line of `uas-ebpfd`.
#[derive(Debug, Parser)]
#[command(name = "uas-ebpfd", about = "UAS eBPF clamping and audit daemon")]
struct Args {
    /// Unix socket to serve `uas-pool` on.
    #[arg(long, default_value = "/run/uas/ebpf.sock")]
    socket: PathBuf,
    /// Group that may connect to the socket (mode 0660). Resolved from /etc/group.
    #[arg(long)]
    socket_group: Option<String>,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    match run(Args::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("uas-ebpfd: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> anyhow::Result<ExitCode> {
    let report = uas_ebpf::probe::probe();
    let summary = report.summary();
    tracing::info!(%summary, "host probe");
    if !report.can_load() {
        eprintln!("uas-ebpfd: cannot load the kernel program on this host: {summary}");
        return Ok(ExitCode::from(2));
    }
    let socket_gid = args
        .socket_group
        .as_deref()
        .map(uas_ebpf::daemon::resolve_group)
        .transpose()?;
    let probes = uas_ebpf::loader::Probes::load()?;
    uas_ebpf::daemon::run_with(probes, &args.socket, socket_gid)?;
    Ok(ExitCode::SUCCESS)
}
