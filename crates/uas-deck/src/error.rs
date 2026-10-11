// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Errors returned by the deck.

use std::io;
use std::net::SocketAddr;

use thiserror::Error;

/// Errors produced by `uas-deck`.
#[derive(Debug, Error)]
pub enum DeckError {
    /// The listener could not bind. The port is taken or permission is denied.
    #[error("cannot bind the deck server to {addr}: {source}")]
    ServerBindFailed {
        /// Address that was requested.
        addr: SocketAddr,
        /// Underlying I/O error.
        source: io::Error,
    },
    /// The settle queue could not be reached. The page shows a fixed notice; the
    /// text is kept for a later log line and is never rendered.
    #[error("settle queue unavailable: {0}")]
    CortexUnavailable(String),
    /// The settle queue answered, but the body does not match the contract. The
    /// page shows the same notice; the text is kept for a log line and never rendered.
    #[error("settle contract mismatch: {0}")]
    SettleContract(String),
}
