//! The ACP server: one JSON-RPC 2.0 object per line, over any reader and writer.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use agent_client_protocol::schema::v1::{
    CancelNotification, InitializeRequest, InitializeResponse, NewSessionRequest,
    NewSessionResponse, SessionId as AcpSessionId,
};
use agent_client_protocol::schema::ProtocolVersion;
use cas_domain::{Budget, Lease, LeaseId, LeaseToken, PrincipalRef, SlotId};
use cas_engine::{SessionId, SessionState, WorkflowSession};
use cas_pool::PoolError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AcpError;
use crate::pool_port::PoolPort;

const METHOD_INITIALIZE: &str = "initialize";
const METHOD_SESSION_NEW: &str = "session/new";
const METHOD_SESSION_CANCEL: &str = "session/cancel";

/// Settings the server needs to build leases.
pub struct AcpConfig {
    /// Slot ids this server may lease, in the order it tries them. The
    /// pool must already hold and warm each one.
    pub slots: Vec<SlotId>,
    /// Budget written into every lease.
    pub budget: Budget,
    /// Seconds from issue to expiry of a lease.
    pub lease_secs: u64,
    /// The principal that holds each lease.
    pub holder: PrincipalRef,
    /// Epoch seconds. Tests pass a fixed clock.
    pub clock: fn() -> u64,
}

/// What the server keeps for one open session.
struct ActiveSession {
    slot: SlotId,
    lease: Lease,
    workflow: WorkflowSession,
}

/// The ACP server. It owns the pool port and the open sessions.
pub struct AcpServer<P: PoolPort> {
    pool: P,
    config: AcpConfig,
    sessions: BTreeMap<String, ActiveSession>,
    next_seq: u64,
}

/// An incoming frame, before its method is dispatched.
#[derive(Deserialize)]
struct Envelope {
    method: String,
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Serialize)]
struct ResultFrame<'a> {
    jsonrpc: &'static str,
    id: &'a Value,
    result: Value,
}

#[derive(Serialize)]
struct ErrorFrame<'a> {
    jsonrpc: &'static str,
    id: &'a Value,
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: i64,
    message: String,
}

fn internal(err: impl std::fmt::Display) -> AcpError {
    AcpError::Engine(err.to_string())
}

fn parse_params<T: DeserializeOwned>(params: Option<Value>) -> Result<T, AcpError> {
    serde_json::from_value(params.unwrap_or(Value::Null))
        .map_err(|e| AcpError::InvalidParams(e.to_string()))
}

fn error_frame(id: &Value, err: &AcpError) -> Option<String> {
    serde_json::to_string(&ErrorFrame {
        jsonrpc: "2.0",
        id,
        error: ErrorBody {
            code: err.code(),
            message: err.to_string(),
        },
    })
    .ok()
}

fn result_frame(id: &Value, result: Value) -> Option<String> {
    serde_json::to_string(&ResultFrame {
        jsonrpc: "2.0",
        id,
        result,
    })
    .ok()
}

impl<P: PoolPort> AcpServer<P> {
    /// A server over `pool`, with no open sessions.
    pub fn new(pool: P, config: AcpConfig) -> Self {
        Self {
            pool,
            config,
            sessions: BTreeMap::new(),
            next_seq: 0,
        }
    }

    /// Read frames from `reader` until it ends. Write one response line per request.
    pub fn serve<R: BufRead, W: Write>(&mut self, reader: R, mut writer: W) -> std::io::Result<()> {
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(frame) = self.handle_line(&line) {
                writeln!(writer, "{frame}")?;
                writer.flush()?;
            }
        }
        Ok(())
    }

    /// Serve the process's standard input and output.
    pub fn serve_stdio(&mut self) -> std::io::Result<()> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        self.serve(stdin.lock(), stdout.lock())
    }

    /// Handle one line. Returns the response line, or `None` for a notification.
    fn handle_line(&mut self, line: &str) -> Option<String> {
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(e) => return error_frame(&Value::Null, &AcpError::Parse(e.to_string())),
        };
        let envelope: Envelope = match serde_json::from_value(value) {
            Ok(envelope) => envelope,
            Err(e) => {
                return error_frame(&Value::Null, &AcpError::InvalidRequest(e.to_string()));
            }
        };
        let outcome = self.dispatch(&envelope.method, envelope.params);
        // A frame without an id is a notification: it never gets a response.
        let id = envelope.id?;
        match outcome {
            Ok(Some(result)) => result_frame(&id, result),
            Ok(None) => None,
            Err(err) => error_frame(&id, &err),
        }
    }

    fn dispatch(&mut self, method: &str, params: Option<Value>) -> Result<Option<Value>, AcpError> {
        match method {
            METHOD_INITIALIZE => self.initialize(params).map(Some),
            METHOD_SESSION_NEW => self.session_new(params).map(Some),
            METHOD_SESSION_CANCEL => self.session_cancel(params).map(|()| None),
            other => Err(AcpError::MethodNotFound(other.to_string())),
        }
    }

    fn initialize(&mut self, params: Option<Value>) -> Result<Value, AcpError> {
        let _request: InitializeRequest = parse_params(params)?;
        // The server answers version 1 whatever version the client asks for.
        let response = InitializeResponse::new(ProtocolVersion::V1);
        serde_json::to_value(response).map_err(internal)
    }

    fn session_new(&mut self, params: Option<Value>) -> Result<Value, AcpError> {
        let _request: NewSessionRequest = parse_params(params)?;
        let slot = self.free_slot()?;
        let lease = self.new_lease(slot)?;
        let slot_id = self.pool.acquire(lease.clone())?;
        let key = format!("acp-session-{}", self.bump());
        let session_id = SessionId::new(key.clone()).map_err(internal)?;
        let mut workflow = WorkflowSession::new(session_id);
        let now = (self.config.clock)();
        if let Err(err) = workflow.start(slot_id.clone(), lease.id.clone(), now) {
            // The slot is held and no session will ever release it: give it back.
            let _ = self.pool.release(&slot_id, &lease.token);
            return Err(internal(err));
        }
        self.sessions.insert(
            key.clone(),
            ActiveSession {
                slot: slot_id,
                lease,
                workflow,
            },
        );
        serde_json::to_value(NewSessionResponse::new(AcpSessionId::new(key))).map_err(internal)
    }

    /// Freeze the session's slot, then release it. Sends no response.
    /// An unknown session is a no-op. The workflow has no cancel API yet (CER-2727).
    fn session_cancel(&mut self, params: Option<Value>) -> Result<(), AcpError> {
        let notice: CancelNotification = parse_params(params)?;
        let key = notice.session_id.0.to_string();
        let Some(active) = self.sessions.get(&key) else {
            return Ok(());
        };
        let slot = active.slot.clone();
        let token = active.lease.token.clone();
        // A failed freeze must not keep the slot leased: release anyway. The pool's
        // release thaws a frozen slot first. Errors go to stderr; stdout carries frames only.
        if let Err(err) = self.pool.suspend(&slot) {
            eprintln!("cas-acp: suspend {}: {err}", slot.as_str());
        }
        match self.pool.release(&slot, &token) {
            Ok(()) => {
                self.sessions.remove(&key);
            }
            Err(err) => eprintln!("cas-acp: release {}: {err}", slot.as_str()),
        }
        Ok(())
    }

    /// The engine state of an open session, or `None` if no such session is open.
    pub fn workflow_state(&self, session_id: &str) -> Option<SessionState> {
        self.sessions.get(session_id).map(|s| s.workflow.state())
    }

    /// The first configured slot that no open session holds.
    fn free_slot(&self) -> Result<SlotId, AcpError> {
        let full = AcpError::Pool(PoolError::CapacityExhausted(self.config.slots.len()));
        self.config
            .slots
            .iter()
            .find(|slot| !self.sessions.values().any(|s| &s.slot == *slot))
            .cloned()
            .ok_or(full)
    }

    fn new_lease(&mut self, slot: SlotId) -> Result<Lease, AcpError> {
        let n = self.bump();
        let now = (self.config.clock)();
        let id = LeaseId::new(format!("lease-{n}")).map_err(internal)?;
        let token = LeaseToken::new(format!("token-{n}")).map_err(internal)?;
        Lease::new(
            id,
            slot,
            self.config.holder.clone(),
            token,
            now,
            now + self.config.lease_secs,
            self.config.budget,
        )
        .map_err(internal)
    }

    fn bump(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }
}
