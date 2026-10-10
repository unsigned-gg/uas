//! ACP over stdio, against a mocked pool. Spec 0016 (A1, A2), Tests section.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use cas_acp::server::SessionEnd;
use cas_acp::{AcpConfig, AcpServer, PoolPort};
use cas_domain::{Budget, Lease, LeaseToken, PrincipalRef, SlotId};
use cas_pool::PoolError;
use serde_json::Value;

thread_local! {
    // A thread-local, not a static: cargo test runs tests as threads in one process.
    static NOW: Cell<u64> = const { Cell::new(1_000) };
}

fn now() -> u64 {
    NOW.with(Cell::get)
}

fn set_now(t: u64) {
    NOW.with(|cell| cell.set(t));
}

/// Records every call. Acquire grants the slot the lease names.
#[derive(Clone, Default)]
struct MockPool {
    log: Rc<RefCell<Vec<String>>>,
    full: bool,
    suspend_fails: bool,
    release_fails: bool,
}

impl MockPool {
    fn calls(&self) -> Vec<String> {
        self.log.borrow().clone()
    }
}

impl PoolPort for MockPool {
    fn acquire(&self, lease: Lease) -> Result<SlotId, PoolError> {
        self.log
            .borrow_mut()
            .push(format!("acquire:{}", lease.slot.as_str()));
        if self.full {
            return Err(PoolError::CapacityExhausted(2));
        }
        Ok(lease.slot)
    }

    fn suspend(&self, slot: &SlotId) -> Result<(), PoolError> {
        self.log
            .borrow_mut()
            .push(format!("suspend:{}", slot.as_str()));
        if self.suspend_fails {
            return Err(PoolError::NoActiveLease(slot.clone()));
        }
        Ok(())
    }

    fn release(&self, slot: &SlotId, token: &LeaseToken) -> Result<(), PoolError> {
        self.log
            .borrow_mut()
            .push(format!("release:{}:{}", slot.as_str(), token.expose()));
        if self.release_fails {
            return Err(PoolError::NoActiveLease(slot.clone()));
        }
        Ok(())
    }
}

/// Resets the clock to 1000, so every test starts from the same time.
fn config() -> AcpConfig {
    set_now(1_000);
    AcpConfig {
        slots: vec![SlotId::new("slot-1").unwrap()],
        budget: Budget::new(10_000, 600).unwrap(),
        lease_secs: 600,
        holder: PrincipalRef::new("cas-acp").unwrap(),
        clock: now,
    }
}

fn two_slot_config() -> AcpConfig {
    let mut config = config();
    config.slots.push(SlotId::new("slot-2").unwrap());
    config
}

/// A writer whose every write fails, as a closed pipe does.
struct FailingWriter;

impl std::io::Write for FailingWriter {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Feed `input` to the server and parse each response line.
fn serve(server: &mut AcpServer<MockPool>, input: &str) -> Vec<Value> {
    let mut out = Vec::new();
    server.serve(input.as_bytes(), &mut out).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Hand one frame to the server at the current clock. Parse the response, if any.
fn frames_of(server: &mut AcpServer<MockPool>, line: &str) -> Vec<Value> {
    server
        .handle_frame(line)
        .into_iter()
        .map(|reply| serde_json::from_str(&reply).unwrap())
        .collect()
}

fn session_id_of(frames: &[Value]) -> String {
    frames[0]["result"]["sessionId"]
        .as_str()
        .unwrap()
        .to_string()
}

fn cancel_line(session_id: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","method":"session/cancel","params":{{"sessionId":"{session_id}"}}}}"#
    )
}

const INITIALIZE: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}"#;
const SESSION_NEW: &str =
    r#"{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":"/work","mcpServers":[]}}"#;
const SESSION_NEW_AGAIN: &str =
    r#"{"jsonrpc":"2.0","id":3,"method":"session/new","params":{"cwd":"/work","mcpServers":[]}}"#;

#[test]
fn initialize_returns_v1() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool, config());
    let frames = serve(&mut server, &format!("{INITIALIZE}\n"));
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["id"], 1);
    assert_eq!(frames[0]["result"]["protocolVersion"], 1);
}

#[test]
fn session_new_allocates_slot() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let frames = frames_of(&mut server, SESSION_NEW);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["id"], 2);
    let session_id = session_id_of(&frames);
    assert_eq!(pool.calls(), vec!["acquire:slot-1".to_string()]);
    assert_eq!(
        server.workflow_state(&session_id),
        Some(cas_engine::SessionState::Running)
    );
}

#[test]
fn session_new_capacity_error() {
    let pool = MockPool {
        full: true,
        ..MockPool::default()
    };
    let mut server = AcpServer::new(pool, config());
    let frames = serve(&mut server, &format!("{SESSION_NEW}\n"));
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["error"]["code"], -32001);
}

#[test]
fn cancel_freezes_then_releases() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let opened = frames_of(&mut server, SESSION_NEW);
    let session_id = session_id_of(&opened);

    let frames = frames_of(&mut server, &cancel_line(&session_id));

    assert!(
        frames.is_empty(),
        "a notification must get no response frame"
    );
    let calls = pool.calls();
    assert_eq!(calls.len(), 3, "calls: {calls:?}");
    assert_eq!(calls[0], "acquire:slot-1");
    assert_eq!(calls[1], "suspend:slot-1");
    assert!(
        calls[2].starts_with("release:slot-1:token-"),
        "calls: {calls:?}"
    );
    assert!(
        server.workflow_state(&session_id).is_none(),
        "cancelled session is closed"
    );
    assert_eq!(server.session_end(&session_id), Some(SessionEnd::Cancelled));
}

#[test]
fn unknown_method_is_32601() {
    let mut server = AcpServer::new(MockPool::default(), config());
    let frames = serve(
        &mut server,
        "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"session/fly\"}\n",
    );
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["id"], 5);
    assert_eq!(frames[0]["error"]["code"], -32601);
}

#[test]
fn bad_json_is_32700() {
    let mut server = AcpServer::new(MockPool::default(), config());
    let frames = serve(&mut server, "{not json\n");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["id"], Value::Null);
    assert_eq!(frames[0]["error"]["code"], -32700);
}

#[test]
fn suspend_failure_still_releases() {
    let pool = MockPool {
        suspend_fails: true,
        ..MockPool::default()
    };
    let mut server = AcpServer::new(pool.clone(), config());
    let opened = frames_of(&mut server, SESSION_NEW);
    let session_id = session_id_of(&opened);

    let frames = frames_of(&mut server, &cancel_line(&session_id));

    assert!(
        frames.is_empty(),
        "a notification must get no response frame"
    );
    let calls = pool.calls();
    assert!(
        calls.iter().any(|c| c.starts_with("release:slot-1:")),
        "calls: {calls:?}"
    );
    assert!(
        server.workflow_state(&session_id).is_none(),
        "released session is closed"
    );
}

fn ids_and_codes(frames: &[Value]) -> Vec<(Value, i64)> {
    frames
        .iter()
        .map(|f| (f["id"].clone(), f["error"]["code"].as_i64().unwrap_or(0)))
        .collect()
}

#[test]
fn session_new_without_id_is_rejected_before_any_side_effect() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let frames = serve(
        &mut server,
        "{\"jsonrpc\":\"2.0\",\"method\":\"session/new\",\"params\":{\"cwd\":\"/work\",\"mcpServers\":[]}}\n",
    );
    assert!(
        frames.is_empty(),
        "a request without an id is a notification; no frame: {frames:?}"
    );
    assert!(
        pool.calls().is_empty(),
        "no slot may be acquired: {:?}",
        pool.calls()
    );
    // The slot was never taken, so a request with an id still succeeds.
    let frames = serve(&mut server, &format!("{SESSION_NEW}\n"));
    assert!(
        frames[0]["result"]["sessionId"].is_string(),
        "frames: {frames:?}"
    );
}

#[test]
fn missing_or_wrong_jsonrpc_version_is_32600() {
    let mut server = AcpServer::new(MockPool::default(), config());
    let missing = serve(
        &mut server,
        "{\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":1}}\n",
    );
    assert_eq!(ids_and_codes(&missing), vec![(Value::Null, -32600)]);
    let wrong = serve(&mut server, "{\"jsonrpc\":\"1.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":1}}\n");
    assert_eq!(ids_and_codes(&wrong), vec![(Value::Null, -32600)]);
}

#[test]
fn null_id_request_gets_32600_with_null_id() {
    let mut server = AcpServer::new(MockPool::default(), config());
    let frames = serve(
        &mut server,
        "{\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"initialize\",\"params\":{\"protocolVersion\":1}}\n",
    );
    assert_eq!(ids_and_codes(&frames), vec![(Value::Null, -32600)]);
}

#[test]
fn cancel_sent_with_an_id_is_32601_with_that_id() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let opened = frames_of(&mut server, SESSION_NEW);
    let session_id = session_id_of(&opened);
    let cancel = format!(
        r#"{{"jsonrpc":"2.0","id":9,"method":"session/cancel","params":{{"sessionId":"{session_id}"}}}}"#
    );
    let frames = frames_of(&mut server, &cancel);
    assert_eq!(ids_and_codes(&frames), vec![(Value::from(9), -32601)]);
    assert_eq!(
        pool.calls(),
        vec!["acquire:slot-1".to_string()],
        "a rejected cancel must not touch the slot"
    );
}

#[test]
fn oversize_frame_is_32600_and_the_next_frame_is_served() {
    let mut server = AcpServer::new(MockPool::default(), config());
    let pad = "a".repeat(1024 * 1024 + 1);
    let long = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":1,\"pad\":\"{pad}\"}}}}"
    );
    let frames = serve(&mut server, &format!("{long}\n{INITIALIZE}\n"));
    assert_eq!(frames.len(), 2, "frames: {frames:?}");
    assert_eq!(ids_and_codes(&frames[..1]), vec![(Value::Null, -32600)]);
    assert_eq!(frames[1]["id"], 1);
    assert_eq!(frames[1]["result"]["protocolVersion"], 1);
}

#[test]
fn every_error_variant_maps_to_its_code() {
    use cas_acp::AcpError;
    let slot = SlotId::new("slot-1").unwrap();
    let cases: Vec<(AcpError, i32)> = vec![
        (AcpError::Parse("x".into()), -32700),
        (AcpError::InvalidRequest("x".into()), -32600),
        (AcpError::MethodNotFound("x".into()), -32601),
        (AcpError::InvalidParams("x".into()), -32602),
        (AcpError::Pool(PoolError::CapacityExhausted(2)), -32001),
        (
            AcpError::Pool(PoolError::LeaseExpired(slot.clone())),
            -32002,
        ),
        (
            AcpError::Pool(PoolError::NoActiveLease(slot.clone())),
            -32002,
        ),
        (
            AcpError::Pool(PoolError::TokenMismatch(slot.clone())),
            -32002,
        ),
        (AcpError::Pool(PoolError::SlotNotFound(slot)), -32603),
        (AcpError::Engine("x".into()), -32603),
    ];
    for (err, code) in cases {
        assert_eq!(err.code(), code, "variant: {err}");
    }
}

#[test]
fn session_new_with_missing_params_is_32602() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let frames = serve(
        &mut server,
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"session/new\",\"params\":{}}\n",
    );
    assert_eq!(ids_and_codes(&frames), vec![(Value::from(3), -32602)]);
    assert!(
        pool.calls().is_empty(),
        "bad params must not reach the pool"
    );
}

#[test]
fn second_session_on_one_slot_is_32001_without_a_pool_call() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let first = frames_of(&mut server, SESSION_NEW);
    assert!(
        first[0]["result"]["sessionId"].is_string(),
        "frames: {first:?}"
    );
    let second = frames_of(&mut server, SESSION_NEW_AGAIN);
    assert_eq!(ids_and_codes(&second), vec![(Value::from(3), -32001)]);
    assert_eq!(pool.calls(), vec!["acquire:slot-1".to_string()]);
}

#[test]
fn lease_expiry_boundary_releases_on_next_frame() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let first = session_id_of(&frames_of(&mut server, SESSION_NEW));

    set_now(1_599);
    let control = frames_of(&mut server, SESSION_NEW_AGAIN);
    assert_eq!(ids_and_codes(&control), vec![(Value::from(3), -32001)]);

    set_now(1_600);
    let second = frames_of(&mut server, SESSION_NEW_AGAIN);
    assert!(
        second[0]["result"]["sessionId"].is_string(),
        "frames: {second:?}"
    );
    let calls = pool.calls();
    assert_eq!(calls.len(), 4, "calls: {calls:?}");
    assert_eq!(calls[0], "acquire:slot-1");
    assert_eq!(calls[1], "suspend:slot-1");
    assert!(
        calls[2].starts_with("release:slot-1:token-"),
        "calls: {calls:?}"
    );
    assert_eq!(calls[3], "acquire:slot-1");
    assert_eq!(server.session_end(&first), Some(SessionEnd::Expired));
}

#[test]
fn tick_expires_without_a_frame() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let id = session_id_of(&frames_of(&mut server, SESSION_NEW));

    set_now(1_600);
    assert_eq!(server.tick(), vec![id.clone()]);
    let after_first = pool.calls();
    assert!(
        after_first.iter().any(|c| c.starts_with("release:slot-1:")),
        "calls: {after_first:?}"
    );

    assert!(server.tick().is_empty(), "a second tick ends nothing");
    assert_eq!(
        pool.calls(),
        after_first,
        "a second tick calls no pool method"
    );
    assert_eq!(server.session_end(&id), Some(SessionEnd::Expired));
}

#[test]
fn eof_releases_every_session() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), two_slot_config());
    let a = session_id_of(&frames_of(&mut server, SESSION_NEW));
    let b = session_id_of(&frames_of(&mut server, SESSION_NEW_AGAIN));

    serve(&mut server, "");

    let calls = pool.calls();
    for slot in ["slot-1", "slot-2"] {
        assert!(
            calls.contains(&format!("suspend:{slot}")),
            "calls: {calls:?}"
        );
        assert!(
            calls
                .iter()
                .any(|c| c.starts_with(&format!("release:{slot}:"))),
            "calls: {calls:?}"
        );
    }
    assert_eq!(server.session_end(&a), Some(SessionEnd::Disconnected));
    assert_eq!(server.session_end(&b), Some(SessionEnd::Disconnected));
}

#[test]
fn transport_error_releases_every_session() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let input = format!("{SESSION_NEW}\n");

    let result = server.serve(input.as_bytes(), FailingWriter);

    assert!(result.is_err(), "a write error must return Err");
    let calls = pool.calls();
    assert_eq!(calls.len(), 3, "calls: {calls:?}");
    assert_eq!(calls[0], "acquire:slot-1");
    assert_eq!(calls[1], "suspend:slot-1");
    assert!(calls[2].starts_with("release:slot-1:"), "calls: {calls:?}");
}

#[test]
fn eof_release_failure_still_ends_the_session() {
    let pool = MockPool {
        release_fails: true,
        ..MockPool::default()
    };
    let mut server = AcpServer::new(pool.clone(), two_slot_config());
    let a = session_id_of(&frames_of(&mut server, SESSION_NEW));
    let b = session_id_of(&frames_of(&mut server, SESSION_NEW_AGAIN));

    serve(&mut server, "");

    let releases = pool
        .calls()
        .iter()
        .filter(|c| c.starts_with("release:"))
        .count();
    assert_eq!(releases, 2, "calls: {:?}", pool.calls());
    assert_eq!(server.session_end(&a), Some(SessionEnd::Disconnected));
    assert_eq!(server.session_end(&b), Some(SessionEnd::Disconnected));
}

#[test]
fn expired_session_cancel_is_a_noop() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let id = session_id_of(&frames_of(&mut server, SESSION_NEW));

    set_now(1_600);
    let _ = frames_of(&mut server, INITIALIZE);
    assert_eq!(server.session_end(&id), Some(SessionEnd::Expired));
    let before = pool.calls();

    let frames = frames_of(&mut server, &cancel_line(&id));

    assert!(
        frames.is_empty(),
        "a cancel for an ended session sends no frame"
    );
    assert_eq!(pool.calls(), before, "the cancel makes no pool call");
    assert_eq!(server.session_end(&id), Some(SessionEnd::Expired));
}

#[test]
fn blank_frame_still_expires_due_sessions() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let id = session_id_of(&frames_of(&mut server, SESSION_NEW));

    set_now(1_600);
    let frames = serve(&mut server, "\n");

    assert!(frames.is_empty(), "a blank line gets no reply: {frames:?}");
    assert_eq!(
        server.session_end(&id),
        Some(SessionEnd::Expired),
        "the expiry check runs before the blank line is skipped"
    );
    let calls = pool.calls();
    assert!(
        calls.iter().any(|c| c.starts_with("release:slot-1:")),
        "calls: {calls:?}"
    );
}

/// Yields `data`, then every read fails, as a broken transport does.
struct ReadFailsAfter {
    data: &'static [u8],
    pos: usize,
}

impl std::io::Read for ReadFailsAfter {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let available = std::io::BufRead::fill_buf(self)?;
        let n = available.len().min(buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        std::io::BufRead::consume(self, n);
        Ok(n)
    }
}

impl std::io::BufRead for ReadFailsAfter {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.pos < self.data.len() {
            Ok(&self.data[self.pos..])
        } else {
            Err(std::io::Error::other("read failed"))
        }
    }

    fn consume(&mut self, amount: usize) {
        self.pos += amount;
    }
}

#[test]
fn read_error_releases_every_session() {
    let pool = MockPool::default();
    let mut server = AcpServer::new(pool.clone(), config());
    let input = ReadFailsAfter {
        data: format!("{SESSION_NEW}\n").leak().as_bytes(),
        pos: 0,
    };

    let result = server.serve(input, Vec::new());

    assert!(result.is_err(), "a read error must return Err");
    let calls = pool.calls();
    assert_eq!(calls.len(), 3, "calls: {calls:?}");
    assert_eq!(calls[0], "acquire:slot-1");
    assert_eq!(calls[1], "suspend:slot-1");
    assert!(calls[2].starts_with("release:slot-1:"), "calls: {calls:?}");
}

#[test]
fn handle_frame_rejects_an_oversize_line_before_parsing() {
    let mut server = AcpServer::new(MockPool::default(), config());
    let line = "a".repeat(cas_acp::server::MAX_FRAME_BYTES + 1);
    let frames = frames_of(&mut server, &line);
    assert_eq!(ids_and_codes(&frames), vec![(Value::Null, -32600)]);
}
