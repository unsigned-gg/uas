//! ACP over stdio, against a mocked pool. Spec 0016 (A1), Tests section.

use std::cell::RefCell;
use std::rc::Rc;

use cas_acp::{AcpConfig, AcpServer, PoolPort};
use cas_domain::{Budget, Lease, LeaseToken, PrincipalRef, SlotId};
use cas_pool::PoolError;
use serde_json::Value;

/// Records every call. Acquire grants the slot the lease names.
#[derive(Clone, Default)]
struct MockPool {
    log: Rc<RefCell<Vec<String>>>,
    full: bool,
    suspend_fails: bool,
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
        Ok(())
    }
}

fn now() -> u64 {
    1_000
}

fn config() -> AcpConfig {
    AcpConfig {
        slots: vec![SlotId::new("slot-1").unwrap()],
        budget: Budget::new(10_000, 600).unwrap(),
        lease_secs: 600,
        holder: PrincipalRef::new("cas-acp").unwrap(),
        clock: now,
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

const INITIALIZE: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}"#;
const SESSION_NEW: &str =
    r#"{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":"/work","mcpServers":[]}}"#;

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
    let frames = serve(&mut server, &format!("{SESSION_NEW}\n"));
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["id"], 2);
    let session_id = frames[0]["result"]["sessionId"].as_str().unwrap();
    assert_eq!(pool.calls(), vec!["acquire:slot-1".to_string()]);
    assert_eq!(
        server.workflow_state(session_id),
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
    let opened = serve(&mut server, &format!("{SESSION_NEW}\n"));
    let session_id = opened[0]["result"]["sessionId"]
        .as_str()
        .unwrap()
        .to_string();

    let cancel = format!(
        r#"{{"jsonrpc":"2.0","method":"session/cancel","params":{{"sessionId":"{session_id}"}}}}"#
    );
    let frames = serve(&mut server, &format!("{cancel}\n"));

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
    let opened = serve(&mut server, &format!("{SESSION_NEW}\n"));
    let session_id = opened[0]["result"]["sessionId"]
        .as_str()
        .unwrap()
        .to_string();

    let cancel = format!(
        r#"{{"jsonrpc":"2.0","method":"session/cancel","params":{{"sessionId":"{session_id}"}}}}"#
    );
    let frames = serve(&mut server, &format!("{cancel}\n"));

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
    let opened = serve(&mut server, &format!("{SESSION_NEW}\n"));
    let session_id = opened[0]["result"]["sessionId"]
        .as_str()
        .unwrap()
        .to_string();
    let cancel = format!(
        r#"{{"jsonrpc":"2.0","id":9,"method":"session/cancel","params":{{"sessionId":"{session_id}"}}}}"#
    );
    let frames = serve(&mut server, &format!("{cancel}\n"));
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
    let first = serve(&mut server, &format!("{SESSION_NEW}\n"));
    assert!(
        first[0]["result"]["sessionId"].is_string(),
        "frames: {first:?}"
    );
    let second = serve(
        &mut server,
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"session/new\",\"params\":{\"cwd\":\"/work\",\"mcpServers\":[]}}\n",
    );
    assert_eq!(ids_and_codes(&second), vec![(Value::from(3), -32001)]);
    assert_eq!(pool.calls(), vec!["acquire:slot-1".to_string()]);
}
