//! The ACP server: one JSON-RPC 2.0 object per line, over any reader and writer.

use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};

use agent_client_protocol::schema::v1::{
    CancelNotification, InitializeRequest, InitializeResponse, JsonRpcMessage, NewSessionRequest,
    NewSessionResponse, Notification, Request, RequestId, Response, SessionId as AcpSessionId,
    AGENT_METHOD_NAMES,
};
use agent_client_protocol::schema::ProtocolVersion;
use cas_domain::{Budget, Lease, LeaseId, LeaseToken, PrincipalRef, SlotId};
use cas_engine::{SessionId, SessionState, WorkflowSession};
use cas_pool::PoolError;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::error::AcpError;
use crate::pool_port::PoolPort;

const METHOD_INITIALIZE: &str = AGENT_METHOD_NAMES.initialize;
const METHOD_SESSION_NEW: &str = AGENT_METHOD_NAMES.session_new;
const METHOD_SESSION_CANCEL: &str = AGENT_METHOD_NAMES.session_cancel;

/// The longest frame the server reads. A longer line gets -32600 and is skipped.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

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

/// How an open session ended. Spec 0016 D6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    /// A `session/cancel` released the slot.
    Cancelled,
    /// The lease expired and the server released the slot.
    Expired,
    /// The input ended, or the transport failed, and the server released the slot.
    Disconnected,
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
    /// How each ended session ended. Kept for the server's life (spec 0016 N4).
    ends: BTreeMap<String, SessionEnd>,
    next_seq: u64,
}

/// One line read from the input.
enum Frame {
    /// A line of text, without its newline.
    Line(String),
    /// A line over `MAX_FRAME_BYTES`. The rest of it is skipped.
    TooLong,
    /// A line that is not UTF-8.
    Malformed,
    /// The input ended.
    End,
}

fn internal(err: impl std::fmt::Display) -> AcpError {
    AcpError::Engine(err.to_string())
}

fn parse_params<T: DeserializeOwned>(params: Option<Value>) -> Result<T, AcpError> {
    serde_json::from_value(params.unwrap_or(Value::Null))
        .map_err(|e| AcpError::InvalidParams(e.to_string()))
}

fn frame<T: Serialize>(message: JsonRpcMessage<T>) -> Option<String> {
    serde_json::to_string(&message).ok()
}

fn result_frame(id: RequestId, result: Value) -> Option<String> {
    frame(JsonRpcMessage::wrap(Response::<Value>::new(id, Ok(result))))
}

fn error_frame(id: RequestId, err: &AcpError) -> Option<String> {
    frame(JsonRpcMessage::wrap(Response::<Value>::new(
        id,
        Err(err.to_rpc()),
    )))
}

/// Read one line, capped at `MAX_FRAME_BYTES`.
fn read_frame<R: BufRead>(reader: &mut R) -> std::io::Result<Frame> {
    let mut buf = Vec::new();
    let read = reader
        .by_ref()
        .take(MAX_FRAME_BYTES as u64 + 1)
        .read_until(b'\n', &mut buf)?;
    if read == 0 {
        return Ok(Frame::End);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
    } else if buf.len() > MAX_FRAME_BYTES {
        skip_line(reader)?;
        return Ok(Frame::TooLong);
    }
    Ok(match String::from_utf8(buf) {
        Ok(text) => Frame::Line(text),
        Err(_) => Frame::Malformed,
    })
}

/// Discard input up to and including the next newline, without buffering it.
fn skip_line<R: BufRead>(reader: &mut R) -> std::io::Result<()> {
    loop {
        let (done, used) = {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                return Ok(());
            }
            match available.iter().position(|b| *b == b'\n') {
                Some(i) => (true, i + 1),
                None => (false, available.len()),
            }
        };
        reader.consume(used);
        if done {
            return Ok(());
        }
    }
}

impl<P: PoolPort> AcpServer<P> {
    /// A server over `pool`, with no open sessions.
    pub fn new(pool: P, config: AcpConfig) -> Self {
        Self {
            pool,
            config,
            sessions: BTreeMap::new(),
            ends: BTreeMap::new(),
            next_seq: 0,
        }
    }

    /// Read frames from `reader` until it ends. Write one response line per request.
    /// When the input ends, or a read or write fails, every live session ends as
    /// `Disconnected` (spec 0016 D6b). The error, if any, is then returned.
    pub fn serve<R: BufRead, W: Write>(
        &mut self,
        mut reader: R,
        mut writer: W,
    ) -> std::io::Result<()> {
        let result = self.serve_frames(&mut reader, &mut writer);
        self.end_all(SessionEnd::Disconnected);
        result
    }

    /// The read and write loop of `serve`. Returns at the end of input, or on the first
    /// transport error.
    fn serve_frames<R: BufRead, W: Write>(
        &mut self,
        reader: &mut R,
        writer: &mut W,
    ) -> std::io::Result<()> {
        loop {
            let frame = read_frame(reader)?;
            if matches!(frame, Frame::End) {
                return Ok(());
            }
            if let Some(line) = self.reply_to(frame) {
                writeln!(writer, "{line}")?;
                writer.flush()?;
            }
        }
    }

    /// The reply to one frame read from the input, after the expiry check (spec 0016 D6a).
    fn reply_to(&mut self, frame: Frame) -> Option<String> {
        self.expire_due();
        match frame {
            Frame::Line(text) => self.handle_text(&text),
            Frame::TooLong => error_frame(
                RequestId::Null,
                &AcpError::InvalidRequest(format!("frame exceeds {MAX_FRAME_BYTES} bytes")),
            ),
            Frame::Malformed => error_frame(
                RequestId::Null,
                &AcpError::Parse("frame is not UTF-8".into()),
            ),
            Frame::End => None,
        }
    }

    /// A line of text: a blank line gets no reply, anything else is handled.
    fn handle_text(&mut self, text: &str) -> Option<String> {
        if text.trim().is_empty() {
            None
        } else {
            self.handle_line(text)
        }
    }

    /// Serve the process's standard input and output.
    pub fn serve_stdio(&mut self) -> std::io::Result<()> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        self.serve(stdin.lock(), stdout.lock())
    }

    /// Handle one line. Returns the response line, or `None` when no response is due.
    fn handle_line(&mut self, text: &str) -> Option<String> {
        let value: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(e) => return error_frame(RequestId::Null, &AcpError::Parse(e.to_string())),
        };
        if value.get("jsonrpc") != Some(&Value::from("2.0")) {
            return error_frame(
                RequestId::Null,
                &AcpError::InvalidRequest("jsonrpc must be \"2.0\"".into()),
            );
        }
        let Some(raw_id) = value.get("id") else {
            return self.handle_notification(value);
        };
        let id: RequestId = match serde_json::from_value(raw_id.clone()) {
            Ok(RequestId::Null) => {
                return error_frame(
                    RequestId::Null,
                    &AcpError::InvalidRequest("id must not be null".into()),
                );
            }
            Ok(id) => id,
            Err(e) => {
                return error_frame(
                    RequestId::Null,
                    &AcpError::InvalidRequest(format!("id: {e}")),
                );
            }
        };
        let request: Request<Value> = match serde_json::from_value(value) {
            Ok(request) => request,
            Err(e) => return error_frame(id, &AcpError::InvalidRequest(e.to_string())),
        };
        let outcome = match request.method.as_ref() {
            METHOD_INITIALIZE => self.initialize(request.params),
            METHOD_SESSION_NEW => self.session_new(request.params),
            // The crate's decoder answers a cancel that carries an id with method-not-found.
            METHOD_SESSION_CANCEL => Err(AcpError::MethodNotFound(
                "session/cancel is a notification and must not carry an id".into(),
            )),
            other => Err(AcpError::MethodNotFound(other.to_string())),
        };
        match outcome {
            Ok(result) => result_frame(id, result),
            Err(err) => error_frame(id, &err),
        }
    }

    /// Handle a frame without an id. A notification never gets a response, except
    /// when it names a request method, which is then an invalid request.
    fn handle_notification(&mut self, value: Value) -> Option<String> {
        let notification: Notification<Value> = match serde_json::from_value(value) {
            Ok(notification) => notification,
            Err(e) => {
                return error_frame(RequestId::Null, &AcpError::InvalidRequest(e.to_string()))
            }
        };
        match notification.method.as_ref() {
            METHOD_SESSION_CANCEL => {
                if let Err(err) = self.session_cancel(notification.params) {
                    eprintln!("cas-acp: session/cancel: {err}");
                }
                None
            }
            method @ (METHOD_INITIALIZE | METHOD_SESSION_NEW) => {
                // JSON-RPC 2.0 section 4.1: a request without an id is a notification
                // and the server must not reply. Nothing is dispatched either.
                eprintln!("cas-acp: {method} without an id is a notification; ignored");
                None
            }
            _ => None,
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
        // Build the session id before acquiring, so an id error cannot leak the slot.
        let key = format!("acp-session-{}", self.bump());
        let session_id = SessionId::new(key.clone()).map_err(internal)?;
        let slot_id = self.pool.acquire(lease.clone())?;
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
                self.ends.insert(key, SessionEnd::Cancelled);
            }
            Err(err) => eprintln!("cas-acp: release {}: {err}", slot.as_str()),
        }
        Ok(())
    }

    /// Handle one frame at the current clock: the expiry check, then the frame. Returns
    /// the response line, if any. A caller that owns its loop uses this instead of `serve`.
    pub fn handle_frame(&mut self, line: &str) -> Option<String> {
        self.reply_to(Frame::Line(line.to_string()))
    }

    /// End every session whose lease is expired at the current clock. Returns the ended
    /// session ids, in ascending order. A caller that owns its loop calls this when no
    /// frame arrives (spec 0016 D6a).
    pub fn tick(&mut self) -> Vec<String> {
        self.expire_due()
    }

    /// How an ended session ended. `None` if the session is open or was never opened.
    pub fn session_end(&self, session_id: &str) -> Option<SessionEnd> {
        self.ends.get(session_id).copied()
    }

    /// The engine state of an open session, or `None` if no such session is open.
    pub fn workflow_state(&self, session_id: &str) -> Option<SessionState> {
        self.sessions.get(session_id).map(|s| s.workflow.state())
    }

    /// Read the clock once. End every session whose lease is expired at that time.
    /// Returns the ended ids, ascending.
    fn expire_due(&mut self) -> Vec<String> {
        let now = (self.config.clock)();
        let due: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, active)| active.lease.is_expired(now))
            .map(|(key, _)| key.clone())
            .collect();
        for key in &due {
            self.end_session(key, SessionEnd::Expired);
        }
        due
    }

    /// End every live session with `end`.
    fn end_all(&mut self, end: SessionEnd) {
        let live: Vec<String> = self.sessions.keys().cloned().collect();
        for key in &live {
            self.end_session(key, end);
        }
    }

    /// Suspend, then release, the slot of one session. The session ends whatever the
    /// result, so a retry on every frame cannot repeat the error (spec 0016 D6).
    /// Errors go to stderr; stdout carries frames only.
    fn end_session(&mut self, key: &str, end: SessionEnd) {
        let Some(active) = self.sessions.remove(key) else {
            return;
        };
        if let Err(err) = self.pool.suspend(&active.slot) {
            eprintln!("cas-acp: suspend {}: {err}", active.slot.as_str());
        }
        if let Err(err) = self.pool.release(&active.slot, &active.lease.token) {
            eprintln!("cas-acp: release {}: {err}", active.slot.as_str());
        }
        self.ends.insert(key.to_string(), end);
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
