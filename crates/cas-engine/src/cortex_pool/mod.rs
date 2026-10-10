//! The Cortex pool-member adapter (spec 0026), behind the feature `cortex-grpc`.

mod adapter;
mod bridge;
mod port;

pub use adapter::{AdapterConfig, PoolAdapter, PoolError};
pub use bridge::{issue, nack_delay_ms, renew, ttl_secs, MIN_LEASE_MS};
pub use port::{
    Clock, PoolDelivery, PoolWork, PortError, RecvEvent, TaskError, TaskRunner, TaskStatus,
};
