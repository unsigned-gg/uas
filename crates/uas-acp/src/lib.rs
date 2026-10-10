//! Agent Communication Protocol server (JSON-RPC 2.0 over stdio) for Zed and IDE integration.
//!
//! Spec 0016, amended by A1. This crate serves `initialize`, `session/new` and the
//! `session/cancel` notification. It takes slots from the pool through [`PoolPort`].
//! It runs no workflow turns yet: `session/prompt` and `session/update` need real
//! execution. See `README.md` in this crate.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod pool_port;
pub mod server;

pub use error::AcpError;
pub use pool_port::PoolPort;
pub use server::{AcpConfig, AcpServer};
