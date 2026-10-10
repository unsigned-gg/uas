//! Typed errors for the loader, the clamp and the daemon protocol (spec 0014).

use thiserror::Error;

/// Everything that can go wrong on the userspace side of `cas-ebpf`.
#[derive(Debug, Error)]
pub enum EbpfError {
    /// `/sys/kernel/btf/vmlinux` is absent.
    #[error("kernel lacks BTF debug info at /sys/kernel/btf/vmlinux")]
    BtfMissing,
    /// The caller lacks `CAP_BPF`, `CAP_PERFMON` or `CAP_SYS_NICE`.
    #[error("missing required capabilities (CAP_BPF, CAP_PERFMON, CAP_SYS_NICE)")]
    PermissionDenied,
    /// aya could not parse or load the committed object.
    #[error("failed to load the BPF object: {0}")]
    Load(#[from] aya::EbpfError),
    /// The object has no program with this name, or it is not a tracepoint.
    #[error("BPF program {name} is missing from the object or has the wrong type")]
    ProgramMissing {
        /// The program looked up.
        name: &'static str,
    },
    /// The verifier or the kernel refused the program.
    #[error("failed to load program {name}: {source}")]
    ProgramLoadFailed {
        /// The program that failed.
        name: &'static str,
        /// The aya error.
        #[source]
        source: aya::programs::ProgramError,
    },
    /// The tracepoint attach failed.
    #[error("failed to attach to tracepoint {name}: {source}")]
    AttachTracepointFailed {
        /// The tracepoint, as `category/name`.
        name: &'static str,
        /// The aya error.
        #[source]
        source: aya::programs::ProgramError,
    },
    /// A tracepoint `format` file is unreadable or breaks a D13 rule. Nothing attached.
    #[error("tracepoint format {tracepoint}: {source}")]
    TracepointFormat {
        /// The tracepoint, as `category/name`.
        tracepoint: &'static str,
        /// What was wrong with the file.
        #[source]
        source: crate::tracefs::FormatError,
    },
    /// The object has no map with this name.
    #[error("BPF map {name} is missing from the object")]
    MapMissing {
        /// The map looked up.
        name: &'static str,
    },
    /// A map operation failed.
    #[error("BPF map {name}: {source}")]
    Map {
        /// The map operated on.
        name: &'static str,
        /// The aya error.
        #[source]
        source: aya::maps::MapError,
    },
    /// The kernel dropped records because the ring buffer was full.
    #[error("audit ring buffer dropped {dropped} events")]
    RingBufferOverflow {
        /// Records dropped since the last read.
        dropped: u64,
    },
    /// `cas-ebpfd` is not listening at the socket path.
    #[error("failed to connect to cas-ebpfd socket at {path}: {source}")]
    DaemonUnavailable {
        /// The socket path tried.
        path: String,
        /// The connect error.
        #[source]
        source: std::io::Error,
    },
    /// A frame on the daemon socket was malformed or not on the allowlist.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// An I/O error outside the categories above.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}
