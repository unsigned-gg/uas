//! Errors returned by the deck.

use std::io;
use std::net::SocketAddr;

use thiserror::Error;

/// Errors produced by `cas-deck`.
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
    /// The settle queue could not be reached. The text is for logs, never for the page.
    #[error("settle queue unavailable: {0}")]
    CortexUnavailable(String),
}
