//! Error type for the ACP server, and its JSON-RPC error codes.

use cas_pool::PoolError;

/// An error raised while the ACP server handles one frame.
#[derive(Debug, thiserror::Error)]
pub enum AcpError {
    /// The frame is not valid JSON.
    #[error("protocol parse error: {0}")]
    Parse(String),
    /// The frame is JSON but not a JSON-RPC request.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// The method is not one `cas-acp` serves.
    #[error("method not found: {0}")]
    MethodNotFound(String),
    /// The params do not match the method's schema.
    #[error("invalid parameters: {0}")]
    InvalidParams(String),
    /// The worker pool refused the operation.
    #[error("pool error: {0}")]
    Pool(#[from] PoolError),
    /// The engine or an internal id could not be built.
    #[error("engine workflow error: {0}")]
    Engine(String),
}

impl AcpError {
    /// The JSON-RPC error code for this error (spec 0016, Error Protocol).
    pub fn code(&self) -> i64 {
        match self {
            Self::Parse(_) => -32700,
            Self::InvalidRequest(_) => -32600,
            Self::MethodNotFound(_) => -32601,
            Self::InvalidParams(_) => -32602,
            Self::Pool(PoolError::CapacityExhausted(_)) => -32001,
            Self::Pool(
                PoolError::LeaseExpired(_)
                | PoolError::NoActiveLease(_)
                | PoolError::TokenMismatch(_),
            ) => -32002,
            Self::Pool(_) | Self::Engine(_) => -32603,
        }
    }
}
