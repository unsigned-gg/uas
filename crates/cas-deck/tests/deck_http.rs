//! HTTP tests for the deck. They run in-process on 127.0.0.1 with a raw HTTP/1.1 request.

use std::net::SocketAddr;
use std::sync::Arc;

use cas_deck::{
    bind, serve, CortexSettleSource, DeckError, DeckState, PendingCards, PendingFetch, SettleSource,
};
use cas_domain::{Attestation, EvidenceTier, LeaseId, PrincipalRef, SlotId};
use cas_engine::{
    Activity, ActivityKind, ActivityResult, EventStore, InMemoryEventStore, SessionId, SettleCard,
    SettleQueue, WorkflowEvent,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        let wanted = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(key, _)| *key == wanted)
            .map(|(_, value)| value.as_str())
    }
}

fn parse(raw: &str) -> Response {
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
    let mut lines = head.lines();
    let status = lines
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    Response {
        status,
        headers,
        body: body.to_string(),
    }
}

async fn get(state: DeckState, path: &str) -> Response {
    let listener = bind("127.0.0.1:0".parse::<SocketAddr>().unwrap())
        .await
        .expect("bind on an ephemeral port");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, state));

    tokio::time::timeout(std::time::Duration::from_secs(5), async move {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        parse(&String::from_utf8_lossy(&raw))
    })
    .await
    .expect("the deck answered within 5 s")
}

fn session_events() -> Vec<WorkflowEvent> {
    vec![
        WorkflowEvent::SessionStarted {
            sequence: 0,
            session_id: SessionId::new("sess-deck-1").unwrap(),
            slot_id: SlotId::new("slot-7").unwrap(),
            lease_id: LeaseId::new("lease-1").unwrap(),
            timestamp: 100,
        },
        WorkflowEvent::ActivityStarted {
            sequence: 1,
            activity: Activity::new("act-1", "Run make check", ActivityKind::Verify),
            timestamp: 110,
        },
        WorkflowEvent::ActivityCompleted {
            sequence: 2,
            result: ActivityResult {
                activity_id: "act-1".into(),
                success: true,
                attestation: Attestation {
                    signer: PrincipalRef::new("cas/builder-1").unwrap(),
                    evidence: EvidenceTier::Verified,
                    receipt: "make check: 0 failures".into(),
                    at: 120,
                },
                summary: None,
            },
            timestamp: 120,
        },
    ]
}

fn sessions() -> Arc<Vec<InMemoryEventStore>> {
    let mut store = InMemoryEventStore::new();
    for event in session_events() {
        store.append(event).unwrap();
    }
    Arc::new(vec![store])
}

fn queue_with_two_cards() -> Arc<SettleQueue> {
    let queue = SettleQueue::new();
    queue
        .enqueue(SettleCard::new(
            "card-1",
            "sess-deck-1",
            "step-1",
            "Push fix to development",
            "Make check passed",
            200,
        ))
        .unwrap();
    queue
        .enqueue(SettleCard::new(
            "card-2",
            "sess-deck-1",
            "step-2",
            "Rotate deploy key",
            "Scheduled rotation",
            201,
        ))
        .unwrap();
    queue.resolve("card-2", true).unwrap();
    Arc::new(queue)
}

struct FailingSettle;

impl SettleSource for FailingSettle {
    fn pending_cards(&self) -> Result<PendingCards, DeckError> {
        Err(DeckError::CortexUnavailable("connection refused".into()))
    }
}

/// Serves the committed Cortex fixture. No socket is opened.
struct FixtureFetch;

impl PendingFetch for FixtureFetch {
    fn fetch_pending(&self) -> Result<String, DeckError> {
        Ok(include_str!("fixtures/settle-pending.json").to_string())
    }
}

/// Serves the committed empty-queue fixture.
struct EmptyFetch;

impl PendingFetch for EmptyFetch {
    fn fetch_pending(&self) -> Result<String, DeckError> {
        Ok(include_str!("fixtures/settle-pending-empty.json").to_string())
    }
}

/// Fails the way a refused connection fails.
struct RefusedFetch;

impl PendingFetch for RefusedFetch {
    fn fetch_pending(&self) -> Result<String, DeckError> {
        Err(DeckError::CortexUnavailable("connection refused".into()))
    }
}

/// Answers with a body that is not the contract.
struct MalformedFetch;

impl PendingFetch for MalformedFetch {
    fn fetch_pending(&self) -> Result<String, DeckError> {
        Ok("{not json".to_string())
    }
}

fn cortex_state(fetch: impl PendingFetch + 'static) -> DeckState {
    DeckState::new(sessions(), Arc::new(CortexSettleSource::new(fetch)))
}

#[tokio::test]
async fn get_settle_lists_pending_only() {
    let state = DeckState::new(sessions(), queue_with_two_cards());
    let response = get(state, "/settle").await;
    assert_eq!(response.status, 200);
    assert!(
        response.body.contains("Push fix to development"),
        "{}",
        response.body
    );
    assert!(
        !response.body.contains("Rotate deploy key"),
        "{}",
        response.body
    );
}

#[tokio::test]
async fn get_settle_disconnected_when_source_fails() {
    let state = DeckState::new(sessions(), Arc::new(FailingSettle));
    let response = get(state, "/settle").await;
    assert_eq!(
        response.status, 200,
        "htmx swaps 2xx only, so the notice must be 2xx"
    );
    assert!(response.body.contains("disconnected"), "{}", response.body);
    assert!(
        !response.body.contains("connection refused"),
        "error text must not leak"
    );
    assert!(!response.body.contains("class=\"card"), "{}", response.body);
}

#[tokio::test]
async fn get_settle_renders_cortex_fixture() {
    let response = get(cortex_state(FixtureFetch), "/settle").await;
    assert_eq!(response.status, 200);
    let body = &response.body;
    assert!(body.contains(r#"id="settle/e1""#), "{body}");
    assert!(body.contains("Reply to the vendor invoice"), "{body}");
    assert!(body.contains("guard-deny"), "{body}");
    assert!(body.contains("step 2 of 5"), "{body}");
    assert!(!body.contains("cortex.invalid"), "{body}");
    assert!(!body.contains("http:"), "{body}");
    assert!(!body.contains("disconnected"), "{body}");
}

#[tokio::test]
async fn get_settle_disconnected_on_fetch_error() {
    let response = get(cortex_state(RefusedFetch), "/settle").await;
    assert_eq!(response.status, 200);
    assert!(response.body.contains("disconnected"), "{}", response.body);
    assert!(
        !response.body.contains("connection refused"),
        "error text must not leak"
    );
    assert!(!response.body.contains("class=\"card"), "{}", response.body);
}

#[tokio::test]
async fn get_settle_disconnected_on_parse_error() {
    let response = get(cortex_state(MalformedFetch), "/settle").await;
    assert_eq!(response.status, 200);
    assert!(response.body.contains("disconnected"), "{}", response.body);
    assert!(!response.body.contains("class=\"card"), "{}", response.body);
}

#[tokio::test]
async fn get_settle_empty_queue_is_not_disconnected() {
    let response = get(cortex_state(EmptyFetch), "/settle").await;
    assert_eq!(response.status, 200);
    assert!(
        response.body.contains("Nothing waits for a decision."),
        "{}",
        response.body
    );
    assert!(!response.body.contains("disconnected"), "{}", response.body);
}

#[tokio::test]
async fn get_root_renders_sessions_and_queue() {
    let state = DeckState::new(sessions(), queue_with_two_cards());
    let response = get(state, "/").await;
    assert_eq!(response.status, 200);
    let content_type = response.header("content-type").unwrap_or("");
    assert!(content_type.starts_with("text/html"), "{content_type}");
    assert!(
        response.body.contains("<!DOCTYPE html>"),
        "{}",
        response.body
    );
    assert!(response.body.contains("sess-deck-1"), "{}", response.body);
    assert!(response.body.contains("slot-7"), "{}", response.body);
    assert!(
        response.body.contains("class=\"badge badge-verified\""),
        "{}",
        response.body
    );
    assert!(
        response.body.contains("Push fix to development"),
        "{}",
        response.body
    );
}

#[tokio::test]
async fn bind_taken_port_is_server_bind_failed() {
    let taken = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = taken.local_addr().unwrap();
    match bind(addr).await {
        Err(DeckError::ServerBindFailed { addr: reported, .. }) => assert_eq!(reported, addr),
        other => panic!("expected ServerBindFailed, got {:?}", other.map(|_| ())),
    }
}
