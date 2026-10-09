//! vLLM prefix-hash routing, FastCache affinity and load balancing for model calls.
//!
//! First code (spec 0013): the Unix-socket proxy that carries a jailed worker's
//! gateway calls over plain HTTP, with an allowlist and key injection. Routing
//! itself is still to come (spec 0001, phase 4). See `README.md` in this crate.
#![forbid(unsafe_code)]

pub mod uds_proxy;

pub use uds_proxy::{
    ProxyConfig, ProxyError, ProxyHandle, DEFAULT_ALLOWED_PATHS, MAX_HEADER_BYTES,
};
