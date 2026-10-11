// SPDX-License-Identifier: Apache-2.0 OR MIT
//! The tonic client of `cortex.mesh.v1.PoolWork` (spec 0026, Wire contract). It implements the
//! sync [`PoolWork`] port over a private current-thread runtime. This is the only file that
//! names a tonic or generated type; the port types carry none.

use std::time::Duration;

use tonic::transport::Channel;

use super::port::{PoolDelivery, PoolWork, PortError, RecvEvent};

/// The generated messages and stubs of `pool.proto`, `lane.proto` and `peer.proto`.
#[doc(hidden)]
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod pb {
    tonic::include_proto!("cortex.mesh.v1");
}

use pb::pool_work_client::PoolWorkClient;

/// The deadlines of the client. The adapter is single-threaded, so one hung RPC would stop every
/// unit's Extend: each call must fail inside `request`, and the connect inside `connect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrpcTimeouts {
    /// How long the first connection may take.
    pub connect: Duration,
    /// How long one RPC may take to answer (Ack, Extend, Nack, Term, and the opening of Recv).
    pub request: Duration,
}

impl Default for GrpcTimeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(5),
            request: Duration::from_secs(2),
        }
    }
}

/// A `PoolWork` client for one member. The `peer_id` comes from config, and no request carries
/// a pool name (F1).
pub struct GrpcPoolWork {
    rt: tokio::runtime::Runtime,
    client: PoolWorkClient<Channel>,
    peer_id: String,
    stream: Option<tonic::Streaming<pb::PoolDelivery>>,
}

/// Run one RPC, and cut it at `within_ms` (spec 0026 D12). The timer is built inside `block_on`,
/// which gives it the runtime context. The channel caps (connect, request) still apply.
fn bounded<T>(
    rt: &tokio::runtime::Runtime,
    within_ms: u64,
    rpc: impl std::future::Future<Output = Result<T, tonic::Status>>,
) -> Result<T, PortError> {
    let wait = Duration::from_millis(within_ms);
    match rt.block_on(async { tokio::time::timeout(wait, rpc).await }) {
        Ok(result) => result.map_err(port),
        Err(_) => Err(PortError(format!("call cut at {within_ms} ms"))),
    }
}

fn port(err: impl std::fmt::Display) -> PortError {
    PortError(err.to_string())
}

impl GrpcPoolWork {
    /// Connect to `endpoint`, for example `http://127.0.0.1:50051`.
    pub fn connect(
        endpoint: impl Into<String>,
        peer_id: impl Into<String>,
    ) -> Result<Self, PortError> {
        Self::connect_with(endpoint, peer_id, GrpcTimeouts::default())
    }

    /// Connect with explicit deadlines.
    pub fn connect_with(
        endpoint: impl Into<String>,
        peer_id: impl Into<String>,
        timeouts: GrpcTimeouts,
    ) -> Result<Self, PortError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(port)?;
        let endpoint = Channel::from_shared(endpoint.into())
            .map_err(port)?
            .connect_timeout(timeouts.connect)
            .timeout(timeouts.request);
        let channel = rt.block_on(endpoint.connect()).map_err(port)?;
        Ok(Self {
            rt,
            client: PoolWorkClient::new(channel),
            peer_id: peer_id.into(),
            stream: None,
        })
    }
}

fn delivery(d: pb::PoolDelivery) -> PoolDelivery {
    let message = d.message.unwrap_or_default();
    PoolDelivery {
        delivery_token: d.delivery_token,
        stream_seq: d.stream_seq,
        subject: message.subject,
        body: message.body,
        lease_ms: u64::from(d.lease_ms),
        delivery_attempt: d.delivery_attempt,
    }
}

impl PoolWork for GrpcPoolWork {
    fn recv(&mut self, free_slots: u32, within_ms: u64) -> Result<(), PortError> {
        // The port has no close: drop the prior server stream before the new Recv.
        self.stream = None;
        let request = pb::PoolRecvRequest {
            peer_id: self.peer_id.clone(),
            free_slots,
        };
        let stream = bounded(&self.rt, within_ms, self.client.recv(request))?.into_inner();
        self.stream = Some(stream);
        Ok(())
    }

    fn next_event(&mut self, wait_ms: u64) -> Result<RecvEvent, PortError> {
        let Some(stream) = self.stream.as_mut() else {
            return Ok(RecvEvent::End);
        };
        let wait = Duration::from_millis(wait_ms);
        // The timer needs the runtime context, so build the timeout inside `block_on`.
        match self
            .rt
            .block_on(async { tokio::time::timeout(wait, stream.message()).await })
        {
            Err(_) => Ok(RecvEvent::Idle),
            Ok(Ok(Some(d))) => Ok(RecvEvent::Delivery(delivery(d))),
            Ok(Ok(None)) => {
                self.stream = None;
                Ok(RecvEvent::End)
            }
            Ok(Err(status)) => {
                self.stream = None;
                Err(port(status))
            }
        }
    }

    fn ack(&mut self, delivery_tokens: &[String], within_ms: u64) -> Result<(), PortError> {
        let request = pb::PoolAckRequest {
            peer_id: self.peer_id.clone(),
            delivery_tokens: delivery_tokens.to_vec(),
        };
        bounded(&self.rt, within_ms, self.client.ack(request)).map(drop)
    }

    fn extend(&mut self, delivery_token: &str, within_ms: u64) -> Result<(), PortError> {
        let request = pb::PoolExtendRequest {
            peer_id: self.peer_id.clone(),
            delivery_token: delivery_token.to_owned(),
        };
        bounded(&self.rt, within_ms, self.client.extend(request)).map(drop)
    }

    fn nack(
        &mut self,
        delivery_token: &str,
        delay_ms: u64,
        within_ms: u64,
    ) -> Result<(), PortError> {
        let request = pb::PoolNackRequest {
            peer_id: self.peer_id.clone(),
            delivery_token: delivery_token.to_owned(),
            delay_ms: u32::try_from(delay_ms).unwrap_or(u32::MAX),
        };
        bounded(&self.rt, within_ms, self.client.nack(request)).map(drop)
    }

    fn term(
        &mut self,
        delivery_token: &str,
        reason: &str,
        within_ms: u64,
    ) -> Result<(), PortError> {
        let request = pb::PoolTermRequest {
            peer_id: self.peer_id.clone(),
            delivery_token: delivery_token.to_owned(),
            reason: reason.to_owned(),
        };
        bounded(&self.rt, within_ms, self.client.term(request)).map(drop)
    }
}
