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

/// How long a fake call takes to answer.
#[derive(Clone, Copy)]
enum Answer {
    Ms(u64),
    Hang,
}

type Answerer = Rc<RefCell<Box<dyn Fn(&str, &str) -> Answer>>>;

/// A fake port: it logs each call with the clock reading, and serves scripted events. A call
/// advances the fake clock by `min(its answer time, within_ms)` and fails when cut (D12).
#[derive(Clone)]
struct Port {
    clock: Clk,
    calls: Rc<RefCell<Vec<(u64, Call)>>>,
    events: Rc<RefCell<VecDeque<(u64, RecvEvent)>>>,
    extend_failures: Rc<Cell<u32>>,
    answer: Answerer,
    withins: Rc<RefCell<Vec<(String, u64)>>>,
    landed: Rc<RefCell<Vec<(String, u64)>>>,
}

impl Port {
    fn log(&self, call: Call) {
        self.calls.borrow_mut().push((self.clock.now_ms(), call));
    }

    fn serve(&self, rpc: &str, token: &str, within: u64) -> Result<(), PortError> {
        self.withins.borrow_mut().push((rpc.into(), within));
        let (spent, ok) = match (self.answer.borrow())(rpc, token) {
            Answer::Ms(ms) => (ms.min(within), ms <= within),
            Answer::Hang => (within, false),
        };
        self.clock.sleep_ms(spent);
        if !ok {
            return Err(PortError(format!("{rpc} cut at {within} ms")));
        }
        if rpc == "extend" {
            self.landed
                .borrow_mut()
                .push((token.into(), self.clock.now_ms()));
        }
        Ok(())
    }
}

impl PoolWork for Port {
    fn recv(&mut self, free_slots: u32, within_ms: u64) -> Result<(), PortError> {
        self.log(Call::Recv(free_slots));
        self.serve("recv", "", within_ms)
    }
    fn next_event(&mut self, wait_ms: u64) -> Result<RecvEvent, PortError> {
        let now = self.clock.now_ms();
        let front = self.events.borrow().front().map(|(at, _)| *at);
        match front {
            Some(at) if at <= now => Ok(self.events.borrow_mut().pop_front().unwrap().1),
            Some(at) => {
                self.clock.sleep_ms(wait_ms.min(at - now));
                Ok(RecvEvent::Idle)
            }
            None => {
                self.clock.sleep_ms(wait_ms);
                Ok(RecvEvent::Idle)
            }
        }
    }
    fn ack(&mut self, tokens: &[String], within_ms: u64) -> Result<(), PortError> {
        self.log(Call::Ack(tokens.to_vec()));
        self.serve("ack", &tokens[0], within_ms)
    }
    fn extend(&mut self, token: &str, within_ms: u64) -> Result<(), PortError> {
        self.log(Call::Extend(token.into()));
        if self.extend_failures.get() > 0 {
            self.extend_failures.set(self.extend_failures.get() - 1);
            return Err(PortError("extend refused".into()));
        }
        self.serve("extend", token, within_ms)
    }
    fn nack(&mut self, token: &str, delay_ms: u64, within_ms: u64) -> Result<(), PortError> {
        self.log(Call::Nack(token.into(), delay_ms));
        self.serve("nack", token, within_ms)
    }
    fn term(&mut self, token: &str, reason: &str, within_ms: u64) -> Result<(), PortError> {
        self.log(Call::Term(token.into(), reason.into()));
        self.serve("term", token, within_ms)
    }
}

#[derive(Clone, Default)]
struct Runner {
    start_error: Rc<RefCell<Option<TaskError>>>,
    status: Rc<RefCell<HashMap<String, TaskStatus>>>,
    cancelled: Rc<RefCell<Vec<String>>>,
    started: Rc<Cell<u32>>,
    clock: Clk,
    finish_at: Rc<RefCell<HashMap<String, u64>>>,
}

impl TaskRunner for Runner {
    fn start(&mut self, _d: &PoolDelivery) -> Result<(), TaskError> {
        self.started.set(self.started.get() + 1);
        self.start_error.borrow().clone().map_or(Ok(()), Err)
    }
    fn poll(&mut self, token: &str) -> TaskStatus {
        let done = self
            .finish_at
            .borrow()
            .get(token)
            .is_some_and(|at| *at <= self.clock.now_ms());
        if done {
            return TaskStatus::Succeeded;
        }
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
    rig_at(slots, events.into_iter().map(|e| (0, e)).collect())
}

/// A rig whose events arrive at the given clock readings.
fn rig_at(slots: usize, events: Vec<(u64, RecvEvent)>) -> Rig {
    let clock = Clk::default();
    let port = Port {
        clock: clock.clone(),
        calls: Rc::default(),
        events: Rc::new(RefCell::new(events.into())),
        extend_failures: Rc::default(),
        answer: Rc::new(RefCell::new(Box::new(|_, _| Answer::Ms(0)))),
        withins: Rc::default(),
        landed: Rc::default(),
    };
    let runner = Runner {
        clock: clock.clone(),
        ..Runner::default()
    };
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
        call_cap_ms: 2000,
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
fn cortex_pool_filled_stream_dropped_when_slot_frees() {
    // A filled stream (2 asked, 2 delivered) must not stall the pool: when t1 finishes, a second
    // Recv carries the new free count of 1.
    let mut rig = rig(2, vec![delivery("t1", 3000, 1), delivery("t2", 3000, 1)]);
    rig.run_to(200);
    rig.runner
        .status
        .borrow_mut()
        .insert("t1".into(), TaskStatus::Succeeded);
    rig.adapter.step().unwrap();
    let calls = rig.calls();
    let recvs: Vec<&Call> = calls
        .iter()
        .filter(|c| matches!(c, Call::Recv(_)))
        .collect();
    assert_eq!(recvs, [&Call::Recv(2), &Call::Recv(1)], "{calls:?}");
    let ack = calls
        .iter()
        .position(|c| matches!(c, Call::Ack(_)))
        .unwrap();
    let second = calls
        .iter()
        .rposition(|c| matches!(c, Call::Recv(_)))
        .unwrap();
    assert!(ack < second, "the slot frees before the new ask");
}

#[test]
fn cortex_pool_filled_stream_end_no_backoff() {
    // A stream that delivered all it was asked for may End: no backoff, so the next Recv comes
    // as soon as a slot frees.
    let mut filled = rig(1, vec![delivery("t1", 3000, 1), RecvEvent::End]);
    filled.run_to(200);
    filled
        .runner
        .status
        .borrow_mut()
        .insert("t1".into(), TaskStatus::Succeeded);
    filled.adapter.step().unwrap();
    let times = filled.times(|c| matches!(c, Call::Recv(_)));
    assert_eq!(times.len(), 2, "{:?}", filled.calls());
    assert!(
        times[1] < 1000,
        "no backoff after a filled stream: {times:?}"
    );
    // A stream that ends before it delivered the free count backs off 1 s.
    let mut unfilled = rig(2, vec![delivery("t1", 3000, 1), RecvEvent::End]);
    unfilled.run_to(1500);
    let times = unfilled.times(|c| matches!(c, Call::Recv(_)));
    assert_eq!(times, [0, 1000]);
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

/// Set the answer time of each fake call: `f(rpc, token)`.
fn answer(rig: &Rig, f: impl Fn(&str, &str) -> Answer + 'static) {
    *rig.port.answer.borrow_mut() = Box::new(f);
}

fn hung_extends(rig: &Rig) {
    answer(rig, |rpc, _| {
        if rpc == "extend" {
            Answer::Hang
        } else {
            Answer::Ms(0)
        }
    });
}

#[test]
fn cortex_pool_extend_bounded_by_shortest_tick() {
    let mut rig = rig(2, vec![delivery("a", 30000, 1), delivery("b", 3000, 1)]);
    hung_extends(&rig);
    rig.run_to(3500);
    // The quarter is 750 ms, from the shorter lease. Each hung call is cut at 750 ms, and the
    // 3000 ms unit keeps its 1000 ms tick: two cuts release it by 2750 ms.
    assert_eq!(
        rig.times(|c| matches!(c, Call::Extend(t) if t == "b")),
        [1000, 2000]
    );
    let withins: Vec<u64> = (rig.port.withins.borrow().iter())
        .filter(|(rpc, _)| rpc == "extend")
        .map(|(_, w)| *w)
        .collect();
    assert_eq!(withins, [750, 750]);
    assert_eq!(rig.adapter.releases(), [(2750, 1)]);
    assert_eq!(
        rig.states()[0],
        SlotState::Leased,
        "the 30 s unit is not due yet"
    );
}

#[test]
fn cortex_pool_extend_round_budget_two_hung_units() {
    let mut rig = rig(2, vec![delivery("a", 3000, 1), delivery("b", 3000, 1)]);
    hung_extends(&rig);
    rig.run_to(4500);
    let mut released = rig.adapter.releases().to_vec();
    released.sort_by_key(|r| r.1);
    assert_eq!(released, [(2750, 0), (2750, 1)]);
}

#[test]
fn cortex_pool_extend_round_rotates() {
    let mut rig = rig(2, vec![delivery("a", 3000, 1), delivery("b", 3000, 1)]);
    answer(&rig, |rpc, tok| {
        if rpc == "extend" && tok == "a" {
            Answer::Hang
        } else {
            Answer::Ms(0)
        }
    });
    rig.run_to(3500);
    // R1 cuts A and skips B; R2 calls B first, so B never fails twice in a row.
    assert_eq!(rig.adapter.releases(), [(2750, 0)]);
    assert_eq!(rig.states()[1], SlotState::Leased);
    assert!(rig.port.landed.borrow().iter().any(|(t, _)| t == "b"));
}

#[test]
fn cortex_pool_extend_slow_broker() {
    let events = vec![
        delivery("a", 3000, 1),
        delivery("b", 3000, 1),
        delivery("c", 3000, 1),
    ];
    let mut rig = rig(3, events);
    answer(&rig, |rpc, _| {
        if rpc == "extend" {
            Answer::Ms(700)
        } else {
            Answer::Ms(0)
        }
    });
    rig.run_to(5000);
    // B is cut twice and stops at 2750 ms; A and C alternate.
    assert_eq!(rig.adapter.releases(), [(2750, 1)]);
    let landed = rig.port.landed.borrow().clone();
    let at = |t: &str| -> Vec<u64> { landed.iter().filter(|l| l.0 == t).map(|l| l.1).collect() };
    assert_eq!(at("a"), [1700, 3700]);
    assert_eq!(at("c"), [2700, 4700]);
    // No Extend lands after its unit's Cortex lease: each landing renews it by lease_ms.
    for token in ["a", "c"] {
        let mut until = 3000;
        for t in at(token) {
            assert!(t <= until, "{token} landed at {t}, lease ended at {until}");
            until = t + 3000;
        }
    }
}

#[test]
fn cortex_pool_extend_deadline_from_due_time() {
    let mut rig = rig_at(
        2,
        vec![(0, delivery("a", 3000, 1)), (900, delivery("b", 3000, 1))],
    );
    hung_extends(&rig);
    rig.run_to(4000);
    // A: due 1000, deadline 1750 ... released at 2750. B: due 1900, released at 3650.
    assert_eq!(rig.adapter.releases(), [(2750, 0), (3650, 1)]);
}

#[test]
fn cortex_pool_extend_deadline_bounds_other_rpcs() {
    let mut rig = rig(2, vec![delivery("a", 3000, 1), delivery("x", 3000, 1)]);
    rig.runner.finish_at.borrow_mut().insert("x".into(), 1750);
    answer(&rig, |rpc, _| match rpc {
        "extend" | "ack" => Answer::Hang,
        _ => Answer::Ms(0),
    });
    rig.run_to(4000);
    // A's Extend is cut at 1750. X's Ack then hangs, but it is bounded by A's next deadline
    // (2750), not by the 2000 ms cap: A's second failure lands at 2750, not 3750.
    let a_release = rig.adapter.releases().iter().find(|r| r.1 == 0).copied();
    assert_eq!(a_release, Some((2750, 0)), "{:?}", rig.adapter.releases());
    let within = (rig
        .port
        .withins
        .borrow()
        .iter()
        .find(|(rpc, _)| rpc == "ack"))
    .map(|w| w.1);
    let start = rig.times(|c| matches!(c, Call::Ack(_)))[0];
    assert_eq!(
        within.map(|w| start + w),
        Some(2750),
        "the bound ends at A's deadline"
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

// The gRPC client against an in-process fake PoolWork server on 127.0.0.1:0. Each test calls
// one RPC and checks the request as the server decoded it, as bytes: the field numbers and the
// absence of a pool name are then pinned to the spec's Wire contract table.
mod grpc {
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use cas_engine::cortex_pool::pb::{
        self,
        pool_work_server::{PoolWork as PoolWorkService, PoolWorkServer},
    };
    use cas_engine::cortex_pool::{GrpcPoolWork, GrpcTimeouts, PoolDelivery, PoolWork, RecvEvent};
    use prost::Message;
    use std::time::{Duration, Instant};
    use tonic::codegen::tokio_stream::{self, Stream};
    use tonic::{Request, Response, Status};

    /// The encoded request of each RPC, in arrival order.
    type Seen = Arc<Mutex<Vec<(&'static str, Vec<u8>)>>>;

    #[derive(Clone, Default)]
    struct Fake {
        seen: Seen,
    }

    impl Fake {
        fn note(&self, rpc: &'static str, msg: &impl Message) {
            self.seen.lock().unwrap().push((rpc, msg.encode_to_vec()));
        }
    }

    #[tonic::async_trait]
    impl PoolWorkService for Fake {
        type RecvStream = Pin<Box<dyn Stream<Item = Result<pb::PoolDelivery, Status>> + Send>>;

        async fn recv(
            &self,
            request: Request<pb::PoolRecvRequest>,
        ) -> Result<Response<Self::RecvStream>, Status> {
            self.note("recv", request.get_ref());
            if request.get_ref().free_slots == 9 {
                // Hold the stream open and silent.
                return Ok(Response::new(Box::pin(tokio_stream::pending())));
            }
            let delivery = pb::PoolDelivery {
                delivery_token: "t1".into(),
                stream_seq: 7,
                message: Some(pb::LaneMessage {
                    subject: "s".into(),
                    body: "b".into(),
                    ..Default::default()
                }),
                lease_ms: 3000,
                delivery_attempt: 2,
            };
            Ok(Response::new(Box::pin(tokio_stream::iter([Ok(delivery)]))))
        }
        async fn ack(&self, request: Request<pb::PoolAckRequest>) -> Result<Response<()>, Status> {
            self.note("ack", request.get_ref());
            Ok(Response::new(()))
        }
        async fn extend(
            &self,
            request: Request<pb::PoolExtendRequest>,
        ) -> Result<Response<()>, Status> {
            self.note("extend", request.get_ref());
            if request.get_ref().delivery_token == "hang" {
                std::future::pending::<()>().await;
            }
            Ok(Response::new(()))
        }
        async fn nack(
            &self,
            request: Request<pb::PoolNackRequest>,
        ) -> Result<Response<()>, Status> {
            self.note("nack", request.get_ref());
            Ok(Response::new(()))
        }
        async fn term(
            &self,
            request: Request<pb::PoolTermRequest>,
        ) -> Result<Response<()>, Status> {
            self.note("term", request.get_ref());
            Ok(Response::new(()))
        }
    }

    /// Serve the fake on its own thread and runtime; the address it listens on comes back.
    fn serve() -> (Fake, Seen, std::net::SocketAddr) {
        let fake = Fake::default();
        let seen = fake.seen.clone();
        let fake_handle = fake.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let incoming =
                    tonic::transport::server::TcpIncoming::bind("127.0.0.1:0".parse().unwrap())
                        .unwrap();
                tx.send(incoming.local_addr().unwrap()).unwrap();
                // `add_service` needs tonic's `router` feature; this call takes the service.
                tonic::transport::Server::builder()
                    .serve_with_incoming(PoolWorkServer::new(fake), incoming)
                    .await
                    .unwrap();
            });
        });
        (fake_handle, seen, rx.recv().unwrap())
    }

    /// Serve the fake, and connect a client from plain test code.
    fn rig() -> (GrpcPoolWork, Seen) {
        let (_, seen, addr) = serve();
        let client = GrpcPoolWork::connect(format!("http://{addr}"), "p1").unwrap();
        (client, seen)
    }

    /// `peer_id = "p1"` is field 1: 0x0a, length 2.
    const PEER: [u8; 4] = [0x0a, 2, b'p', b'1'];

    fn wire(rpc: &str, seen: &Seen) -> Vec<u8> {
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "one request");
        assert_eq!(seen[0].0, rpc);
        seen[0].1.clone()
    }

    #[test]
    fn cortex_pool_grpc_recv() {
        let (mut client, seen) = rig();
        client.recv(3, 2000).unwrap();
        // free_slots = 3 is field 2, a varint: 0x10, 3.
        assert_eq!(wire("recv", &seen), [PEER.as_slice(), &[0x10, 3]].concat());
        let event = client.next_event(2000).unwrap();
        let want = PoolDelivery {
            delivery_token: "t1".into(),
            stream_seq: 7,
            subject: "s".into(),
            body: "b".into(),
            lease_ms: 3000,
            delivery_attempt: 2,
        };
        assert_eq!(event, RecvEvent::Delivery(want));
        assert_eq!(client.next_event(2000).unwrap(), RecvEvent::End);
    }

    #[test]
    fn cortex_pool_grpc_ack() {
        let (mut client, seen) = rig();
        client.ack(&["a".into(), "b".into()], 2000).unwrap();
        // delivery_tokens is field 2, repeated: 0x12 for each.
        assert_eq!(
            wire("ack", &seen),
            [PEER.as_slice(), &[0x12, 1, b'a', 0x12, 1, b'b']].concat()
        );
    }

    #[test]
    fn cortex_pool_grpc_extend() {
        let (mut client, seen) = rig();
        client.extend("t1", 2000).unwrap();
        assert_eq!(
            wire("extend", &seen),
            [PEER.as_slice(), &[0x12, 2, b't', b'1']].concat()
        );
    }

    #[test]
    fn cortex_pool_grpc_nack() {
        let (mut client, seen) = rig();
        client.nack("t1", 1500, 2000).unwrap();
        // delay_ms = 1500 is field 3, a varint: 0x18, 0xdc, 0x0b.
        let want = [PEER.as_slice(), &[0x12, 2, b't', b'1', 0x18, 0xdc, 0x0b]].concat();
        assert_eq!(wire("nack", &seen), want);
    }

    #[test]
    fn cortex_pool_grpc_term() {
        let (mut client, seen) = rig();
        client.term("t1", "bad", 2000).unwrap();
        // reason is field 3, a string: 0x1a.
        let want = [
            PEER.as_slice(),
            &[0x12, 2, b't', b'1', 0x1a, 3, b'b', b'a', b'd'],
        ]
        .concat();
        assert_eq!(wire("term", &seen), want);
    }

    #[test]
    fn cortex_pool_grpc_quiet_stream_is_idle() {
        let (mut client, _seen) = rig();
        client.recv(9, 2000).unwrap();
        // The server holds the stream open and sends nothing: a timed-out wait is Idle, never End.
        assert_eq!(client.next_event(50).unwrap(), RecvEvent::Idle);
        assert_eq!(client.next_event(50).unwrap(), RecvEvent::Idle);
    }

    /// Call `extend("hang", within_ms)` on a server that never answers it, with the channel
    /// `request` cap; return whether it failed and how long it took.
    fn hung_extend(request: Duration, within_ms: u64) -> (bool, Duration) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (_, _, addr) = serve();
            let timeouts = GrpcTimeouts {
                connect: Duration::from_secs(2),
                request,
            };
            let mut client =
                GrpcPoolWork::connect_with(format!("http://{addr}"), "p1", timeouts).unwrap();
            let started = Instant::now();
            let result = client.extend("hang", within_ms);
            let _ = tx.send((result.is_err(), started.elapsed()));
        });
        rx.recv_timeout(Duration::from_secs(10))
            .expect("Extend returned")
    }

    #[test]
    fn cortex_pool_grpc_hung_extend_times_out() {
        // The channel cap (300 ms) cuts a hung Extend when the caller's bound is longer.
        let (failed, took) = hung_extend(Duration::from_millis(300), 5000);
        assert!(failed, "a hung Extend is an error");
        assert!(took < Duration::from_secs(3), "{took:?}");
    }

    #[test]
    fn cortex_pool_grpc_extend_bound_in_the_call() {
        // The caller's bound (300 ms, D12) cuts the call long before the 4 s channel cap.
        let (failed, took) = hung_extend(Duration::from_secs(4), 300);
        assert!(failed, "a cut Extend is an error");
        assert!(took < Duration::from_secs(2), "{took:?}");
    }
}
