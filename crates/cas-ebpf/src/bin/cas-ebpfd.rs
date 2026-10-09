//! `cas-ebpfd`: the privileged loader daemon (spec 0014 D8, ruling R3).
//!
//! Placeholder. A builder fills it in this branch. It parses the socket
//! argument, initialises logging and exits with a clear message.

use std::path::PathBuf;

use anyhow::bail;
use clap::Parser;

/// Command line of `cas-ebpfd`.
#[derive(Debug, Parser)]
#[command(name = "cas-ebpfd", about = "CAS eBPF clamping and audit daemon")]
struct Args {
    /// Unix socket to serve `cas-pool` on.
    #[arg(long, default_value = "/run/cas/ebpf.sock")]
    socket: PathBuf,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    tracing::info!(socket = %args.socket.display(), "cas-ebpfd placeholder");
    bail!("cas-ebpfd is not implemented yet");
}
