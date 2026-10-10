//! Tests of the Cortex pool-member adapter (spec 0026). A fake port, a fake runner and a fake
//! clock stand in for Cortex, the harness and time, so no network is touched.
#![cfg(feature = "cortex-grpc")]

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use cas_domain::{
    Budget, Lease, LeaseId, LeaseToken, PrincipalRef, Slot, SlotId, SlotState, WorktreeRef,
};
use cas_engine::cortex_pool::{
    issue, nack_delay_ms, renew, ttl_secs, AdapterConfig, Clock, PoolAdapter, PoolDelivery,
    PoolError, PoolWork, PortError, RecvEvent, TaskError, TaskRunner, TaskStatus,
};

#[derive(Clone, Default)]
struct Clk(Rc<Cell<u64>>);

impl Clock for Clk {
    fn now_ms(&self) -> u64 {
        self.0.get()
    }
    fn sleep_ms(&self, ms: u64) {
        self.0.set(self.0.get() + ms);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Recv(u32),
    Ack(Vec<String>),
    Extend(String),
    Nack(String, u64),
    Term(String, String),
}

/// A fake port: it logs each call with the clock reading, and serves scripted events.
#[derive(Clone)]
struct Port {
    clock: Clk,
    calls: Rc<RefCell<Vec<(u64, Call)>>>,
    events: Rc<RefCell<VecDeque<RecvEvent>>>,
    extend_failures: Rc<Cell<u32>>,
}

impl Port {
    fn log(&self, call: Call) {
        self.calls.borrow_mut().push((self.clock.now_ms(), call));
    }
}

impl PoolWork for Port {
    fn recv(&mut self, free_slots: u32) -> Result<(), PortError> {
        self.log(Call::Recv(free_slots));
        Ok(())
    }
    fn next_event(&mut self, wait_ms: u64) -> Result<RecvEvent, PortError> {
        if let Some(event) = self.events.borrow_mut().pop_front() {
            return Ok(event);
        }
        self.clock.sleep_ms(wait_ms);
        Ok(RecvEvent::Idle)
    }
    fn ack(&mut self, tokens: &[String]) -> Result<(), PortError> {
        self.log(Call::Ack(tokens.to_vec()));
        Ok(())
    }
    fn extend(&mut self, token: &str) -> Result<(), PortError> {
        self.log(Call::Extend(token.into()));
        if self.extend_failures.get() > 0 {
            self.extend_failures.set(self.extend_failures.get() - 1);
            return Err(PortError("extend refused".into()));
        }
        Ok(())
    }
    fn nack(&mut self, token: &str, delay_ms: u64) -> Result<(), PortError> {
        self.log(Call::Nack(token.into(), delay_ms));
        Ok(())
    }
    fn term(&mut self, token: &str, reason: &str) -> Result<(), PortError> {
        self.log(Call::Term(token.into(), reason.into()));
        Ok(())
    }
}

#[derive(Clone, Default)]
struct Runner {
    start_error: Rc<RefCell<Option<TaskError>>>,
    status: Rc<RefCell<HashMap<String, TaskStatus>>>,
    cancelled: Rc<RefCell<Vec<String>>>,
    started: Rc<Cell<u32>>,
}

impl TaskRunner for Runner {
    fn start(&mut self, _d: &PoolDelivery) -> Result<(), TaskError> {
        self.started.set(self.started.get() + 1);
        self.start_error.borrow().clone().map_or(Ok(()), Err)
    }
    fn poll(&mut self, token: &str) -> TaskStatus {
        self.status
            .borrow()
            .get(token)
            .cloned()
            .unwrap_or(TaskStatus::Running)
    }
    fn cancel(&mut self, token: &str) {
        self.cancelled.borrow_mut().push(token.into());
    }
}

struct Rig {
    adapter: PoolAdapter<Port, Runner, Clk>,
    port: Port,
    runner: Runner,
    clock: Clk,
}

fn delivery(token: &str, lease_ms: u64, attempt: u32) -> RecvEvent {
    RecvEvent::Delivery(PoolDelivery {
        delivery_token: token.into(),
        stream_seq: 1,
        subject: "s".into(),
        body: "b".into(),
        lease_ms,
        delivery_attempt: attempt,
    })
}

fn rig(slots: usize, events: Vec<RecvEvent>) -> Rig {
    let clock = Clk::default();
    let port = Port {
        clock: clock.clone(),
        calls: Rc::default(),
        events: Rc::new(RefCell::new(events.into())),
        extend_failures: Rc::default(),
    };
    let runner = Runner::default();
    let warm = (0..slots)
        .map(|i| {
            Slot::new(SlotId::new(format!("slot-{i}")).unwrap())
                .warm(WorktreeRef::new(format!("wt-{i}")).unwrap())
                .unwrap()
        })
        .collect();
    let cfg = AdapterConfig {
        peer_id: "peer-1".into(),
        task_budget: Budget::new(1000, 3600).unwrap(),
        poll_ms: 100,
    };
    let adapter = PoolAdapter::new(cfg, port.clone(), runner.clone(), clock.clone(), warm);
    Rig {
        adapter,
        port,
        runner,
        clock,
    }
}

impl Rig {
    /// Step until the fake clock reaches `ms`, at most 10 000 steps.
    fn run_to(&mut self, ms: u64) {
        for _ in 0..10_000 {
            if self.clock.now_ms() >= ms {
                return;
            }
            self.adapter.step().unwrap();
        }
        panic!("the clock did not reach {ms}");
    }
    fn calls(&self) -> Vec<Call> {
        self.port
            .calls
            .borrow()
            .iter()
            .map(|(_, c)| c.clone())
            .collect()
    }
    fn times(&self, pick: impl Fn(&Call) -> bool) -> Vec<u64> {
        self.port
            .calls
            .borrow()
            .iter()
            .filter(|(_, c)| pick(c))
            .map(|(t, _)| *t)
            .collect()
    }
    fn states(&self) -> Vec<SlotState> {
        self.adapter.slots().iter().map(Slot::state).collect()
    }
}

fn lease_at(now: u64, lease_ms: u64) -> Lease {
    issue(
        &[],
        LeaseId::new("lease-1").unwrap(),
        SlotId::new("slot-0").unwrap(),
        PrincipalRef::new("peer-1").unwrap(),
        LeaseToken::new("tok").unwrap(),
        Budget::new(1, 1).unwrap(),
        now,
        lease_ms,
    )
    .unwrap()
}

#[test]
fn cortex_pool_recv_asks_free_slots() {
    let mut rig = rig(2, vec![]);
    rig.adapter.step().unwrap();
    assert_eq!(rig.calls(), [Call::Recv(2)]);
}

#[test]
fn cortex_pool_ack_releases_to_warm() {
    let mut rig = rig(1, vec![delivery("t1", 3000, 1)]);
    rig.run_to(200);
    assert_eq!(rig.states(), [SlotState::Leased]);
    rig.runner
        .status
        .borrow_mut()
        .insert("t1".into(), TaskStatus::Succeeded);
    rig.adapter.step().unwrap();
    assert!(rig.calls().contains(&Call::Ack(vec!["t1".into()])));
    assert_eq!(rig.states(), [SlotState::Warm]);
}

#[test]
fn cortex_pool_extend_every_third_of_lease() {
    let mut rig = rig(1, vec![delivery("t1", 3000, 1)]);
    rig.run_to(3500);
    let ticks = rig.times(|c| matches!(c, Call::Extend(_)));
    assert_eq!(ticks, [1000, 2000, 3000]);
}

#[test]
fn cortex_pool_renew_tracks_wall_time() {
    let mut lease = lease_at(0, 3000);
    let ttl = ttl_secs(3000);
    assert_eq!(ttl, 3);
    for now in 1..=30 {
        lease = renew(&lease, now, 3000).unwrap();
        assert!(
            lease.expires_at > now && lease.expires_at - now <= ttl + 1,
            "now {now}"
        );
    }
}

#[test]
fn cortex_pool_renew_clamps_to_one_second() {
    assert_eq!(
        [ttl_secs(0), ttl_secs(900), ttl_secs(1000), ttl_secs(1001)],
        [1, 1, 1, 2]
    );
    let lease = lease_at(5, 900);
    assert_eq!(lease.expires_at, 7, "ttl 1 plus the extra second");
}

#[test]
fn cortex_pool_short_lease_survives_second_boundaries() {
    let mut lease = lease_at(0, 900);
    for tick in 1..=10u64 {
        let now = tick * 300 / 1000;
        lease = renew(&lease, now, 900).unwrap_or_else(|e| panic!("tick {tick}: {e}"));
        assert!(!lease.is_expired(now));
    }
}

#[test]
fn cortex_pool_nack_below_min_lease_stops_recv() {
    for lease_ms in [0u64, 299] {
        let mut rig = rig(1, vec![delivery("t1", lease_ms, 2)]);
        let err = rig.adapter.step().unwrap_err();
        assert_eq!(err, PoolError::LeaseTooShort { lease_ms });
        let text = err.to_string();
        assert!(
            text.contains("lease_ms") && text.contains(&lease_ms.to_string()),
            "{text}"
        );
        assert_eq!(rig.calls(), [Call::Recv(1), Call::Nack("t1".into(), 2000)]);
        assert_eq!(rig.states(), [SlotState::Warm], "no slot is leased");
        // No second Recv and no reconnect, however long the adapter runs.
        for _ in 0..200 {
            rig.adapter.step().unwrap();
        }
        assert_eq!(rig.calls().len(), 2, "{:?}", rig.calls());
    }
}

#[test]
fn cortex_pool_nack_delay_grows() {
    let got: Vec<u64> = [0, 1, 2, 8, u32::MAX]
        .iter()
        .map(|a| nack_delay_ms(*a))
        .collect();
    assert_eq!(got, [1000, 1000, 2000, 60000, 60000]);
}

#[test]
fn cortex_pool_nack_when_no_warm_slot() {
    // The fake sends one unit more than the 3 asked for. The extra finds no Warm slot.
    let events = (1..=4)
        .map(|i| delivery(&format!("t{i}"), 3000, 2))
        .collect();
    let mut rig = rig(3, events);
    rig.run_to(200);
    let calls = rig.calls();
    assert_eq!(calls[0], Call::Recv(3));
    assert!(calls.contains(&Call::Nack("t4".into(), 2000)), "{calls:?}");
    assert_eq!(
        rig.states(),
        [SlotState::Leased; 3],
        "no slot took the extra"
    );
    assert_eq!(
        rig.runner.started.get(),
        3,
        "the runner never saw the extra"
    );
    assert_eq!(
        calls.iter().filter(|c| matches!(c, Call::Recv(_))).count(),
        1
    );
}

#[test]
fn cortex_pool_lease_budget_is_task_cap() {
    let mut rig = rig(1, vec![delivery("t1", 3000, 1)]);
    // The task runs 20 s, far past ttl + 1 = 4 s. Every renewal keeps the host's budget.
    rig.run_to(20_000);
    let leases: Vec<_> = rig.adapter.leases().collect();
    assert_eq!(leases.len(), 1, "still running");
    assert_eq!(leases[0].budget, Budget::new(1000, 3600).unwrap());
    assert!(leases[0].expires_at >= 20, "renewed with wall time");
}

#[test]
fn cortex_pool_term_on_bad_payload() {
    let mut rig = rig(1, vec![delivery("t1", 3000, 1)]);
    *rig.runner.start_error.borrow_mut() = Some(TaskError::Payload("not json".into()));
    rig.run_to(200);
    assert_eq!(rig.calls()[1], Call::Term("t1".into(), "not json".into()));
    assert!(!rig
        .calls()
        .iter()
        .any(|c| matches!(c, Call::Ack(_) | Call::Nack(..))));
    assert_eq!(rig.states(), [SlotState::Warm]);
}

#[test]
fn cortex_pool_task_error_nacks_and_releases() {
    let mut rig = rig(1, vec![delivery("t1", 3000, 2)]);
    rig.runner
        .status
        .borrow_mut()
        .insert("t1".into(), TaskStatus::Failed("flaky".into()));
    rig.run_to(200);
    assert!(rig.calls().contains(&Call::Nack("t1".into(), 2000)));
    assert!(!rig
        .calls()
        .iter()
        .any(|c| matches!(c, Call::Ack(_) | Call::Term(..))));
    assert_eq!(rig.states(), [SlotState::Warm]);
}

#[test]
fn cortex_pool_extend_fails_twice_releases() {
    let mut rig = rig(1, vec![delivery("t1", 3000, 1)]);
    rig.port.extend_failures.set(2);
    rig.run_to(3500);
    assert_eq!(
        rig.times(|c| matches!(c, Call::Extend(_))).len(),
        2,
        "it stops after two"
    );
    assert!(!rig
        .calls()
        .iter()
        .any(|c| matches!(c, Call::Ack(_) | Call::Nack(..) | Call::Term(..))));
    assert_eq!(rig.states(), [SlotState::Warm]);
    assert_eq!(*rig.runner.cancelled.borrow(), ["t1"]);
}

#[test]
fn cortex_pool_stream_end_reconnects_with_backoff() {
    let events = vec![delivery("t1", 3000, 1), RecvEvent::End, RecvEvent::End];
    let mut rig = rig(2, events);
    rig.run_to(3500);
    // The first stream asks 2. After each End the adapter waits 1 s, then 2 s, and asks 1.
    let recvs: Vec<(u64, Call)> = rig
        .port
        .calls
        .borrow()
        .iter()
        .filter(|(_, c)| matches!(c, Call::Recv(_)))
        .cloned()
        .collect();
    assert_eq!(
        recvs,
        [
            (0, Call::Recv(2)),
            (1000, Call::Recv(1)),
            (3000, Call::Recv(1))
        ]
    );
    // The running unit keeps its Extend ticks during the backoff.
    assert_eq!(
        rig.times(|c| matches!(c, Call::Extend(_))),
        [1000, 2000, 3000]
    );
}

#[test]
fn cortex_pool_shutdown_nacks_then_drains() {
    let mut rig = rig(2, vec![delivery("t1", 3000, 1), delivery("t2", 3000, 1)]);
    rig.run_to(200);
    assert_eq!(rig.states(), [SlotState::Leased, SlotState::Leased]);
    rig.adapter.shutdown().unwrap();
    let nacks: Vec<Call> = rig
        .calls()
        .into_iter()
        .filter(|c| matches!(c, Call::Nack(..)))
        .collect();
    assert_eq!(
        nacks,
        [Call::Nack("t1".into(), 0), Call::Nack("t2".into(), 0)]
    );
    assert_eq!(rig.states(), [SlotState::Draining, SlotState::Draining]);
}
