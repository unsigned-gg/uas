//! The pool-member adapter (spec 0026). A step-driven state machine over the ports, so a fake
//! clock and a fake port test it without a runtime.

use cas_domain::{Budget, DomainError, Lease, LeaseId, LeaseToken, PrincipalRef, Slot, SlotState};

use super::bridge::{issue, nack_delay_ms, renew, ttl_secs, MIN_LEASE_MS};
use super::port::{
    Clock, PoolDelivery, PoolWork, PortError, RecvEvent, TaskError, TaskRunner, TaskStatus,
};

/// What the adapter needs besides its ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolConfig {
    /// This member's `peer_id`. It is also the lease holder.
    pub peer_id: String,
    /// The token budget of each CAS lease.
    pub max_tokens: u64,
    /// The longest wait of one `next_event`, in milliseconds.
    pub poll_ms: u64,
}

/// A failure the adapter surfaces to its caller.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PoolError {
    /// `lease_ms` is below the floor (D9). `Recv` stopped for good.
    #[error("delivery lease_ms {lease_ms} is below the minimum of 300; Recv stopped")]
    LeaseTooShort {
        /// The value that came on the wire.
        lease_ms: u64,
    },
    /// A port call failed where the adapter cannot go on.
    #[error(transparent)]
    Port(#[from] PortError),
    /// A domain rule refused a move.
    #[error(transparent)]
    Domain(#[from] DomainError),
}

/// One running unit: its slot, its CAS lease and its Extend schedule.
struct Unit {
    token: String,
    attempt: u32,
    lease_ms: u64,
    slot: usize,
    lease: Lease,
    due_ms: u64,
    extend_failures: u32,
}

/// An open `Recv` stream: it asked for `asked` units and has seen `got`.
struct Stream {
    asked: u32,
    got: u32,
}

/// The adapter.
pub struct PoolAdapter<P, R, C> {
    port: P,
    runner: R,
    clock: C,
    cfg: PoolConfig,
    slots: Vec<Slot>,
    units: Vec<Unit>,
    epoch_ms: Option<u64>,
    next_lease: u64,
    stream: Option<Stream>,
    stopped: bool,
    stream_failures: u32,
    reconnect_at_ms: u64,
}

impl<P: PoolWork, R: TaskRunner, C: Clock> PoolAdapter<P, R, C> {
    /// Build an adapter over `slots`, which must be `Warm`.
    pub fn new(cfg: PoolConfig, port: P, runner: R, clock: C, slots: Vec<Slot>) -> Self {
        Self {
            port,
            runner,
            clock,
            cfg,
            slots,
            units: Vec::new(),
            epoch_ms: None,
            next_lease: 0,
            stream: None,
            stopped: false,
            stream_failures: 0,
            reconnect_at_ms: 0,
        }
    }

    /// The slots, in order.
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// Run one pass: service the units, then handle one stream event or wait. After a D9
    /// stop it returns the error once and goes on serving the running units.
    pub fn step(&mut self) -> Result<(), PoolError> {
        self.service_units()?;
        let now = self.clock.now_ms();
        let free = self.free();
        if self.stream.is_none() && !self.stopped && free > 0 && now >= self.reconnect_at_ms {
            match self.port.recv(free) {
                Ok(()) => {
                    self.stream = Some(Stream {
                        asked: free,
                        got: 0,
                    })
                }
                Err(err) => self.stream_lost(now, &err),
            }
        }
        let wait = self.wait_ms(now);
        let Some(stream) = self.stream.as_mut() else {
            self.clock.sleep_ms(wait);
            return Ok(());
        };
        match self.port.next_event(wait) {
            Ok(RecvEvent::Delivery(delivery)) => {
                stream.got += 1;
                if stream.got >= stream.asked {
                    self.stream = None;
                }
                self.stream_failures = 0;
                self.on_delivery(delivery)
            }
            Ok(RecvEvent::Idle) => Ok(()),
            Ok(RecvEvent::End) => {
                self.stream_lost(now, &PortError("stream ended".into()));
                Ok(())
            }
            Err(err) => {
                self.stream_lost(now, &err);
                Ok(())
            }
        }
    }

    /// Stop `Recv`, Nack the running units with 0, release and drain the slots (D6).
    pub fn shutdown(&mut self) -> Result<(), PoolError> {
        self.stopped = true;
        self.stream = None;
        for unit in std::mem::take(&mut self.units) {
            self.runner.cancel(&unit.token);
            let result = self.port.nack(&unit.token, 0);
            Self::note("Nack", &unit.token, result);
            self.slots[unit.slot] = self.slots[unit.slot].clone().release()?;
        }
        for slot in &mut self.slots {
            *slot = slot.clone().drain()?;
        }
        Ok(())
    }

    fn free(&self) -> u32 {
        let warm = self
            .slots
            .iter()
            .filter(|s| s.state() == SlotState::Warm)
            .count();
        u32::try_from(warm).unwrap_or(u32::MAX)
    }

    /// The next wait: at most `poll_ms`, and not past the next Extend or the reconnect.
    fn wait_ms(&self, now: u64) -> u64 {
        let mut wait = self.cfg.poll_ms.max(1);
        for unit in &self.units {
            wait = wait.min(unit.due_ms.saturating_sub(now));
        }
        if self.stream.is_none() && self.reconnect_at_ms > now {
            wait = wait.min(self.reconnect_at_ms - now);
        }
        wait.max(1)
    }

    /// The stream ended or failed: back off, doubling from 1 s up to 60 s (D5 table). A
    /// delivery resets the count.
    fn stream_lost(&mut self, now: u64, why: &PortError) {
        tracing::warn!(%why, "Recv stream lost");
        self.stream = None;
        self.stream_failures = self.stream_failures.saturating_add(1);
        self.reconnect_at_ms = now + nack_delay_ms(self.stream_failures);
    }

    fn note(rpc: &str, token: &str, result: Result<(), PortError>) {
        if let Err(err) = result {
            tracing::warn!(rpc, token, %err, "pool call failed; Cortex redelivers");
        }
    }

    /// Whole seconds since the first issue (spec 0026, Lease bridge step 1).
    fn now_s(&mut self, now_ms: u64) -> u64 {
        let epoch = *self.epoch_ms.get_or_insert(now_ms);
        now_ms.saturating_sub(epoch) / 1000
    }

    fn on_delivery(&mut self, d: PoolDelivery) -> Result<(), PoolError> {
        // D9 first: before any slot check, lease or runner start.
        if d.lease_ms < MIN_LEASE_MS {
            let result = self
                .port
                .nack(&d.delivery_token, nack_delay_ms(d.delivery_attempt));
            Self::note("Nack", &d.delivery_token, result);
            self.stopped = true;
            self.stream = None;
            return Err(PoolError::LeaseTooShort {
                lease_ms: d.lease_ms,
            });
        }
        let now_ms = self.clock.now_ms();
        let Some(idx) = self.slots.iter().position(|s| s.state() == SlotState::Warm) else {
            let result = self
                .port
                .nack(&d.delivery_token, nack_delay_ms(d.delivery_attempt));
            Self::note("Nack", &d.delivery_token, result);
            return Ok(());
        };
        let now_s = self.now_s(now_ms);
        self.next_lease += 1;
        let n = self.next_lease;
        let held: Vec<&Lease> = self.units.iter().map(|u| &u.lease).collect();
        let lease = issue(
            &held,
            LeaseId::new(format!("pool-lease-{n}"))?,
            self.slots[idx].id().clone(),
            PrincipalRef::new(self.cfg.peer_id.clone())?,
            LeaseToken::new(format!("{}-{n}", self.cfg.peer_id))?,
            Budget::new(self.cfg.max_tokens, ttl_secs(d.lease_ms) + 1)?,
            now_s,
            d.lease_ms,
        )?;
        self.slots[idx] = self.slots[idx].clone().lease()?;
        let outcome = match self.runner.start(&d) {
            Ok(()) => None,
            Err(TaskError::Payload(reason)) => {
                Self::note(
                    "Term",
                    &d.delivery_token,
                    self.port.term(&d.delivery_token, &reason),
                );
                Some(())
            }
            Err(TaskError::Recoverable(_)) => {
                let delay = nack_delay_ms(d.delivery_attempt);
                Self::note(
                    "Nack",
                    &d.delivery_token,
                    self.port.nack(&d.delivery_token, delay),
                );
                Some(())
            }
        };
        if outcome.is_some() {
            self.slots[idx] = self.slots[idx].clone().release()?;
            return Ok(());
        }
        self.units.push(Unit {
            token: d.delivery_token,
            attempt: d.delivery_attempt,
            lease_ms: d.lease_ms,
            slot: idx,
            lease,
            due_ms: now_ms + d.lease_ms / 3,
            extend_failures: 0,
        });
        Ok(())
    }

    /// Poll each unit's task, and run the Extend that is due.
    fn service_units(&mut self) -> Result<(), PoolError> {
        let mut i = 0;
        while i < self.units.len() {
            let token = self.units[i].token.clone();
            match self.runner.poll(&token) {
                TaskStatus::Succeeded => {
                    Self::note("Ack", &token, self.port.ack(std::slice::from_ref(&token)));
                    self.finish(i)?;
                    continue;
                }
                TaskStatus::Failed(_) => {
                    let delay = nack_delay_ms(self.units[i].attempt);
                    Self::note("Nack", &token, self.port.nack(&token, delay));
                    self.finish(i)?;
                    continue;
                }
                TaskStatus::Running => {}
            }
            let now_ms = self.clock.now_ms();
            if now_ms >= self.units[i].due_ms {
                let step = (self.units[i].lease_ms / 3).max(1);
                self.units[i].due_ms += step;
                let extended = self.port.extend(&token).is_ok();
                let now_s = self.now_s(now_ms);
                let unit = &mut self.units[i];
                // The CAS lease follows the Cortex lease: a failed Extend does not renew.
                let kept = if extended {
                    unit.extend_failures = 0;
                    renew(&unit.lease, now_s, unit.lease_ms)
                        .map(|l| unit.lease = l)
                        .is_ok()
                } else {
                    unit.extend_failures += 1;
                    unit.extend_failures < 2
                };
                if !kept {
                    // No Ack: Cortex redelivers after `lease_ms`.
                    self.runner.cancel(&token);
                    self.finish(i)?;
                    continue;
                }
            }
            i += 1;
        }
        Ok(())
    }

    /// Drop unit `i` and release its slot. Dropping the unit drops its lease from R1's view.
    fn finish(&mut self, i: usize) -> Result<(), PoolError> {
        let unit = self.units.remove(i);
        self.slots[unit.slot] = self.slots[unit.slot].clone().release()?;
        Ok(())
    }
}
