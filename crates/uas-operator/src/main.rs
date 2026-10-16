// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `uas-operator` command line entry point (spec 0038, spec 0042).

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::EnvFilter;
use uas_operator::{HostSlotLockTable, OperatorMode, OperatorReconciler};

/// Version text: crate version plus the git revision stamped by `build.rs` (spec 0042).
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("UAS_BUILD_REV"), ")");

/// Kubernetes operator and node agent for WorkerSlot and AgentSandbox custom resources.
#[derive(Debug, Parser)]
#[command(name = "uas-operator", version = VERSION, about)]
struct Cli {
    /// Operating mode: `controller` (Deployment) or `node-agent` (DaemonSet).
    #[arg(long, env = "UAS_OPERATOR_MODE", default_value = "controller")]
    mode: OperatorMode,

    /// Kubernetes namespace to watch for WorkerSlot and AgentSandbox resources.
    #[arg(long, env = "UAS_OPERATOR_NAMESPACE", default_value = "uas-system")]
    namespace: String,

    /// Node name for DaemonSet node-agent slot lock and sandbox actuation.
    #[arg(long, env = "UAS_NODE_NAME", default_value = "localhost")]
    node_name: String,

    /// Directory holding Spec 0036 host slot locks (`slot-0.lock` .. `slot-(H-1).lock`).
    #[arg(long, env = "UAS_SLOT_DIR", default_value = "/tmp/uas-slots")]
    slot_dir: PathBuf,

    /// Host slot ceiling `H` (`max_slots` per node, default 2 on ceres).
    #[arg(long, env = "UAS_HOST_MAX_SLOTS", default_value_t = 2)]
    host_max_slots: usize,

    /// Log output format (`text` or `json`).
    #[arg(long, env = "UAS_LOG_FORMAT", default_value = "text")]
    log_format: String,

    /// Emit the generated OpenAPI v3 CRD documents as JSON on stdout and exit.
    #[arg(long, alias = "emit-crds")]
    print_crds: bool,

    /// Validate configuration and host slot directory once, then exit 0 without connecting to a cluster.
    #[arg(long)]
    once: bool,
}

fn init_logging(log_format: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    if log_format == "json" {
        builder.json().init();
    } else {
        builder.init();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    init_logging(&cli.log_format);

    if cli.print_crds {
        #[cfg(feature = "k8s")]
        {
            println!("{}", uas_operator::crd_documents_json()?);
            return Ok(());
        }
        #[cfg(not(feature = "k8s"))]
        {
            return Err(
                "uas-operator must be compiled with --features k8s for --print-crds".into(),
            );
        }
    }

    let lock_table = Arc::new(HostSlotLockTable::new(&cli.slot_dir, cli.host_max_slots)?);
    let reconciler = Arc::new(OperatorReconciler::new(
        cli.mode,
        cli.node_name.clone(),
        lock_table,
    ));

    tracing::info!(
        mode = ?reconciler.mode(),
        namespace = %cli.namespace,
        node_name = %cli.node_name,
        slot_dir = %cli.slot_dir.display(),
        host_max_slots = cli.host_max_slots,
        "uas-operator initialized"
    );

    if cli.once {
        return Ok(());
    }

    #[cfg(feature = "k8s")]
    {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        rt.block_on(uas_operator::run_k8s_watch_loop(reconciler, &cli.namespace))?;
        Ok(())
    }

    #[cfg(not(feature = "k8s"))]
    {
        let _ = reconciler;
        Err("uas-operator watch loop requires --features k8s (or pass --once)".into())
    }
}
