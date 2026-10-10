//! The Cortex pool-member adapter (spec 0026), behind the feature `cortex-grpc`.

mod adapter;
mod bridge;
mod grpc;
mod port;

pub use adapter::{AdapterConfig, PoolAdapter, PoolError};
pub use bridge::{issue, nack_delay_ms, renew, ttl_secs, MIN_LEASE_MS};
#[doc(hidden)]
pub use grpc::pb;
pub use grpc::{GrpcPoolWork, GrpcTimeouts};
pub use port::{
    Clock, PoolDelivery, PoolWork, PortError, RecvEvent, TaskError, TaskRunner, TaskStatus,
};
