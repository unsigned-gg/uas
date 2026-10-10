//! The ports of the pool-member adapter (spec 0026 D2). No tonic type appears here, so fakes
//! test the bridge and the adapter. `grpc.rs` maps the generated types to these.

/// One unit of pool work, as `PoolDelivery` carries it (spec 0026, Wire contract).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolDelivery {
    /// The opaque token, bound to this member.
    pub delivery_token: String,
    /// The position in the stream.
    pub stream_seq: u64,
    /// The `subject` of the `LaneMessage`.
    pub subject: String,
    /// The `body` of the `LaneMessage`.
    pub body: String,
    /// The Cortex delivery lease in milliseconds. proto3 sends 0 when unset.
    pub lease_ms: u64,
    /// 1 on the first delivery.
    pub delivery_attempt: u32,
}

/// What `next_event` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecvEvent {
    /// A unit of work.
    Delivery(PoolDelivery),
    /// Nothing arrived within the wait.
    Idle,
    /// The stream ended.
    End,
}

/// A failed port call.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("pool port: {0}")]
pub struct PortError(pub String);

/// The five `PoolWork` RPCs (spec 0026, Wire contract). The member's `peer_id` belongs to the
/// implementation, and no call carries a pool name.
pub trait PoolWork {
    /// Open a `Recv` stream for at most `free_slots` units. Every call below takes `within_ms`:
    /// it must fail by then (spec 0026 D12), so a hung RPC cannot hold the adapter thread.
    fn recv(&mut self, free_slots: u32, within_ms: u64) -> Result<(), PortError>;
    /// Wait up to `wait_ms` for the next event of the open stream. An implementation spends
    /// the wait itself: the adapter does not sleep after `Idle`.
    fn next_event(&mut self, wait_ms: u64) -> Result<RecvEvent, PortError>;
    /// The units are done.
    fn ack(&mut self, delivery_tokens: &[String], within_ms: u64) -> Result<(), PortError>;
    /// Keep the Cortex lease of a unit alive.
    fn extend(&mut self, delivery_token: &str, within_ms: u64) -> Result<(), PortError>;
    /// Redeliver after `delay_ms`.
    fn nack(
        &mut self,
        delivery_token: &str,
        delay_ms: u64,
        within_ms: u64,
    ) -> Result<(), PortError>;
    /// Dead-letter now, with a reason.
    fn term(&mut self, delivery_token: &str, reason: &str, within_ms: u64)
        -> Result<(), PortError>;
}

/// Why a task cannot start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    /// The payload does not parse or breaks policy: `Term`.
    Payload(String),
    /// A recoverable fault: `Nack`.
    Recoverable(String),
}

/// How a running task stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    /// Still running.
    Running,
    /// Done: `Ack`.
    Succeeded,
    /// Failed with a recoverable error: `Nack`.
    Failed(String),
}

/// Runs the work of a delivery in a slot. A real runner waits on the `cas-pool` runtime
/// (spec 0026 D8), so only fakes implement this.
pub trait TaskRunner {
    /// Start the unit.
    fn start(&mut self, delivery: &PoolDelivery) -> Result<(), TaskError>;
    /// Poll the unit with this delivery token.
    fn poll(&mut self, delivery_token: &str) -> TaskStatus;
    /// Stop the unit with this delivery token.
    fn cancel(&mut self, delivery_token: &str);
}

/// The adapter's clock, in milliseconds. The domain has none (spec 0026 F3).
pub trait Clock {
    /// Milliseconds on a monotonic clock.
    fn now_ms(&self) -> u64;
    /// Wait this long.
    fn sleep_ms(&self, ms: u64);
}
