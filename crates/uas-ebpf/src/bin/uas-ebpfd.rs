// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `uas-ebpfd`: the privileged loader daemon (spec 0014 D8, ruling R3).
//!
//! It probes the host, loads the kernel program and serves the line protocol
//! on a Unix socket. On a host that cannot load the program it exits with
//! status 2 and one line saying why.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

/// Version text: crate version plus the git revision stamped by `build.rs`.
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("UAS_BUILD_REV"), ")");

/// Command line of `uas-ebpfd`.
#[derive(Debug, Parser)]
#[command(name = "uas-ebpfd", version = VERSION, about = "UAS eBPF clamping and audit daemon")]
struct Args {
    /// Unix socket to serve `uas-pool` on.
    #[arg(long, default_value = "/run/uas/ebpf.sock")]
    socket: PathBuf,
    /// Group that may connect to the socket (mode 0660). Resolved from /etc/group.
    #[arg(long)]
    socket_group: Option<String>,
}

fn main() -> ExitCode {
    init_logging();
    match run(Args::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("uas-ebpfd: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Log to stderr, `info` by default, JSON when `UAS_LOG_FORMAT=json`.
fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    if std::env::var("UAS_LOG_FORMAT").is_ok_and(|v| v == "json") {
        builder.json().init();
    } else {
        builder.init();
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
