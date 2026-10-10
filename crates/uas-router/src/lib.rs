//! vLLM prefix-hash routing, FastCache affinity and load balancing for model calls.
//!
//! First code (spec 0013): the Unix-socket proxy that carries a jailed worker's
//! gateway calls over plain HTTP, with an allowlist and key injection.
//!
//! Prefix hashing (spec 0017 D1, D2) lives in [`prefix`]. Rendezvous replica choice
//! (spec 0017 D3) lives in [`affinity`]. When every replica is unhealthy, the D5
//! round-robin fallback picks a replica and reports it as [`ReplicaChoice::Fallback`].
//! The token multiplexer and the health probe are still to come. See `README.md` in
//! this crate.
#![forbid(unsafe_code)]

pub mod affinity;
pub mod prefix;
pub mod uds_proxy;

pub use affinity::{AffinityTable, ReplicaChoice, ReplicaId, RouterError};
pub use prefix::{PrefixHash, PrefixInput, ToolDef};

pub use uds_proxy::{
    ProxyConfig, ProxyError, ProxyHandle, DEFAULT_GET_PREFIXES, DEFAULT_POST_PATHS,
    MAX_HEADER_BYTES,
};
