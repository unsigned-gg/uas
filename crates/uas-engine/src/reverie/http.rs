// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Host-side HTTP client for the Reverie memory daemon (spec 0044 D1-D4, D6, D9, D10).
//!
//! Provides bounded HTTP/1.0 communication over raw `TcpStream`, fail-open degradation
//! on unreachable daemons, write-behind outbox durable buffering, and health probe tracking.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::outbox::{generate_event_id, OutboxEntry, ReverieOutbox};
use super::{ReverieClient, ReverieContextQuery, ReverieMemoryRecord};
use crate::activity::MemoryStatus;
use crate::EngineError;

/// Time provider trait for deterministic backoff and timeout testing.
pub trait Clock: Send + Sync {
    /// Return current instant.
    fn now(&self) -> Instant;
}

/// System clock implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Bounded deadlines for Reverie requests (spec 0044 D3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReverieDeadlines {
    /// Connect deadline (default 500 ms).
    pub connect_timeout: Duration,
    /// Response head deadline (default 2 s).
    pub head_timeout: Duration,
    /// Total exchange deadline (default 10 s).
    pub total_timeout: Duration,
    /// Dedicated health probe deadline (default 500 ms).
    pub health_probe_timeout: Duration,
}

impl Default for ReverieDeadlines {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_millis(500),
            head_timeout: Duration::from_secs(2),
            total_timeout: Duration::from_secs(10),
            health_probe_timeout: Duration::from_millis(500),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RequestTimeouts {
    connect: Duration,
    head: Duration,
    total: Duration,
}

/// Internal state tracking backoff, health probe transitions, and degradation flags.
struct ClientState {
    last_probe_success: Option<bool>,
    degraded_in_current_activity: bool,
    outbox_refused_in_current_activity: bool,
    backoff_duration: Duration,
    last_failure_at: Option<Instant>,
}

impl ClientState {
    fn new() -> Self {
        Self {
            last_probe_success: None,
            degraded_in_current_activity: false,
            outbox_refused_in_current_activity: false,
            backoff_duration: Duration::from_secs(1),
            last_failure_at: None,
        }
    }
}

/// Production host-side HTTP Reverie client (spec 0044).
pub struct HttpReverieClient {
    base_url: String,
    deadlines: ReverieDeadlines,
    outbox: Arc<ReverieOutbox>,
    state: Mutex<ClientState>,
    clock: Arc<dyn Clock>,
}

impl HttpReverieClient {
    /// Create a new HTTP client pointed at `base_url` backed by `outbox`.
    pub fn new(base_url: impl Into<String>, outbox: Arc<ReverieOutbox>) -> Self {
        Self::with_deadlines_and_clock(
            base_url,
            outbox,
            ReverieDeadlines::default(),
            Arc::new(SystemClock),
        )
    }

    /// Create a client with custom deadlines and clock (for testing).
    pub fn with_deadlines_and_clock(
        base_url: impl Into<String>,
        outbox: Arc<ReverieOutbox>,
        deadlines: ReverieDeadlines,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let mut base_url = base_url.into();
        if base_url.ends_with('/') {
            base_url.pop();
        }
        Self {
            base_url,
            deadlines,
            outbox,
            state: Mutex::new(ClientState::new()),
            clock,
        }
    }

    /// Return reference to deadlines.
    #[must_use]
    pub fn deadlines(&self) -> &ReverieDeadlines {
        &self.deadlines
    }

    /// Return reference to outbox.
    #[must_use]
    pub fn outbox(&self) -> &ReverieOutbox {
        &self.outbox
    }

    /// Reset per-activity status flags.
    pub fn begin_activity(&self) {
        let mut state = self.state.lock().unwrap();
        state.degraded_in_current_activity = false;
        state.outbox_refused_in_current_activity = false;
    }

    /// Calculate the current memory status by the D5 precedence (spec 0044 D5).
    ///
    /// Precedence:
    /// 1. `OutboxRefused` if outbox directory refused or append failed.
    /// 2. `Degraded` if probe or any reverie call failed.
    /// 3. `Outbox(n)` if pending records > 0.
    /// 4. `Live`.
    pub fn memory_status(&self) -> MemoryStatus {
        let state = self.state.lock().unwrap();
        if self.outbox.is_refused() || state.outbox_refused_in_current_activity {
            return MemoryStatus::OutboxRefused;
        }
        if state.degraded_in_current_activity {
            return MemoryStatus::Degraded;
        }
        let pending = self.outbox.pending_count();
        if pending > 0 {
            return MemoryStatus::Outbox(pending);
        }
        MemoryStatus::Live
    }

    /// Resolve host:port from base URL.
    fn resolve_target(&self) -> Result<(String, SocketAddr), EngineError> {
        let stripped = self
            .base_url
            .strip_prefix("http://")
            .ok_or_else(|| EngineError::Reverie("only http scheme supported".to_string()))?;
        let host_port = stripped.split('/').next().unwrap_or(stripped);
        let mut addrs = host_port.to_socket_addrs().map_err(EngineError::Io)?;
        let addr = addrs
            .next()
            .ok_or_else(|| EngineError::Reverie("cannot resolve host".to_string()))?;
        Ok((host_port.to_string(), addr))
    }

    /// Probe `/health` endpoint with `health_probe_timeout` (spec 0044 D3, D4).
    pub fn probe_health(&self) -> bool {
        let (host, addr) = match self.resolve_target() {
            Ok(t) => t,
            Err(_) => {
                self.record_probe_result(false);
                return false;
            }
        };

        let passed = self
            .send_http_request(
                &host,
                addr,
                "GET",
                "/health",
                None,
                RequestTimeouts {
                    connect: self.deadlines.health_probe_timeout,
                    head: self.deadlines.health_probe_timeout,
                    total: self.deadlines.health_probe_timeout,
                },
            )
            .is_ok();

        self.record_probe_result(passed);
        passed
    }

    /// Record probe result and trigger drain if flipped from down to live (spec 0044 D6).
    fn record_probe_result(&self, passed: bool) {
        let should_drain = {
            let mut state = self.state.lock().unwrap();
            let flipped = match state.last_probe_success {
                None => passed,        // First probe of process passed
                Some(false) => passed, // Flipped from down to live
                Some(true) => false,
            };
            state.last_probe_success = Some(passed);
            if !passed {
                state.degraded_in_current_activity = true;
            }
            flipped
        };

        if should_drain && !self.is_in_backoff() {
            let _ = self.drain_outbox();
        }
    }

    /// Check if backoff window is currently active (spec 0044 D6).
    fn is_in_backoff(&self) -> bool {
        let state = self.state.lock().unwrap();
        if let Some(last_fail) = state.last_failure_at {
            let elapsed = self.clock.now().duration_since(last_fail);
            if elapsed < state.backoff_duration {
                return true;
            }
        }
        false
    }

    /// Record a send failure, triggering exponential backoff (spec 0044 D6).
    fn record_send_failure(&self) {
        let mut state = self.state.lock().unwrap();
        state.last_failure_at = Some(self.clock.now());
        // Double backoff up to 30 s ceiling
        let next = state.backoff_duration * 2;
        state.backoff_duration = next.min(Duration::from_secs(30));
    }

    /// Reset backoff on successful send (spec 0044 D6).
    fn record_send_success(&self) {
        let mut state = self.state.lock().unwrap();
        state.backoff_duration = Duration::from_secs(1);
        state.last_failure_at = None;
    }

    /// Perform a single HTTP/1.0 request with strict deadlines and remaining-budget timeouts (spec 0044 D2, D3).
    fn send_http_request(
        &self,
        host: &str,
        addr: SocketAddr,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        timeouts: RequestTimeouts,
    ) -> Result<(u16, Vec<u8>), EngineError> {
        let start = self.clock.now();

        // 1. Connect
        let mut stream =
            TcpStream::connect_timeout(&addr, timeouts.connect).map_err(EngineError::Io)?;

        // 2. Prepare HTTP/1.0 request
        let mut req = Vec::new();
        req.extend_from_slice(format!("{method} {path} HTTP/1.0\r\n").as_bytes());
        req.extend_from_slice(format!("Host: {host}\r\n").as_bytes());
        req.extend_from_slice(b"Connection: close\r\n");
        if let Some(payload) = body {
            req.extend_from_slice(b"Content-Type: application/json\r\n");
            req.extend_from_slice(format!("Content-Length: {}\r\n", payload.len()).as_bytes());
            req.extend_from_slice(b"\r\n");
            req.extend_from_slice(payload);
        } else {
            req.extend_from_slice(b"\r\n");
        }

        stream.write_all(&req).map_err(EngineError::Io)?;
        stream.flush().map_err(EngineError::Io)?;

        // 3. Read response head with remaining head budget
        let elapsed = self.clock.now().duration_since(start);
        if elapsed >= timeouts.head || elapsed >= timeouts.total {
            return Err(EngineError::Reverie("request timeout".to_string()));
        }

        let remaining_head = timeouts.head - elapsed;
        let remaining_total = timeouts.total - elapsed;
        let read_head_timeout = remaining_head.min(remaining_total);
        stream
            .set_read_timeout(Some(read_head_timeout))
            .map_err(EngineError::Io)?;

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader
            .read_line(&mut status_line)
            .map_err(EngineError::Io)?;

        let parts: Vec<&str> = status_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(EngineError::Reverie("invalid HTTP response".to_string()));
        }
        let status_code: u16 = parts[1]
            .parse()
            .map_err(|_| EngineError::Reverie("invalid status code".to_string()))?;

        // Read headers
        let mut content_length: Option<usize> = None;
        let mut line = String::new();
        loop {
            let elapsed = self.clock.now().duration_since(start);
            if elapsed >= timeouts.head || elapsed >= timeouts.total {
                return Err(EngineError::Reverie("request head timeout".to_string()));
            }
            let read_timeout = (timeouts.head - elapsed).min(timeouts.total - elapsed);
            reader
                .get_ref()
                .set_read_timeout(Some(read_timeout))
                .map_err(EngineError::Io)?;

            line.clear();
            let n = reader.read_line(&mut line).map_err(EngineError::Io)?;
            if n == 0 || line == "\r\n" || line == "\n" {
                break;
            }
            let lower = line.to_lowercase();
            if let Some(val) = lower.strip_prefix("content-length:") {
                if let Ok(len) = val.trim().parse::<usize>() {
                    content_length = Some(len);
                }
            }
        }

        // 4. Read response body bound by remaining total budget
        let mut resp_body = Vec::new();
        if let Some(len) = content_length {
            let mut read_bytes = 0;
            let mut buf = [0u8; 4096];
            while read_bytes < len {
                let elapsed = self.clock.now().duration_since(start);
                if elapsed >= timeouts.total {
                    return Err(EngineError::Reverie("request body timeout".to_string()));
                }
                let remaining = timeouts.total - elapsed;
                reader
                    .get_ref()
                    .set_read_timeout(Some(remaining))
                    .map_err(EngineError::Io)?;

                let to_read = (len - read_bytes).min(buf.len());
                let n = reader.read(&mut buf[..to_read]).map_err(EngineError::Io)?;
                if n == 0 {
                    break;
                }
                resp_body.extend_from_slice(&buf[..n]);
                read_bytes += n;
            }
        } else {
            // Read until EOF
            loop {
                let elapsed = self.clock.now().duration_since(start);
                if elapsed >= timeouts.total {
                    return Err(EngineError::Reverie("request body timeout".to_string()));
                }
                let remaining = timeouts.total - elapsed;
                reader
                    .get_ref()
                    .set_read_timeout(Some(remaining))
                    .map_err(EngineError::Io)?;

                let mut buf = [0u8; 4096];
                let n = match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                    Err(e) => return Err(EngineError::Io(e)),
                };
                resp_body.extend_from_slice(&buf[..n]);
            }
        }

        if !(200..300).contains(&status_code) {
            return Err(EngineError::Reverie(format!("HTTP error {status_code}")));
        }

        Ok((status_code, resp_body))
    }

    /// Send a single memory observation over HTTP (spec 0044 D9).
    pub fn send_observation(&self, entry: &OutboxEntry) -> Result<(), EngineError> {
        let (host, addr) = self.resolve_target()?;
        let body_json =
            serde_json::to_vec(&entry.record).map_err(|e| EngineError::Reverie(e.to_string()))?;

        // Enrich with event_id
        let mut map: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&body_json).map_err(|e| EngineError::Reverie(e.to_string()))?;
        map.insert(
            "event_id".to_string(),
            serde_json::Value::String(entry.event_id.clone()),
        );
        let enriched = serde_json::to_vec(&map).map_err(|e| EngineError::Reverie(e.to_string()))?;

        let res = self.send_http_request(
            &host,
            addr,
            "POST",
            "/observations",
            Some(&enriched),
            RequestTimeouts {
                connect: self.deadlines.connect_timeout,
                head: self.deadlines.head_timeout,
                total: self.deadlines.total_timeout,
            },
        );

        match res {
            Ok(_) => {
                self.record_send_success();
                Ok(())
            }
            Err(e) => {
                self.record_send_failure();
                Err(e)
            }
        }
    }

    /// Drain pending records from outbox, stopping at first failure (spec 0044 D6).
    pub fn drain_outbox(&self) -> Result<usize, EngineError> {
        self.outbox.drain(|entry| self.send_observation(entry))
    }
}

impl ReverieClient for HttpReverieClient {
    fn record(&self, record: ReverieMemoryRecord) -> Result<(), EngineError> {
        let event_id = generate_event_id(&record.session_id);
        let entry = OutboxEntry { event_id, record };

        // If outbox directory is refused, attempt direct send; on failure mark refused and return Ok
        if self.outbox.is_refused() {
            if self.send_observation(&entry).is_err() {
                let mut state = self.state.lock().unwrap();
                state.outbox_refused_in_current_activity = true;
            }
            return Ok(());
        }

        // If outbox has pending records or backoff is active, append directly to maintain FIFO (spec 0044 D6)
        let pending = self.outbox.pending_count();
        if pending > 0 || self.is_in_backoff() {
            if !self.outbox.append(&entry) {
                let mut state = self.state.lock().unwrap();
                state.outbox_refused_in_current_activity = true;
            } else if !self.is_in_backoff() {
                // Drain trigger: append behind pending records (spec 0044 D6)
                let _ = self.drain_outbox();
            }
            return Ok(());
        }

        // Try direct send first
        match self.send_observation(&entry) {
            Ok(()) => {
                // Drain trigger: record send acknowledged (spec 0044 D6)
                let _ = self.drain_outbox();
                Ok(())
            }
            Err(_) => {
                // Buffer to outbox upon delivery failure (spec 0044 D4, D6)
                if !self.outbox.append(&entry) {
                    let mut state = self.state.lock().unwrap();
                    state.outbox_refused_in_current_activity = true;
                }
                Ok(())
            }
        }
    }

    fn search(&self, query: &ReverieContextQuery) -> Result<Vec<ReverieMemoryRecord>, EngineError> {
        // Probe /health first (spec 0044 D4)
        if !self.probe_health() {
            let mut state = self.state.lock().unwrap();
            state.degraded_in_current_activity = true;
            return Ok(Vec::new());
        }

        let (host, addr) = match self.resolve_target() {
            Ok(t) => t,
            Err(_) => {
                let mut state = self.state.lock().unwrap();
                state.degraded_in_current_activity = true;
                return Ok(Vec::new());
            }
        };

        let path = format!(
            "/search?q={}&limit={}{}",
            query.query,
            query.limit,
            query
                .project
                .as_ref()
                .map(|p| format!("&project={p}"))
                .unwrap_or_default()
        );

        let res = self.send_http_request(
            &host,
            addr,
            "GET",
            &path,
            None,
            RequestTimeouts {
                connect: self.deadlines.connect_timeout,
                head: self.deadlines.head_timeout,
                total: self.deadlines.total_timeout,
            },
        );

        match res {
            Ok((_, body)) => {
                // Drain trigger: search answered 2xx (spec 0044 D6)
                if !self.is_in_backoff() {
                    let _ = self.drain_outbox();
                }

                // Filter items that deserialize as ReverieMemoryRecord
                if let Ok(records) = serde_json::from_slice::<Vec<ReverieMemoryRecord>>(&body) {
                    Ok(records)
                } else if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&body) {
                    if let Some(items) = val.get("results").and_then(|r| r.as_array()) {
                        let parsed: Vec<ReverieMemoryRecord> = items
                            .iter()
                            .filter_map(|i| serde_json::from_value(i.clone()).ok())
                            .collect();
                        Ok(parsed)
                    } else {
                        Ok(Vec::new())
                    }
                } else {
                    Ok(Vec::new())
                }
            }
            Err(_) => {
                // Fail-open: search failure marks degraded and returns empty context (spec 0044 D4)
                let mut state = self.state.lock().unwrap();
                state.degraded_in_current_activity = true;
                Ok(Vec::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::thread;
    use uas_domain::{Attestation, EvidenceTier, PrincipalRef};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "uas-http-test-{}-{}-{}",
                name,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct MockClock {
        current: Mutex<Instant>,
    }

    impl MockClock {
        fn new(start: Instant) -> Self {
            Self {
                current: Mutex::new(start),
            }
        }

        fn advance(&self, duration: Duration) {
            *self.current.lock().unwrap() += duration;
        }
    }

    impl Clock for MockClock {
        fn now(&self) -> Instant {
            *self.current.lock().unwrap()
        }
    }

    fn dummy_record(session: &str, topic: &str) -> ReverieMemoryRecord {
        ReverieMemoryRecord {
            project: "uas".into(),
            session_id: session.into(),
            topic_key: topic.into(),
            summary: format!("summary for {session}"),
            attestation: Attestation {
                signer: PrincipalRef::new("uas/test-signer").unwrap(),
                evidence: EvidenceTier::Verified,
                receipt: "test-receipt".into(),
                at: 1000,
            },
        }
    }

    struct TestHttpServer {
        addr: SocketAddr,
        shutdown: Arc<AtomicBool>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl TestHttpServer {
        fn spawn<F>(handler: F) -> Self
        where
            F: Fn(&str, &str, Option<&[u8]>) -> (u16, Vec<u8>, Option<Duration>, bool)
                + Send
                + Sync
                + 'static,
        {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let shutdown = Arc::new(AtomicBool::new(false));
            let shutdown_clone = Arc::clone(&shutdown);
            let handler = Arc::new(handler);

            let handle = thread::spawn(move || {
                while !shutdown_clone.load(Ordering::SeqCst) {
                    let (stream, _) = match listener.accept() {
                        Ok(s) => s,
                        Err(_) => break,
                    };
                    if shutdown_clone.load(Ordering::SeqCst) {
                        break;
                    }
                    let handler = Arc::clone(&handler);
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                    let mut reader = BufReader::new(stream);
                    let mut req_line = String::new();
                    if reader.read_line(&mut req_line).is_err() || req_line.is_empty() {
                        continue;
                    }
                    let parts: Vec<&str> = req_line.split_whitespace().collect();
                    if parts.len() < 2 {
                        continue;
                    }
                    let method = parts[0].to_string();
                    let path = parts[1].to_string();

                    let mut content_length = 0;
                    let mut header_line = String::new();
                    loop {
                        header_line.clear();
                        if reader.read_line(&mut header_line).is_err()
                            || header_line == "\r\n"
                            || header_line == "\n"
                            || header_line.is_empty()
                        {
                            break;
                        }
                        if let Some(val) =
                            header_line.to_lowercase().strip_prefix("content-length:")
                        {
                            if let Ok(len) = val.trim().parse::<usize>() {
                                content_length = len;
                            }
                        }
                    }

                    let mut body = vec![0u8; content_length];
                    if content_length > 0 {
                        let _ = reader.read_exact(&mut body);
                    }

                    let (status, resp_body, stall, trickle) = handler(
                        &method,
                        &path,
                        if content_length > 0 {
                            Some(&body)
                        } else {
                            None
                        },
                    );

                    if let Some(dur) = stall {
                        thread::sleep(dur);
                    }

                    let mut stream = reader.into_inner();
                    let head = format!(
                        "HTTP/1.0 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        resp_body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());

                    if trickle && !resp_body.is_empty() {
                        let _ = stream.write_all(&resp_body[..1]);
                        let _ = stream.flush();
                        thread::sleep(Duration::from_millis(200));
                        let _ = stream.write_all(&resp_body[1..]);
                    } else {
                        let _ = stream.write_all(&resp_body);
                    }
                    let _ = stream.flush();
                }
            });

            Self {
                addr,
                shutdown,
                handle: Some(handle),
            }
        }
    }

    impl Drop for TestHttpServer {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::SeqCst);
            let _ = TcpStream::connect(self.addr);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    #[test]
    fn test_deadlines_default_matches_spec_0032_d4() {
        let deadlines = ReverieDeadlines::default();
        assert_eq!(deadlines.connect_timeout, Duration::from_millis(500));
        assert_eq!(deadlines.head_timeout, Duration::from_secs(2));
        assert_eq!(deadlines.total_timeout, Duration::from_secs(10));
        assert_eq!(deadlines.health_probe_timeout, Duration::from_millis(500));
    }

    #[test]
    fn test_head_timeout_expires_and_fails_open() {
        let server = TestHttpServer::spawn(|method, path, _| match (method, path) {
            ("GET", "/health") => (200, b"OK".to_vec(), None, false),
            ("GET", p) if p.starts_with("/search") => {
                (200, b"[]".to_vec(), Some(Duration::from_millis(200)), false)
            }
            _ => (404, b"Not Found".to_vec(), None, false),
        });

        let temp = TestDir::new("head-timeout");
        let outbox = Arc::new(ReverieOutbox::open(temp.path().join("outbox")));
        let deadlines = ReverieDeadlines {
            connect_timeout: Duration::from_millis(500),
            head_timeout: Duration::from_millis(50),
            total_timeout: Duration::from_millis(500),
            health_probe_timeout: Duration::from_millis(500),
        };
        let client = HttpReverieClient::with_deadlines_and_clock(
            format!("http://{}", server.addr),
            outbox,
            deadlines,
            Arc::new(SystemClock),
        );

        let query = ReverieContextQuery {
            query: "test".into(),
            project: Some("uas".into()),
            limit: 5,
        };

        let start = Instant::now();
        let res = client.search(&query).unwrap();
        let elapsed = start.elapsed();

        assert!(res.is_empty());
        assert!(elapsed < Duration::from_millis(300));
        assert_eq!(client.memory_status(), MemoryStatus::Degraded);
    }

    #[test]
    fn test_total_timeout_trickle_byte_bounded() {
        let server = TestHttpServer::spawn(|method, path, _| match (method, path) {
            ("GET", "/health") => (200, b"OK".to_vec(), None, false),
            ("GET", p) if p.starts_with("/search") => {
                (200, b"[{\"some\":\"data\"}]".to_vec(), None, true)
            }
            _ => (404, b"Not Found".to_vec(), None, false),
        });

        let temp = TestDir::new("trickle");
        let outbox = Arc::new(ReverieOutbox::open(temp.path().join("outbox")));
        let deadlines = ReverieDeadlines {
            connect_timeout: Duration::from_millis(500),
            head_timeout: Duration::from_millis(200),
            total_timeout: Duration::from_millis(60),
            health_probe_timeout: Duration::from_millis(500),
        };
        let client = HttpReverieClient::with_deadlines_and_clock(
            format!("http://{}", server.addr),
            outbox,
            deadlines,
            Arc::new(SystemClock),
        );

        let query = ReverieContextQuery {
            query: "test".into(),
            project: Some("uas".into()),
            limit: 5,
        };

        let start = Instant::now();
        let res = client.search(&query).unwrap();
        let elapsed = start.elapsed();

        assert!(res.is_empty());
        assert!(elapsed < Duration::from_millis(300));
        assert_eq!(client.memory_status(), MemoryStatus::Degraded);
    }

    #[test]
    fn test_fail_open_on_closed_port_and_error_500() {
        let temp = TestDir::new("fail-open");
        let outbox = Arc::new(ReverieOutbox::open(temp.path().join("outbox")));

        // 1. Closed port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_addr = listener.local_addr().unwrap();
        drop(listener);

        let client = HttpReverieClient::new(format!("http://{closed_addr}"), Arc::clone(&outbox));
        let query = ReverieContextQuery {
            query: "fail".into(),
            project: None,
            limit: 10,
        };
        let res = client.search(&query).unwrap();
        assert!(res.is_empty());
        assert_eq!(client.memory_status(), MemoryStatus::Degraded);

        // 2. Health probe returns 500
        let server_health_500 =
            TestHttpServer::spawn(|_, _, _| (500, b"error".to_vec(), None, false));
        let client_health_500 = HttpReverieClient::new(
            format!("http://{}", server_health_500.addr),
            Arc::clone(&outbox),
        );
        let res2 = client_health_500.search(&query).unwrap();
        assert!(res2.is_empty());
        assert_eq!(client_health_500.memory_status(), MemoryStatus::Degraded);

        // 3. Health 200, search returns 500
        let server_search_500 = TestHttpServer::spawn(|method, path, _| match (method, path) {
            ("GET", "/health") => (200, b"OK".to_vec(), None, false),
            _ => (500, b"internal error".to_vec(), None, false),
        });
        let client_search_500 = HttpReverieClient::new(
            format!("http://{}", server_search_500.addr),
            Arc::clone(&outbox),
        );
        let res3 = client_search_500.search(&query).unwrap();
        assert!(res3.is_empty());
        assert_eq!(client_search_500.memory_status(), MemoryStatus::Degraded);
    }

    #[test]
    fn test_observation_delivery_failure_buffers_to_outbox() {
        let post_counter = Arc::new(AtomicUsize::new(0));
        let post_counter_clone = Arc::clone(&post_counter);

        let server = TestHttpServer::spawn(move |method, path, _| {
            if method == "POST" && path == "/observations" {
                post_counter_clone.fetch_add(1, Ordering::SeqCst);
                (500, b"failed to store".to_vec(), None, false)
            } else {
                (200, b"OK".to_vec(), None, false)
            }
        });

        let temp = TestDir::new("buffer-outbox");
        let outbox = Arc::new(ReverieOutbox::open(temp.path().join("outbox")));
        let client = HttpReverieClient::new(format!("http://{}", server.addr), Arc::clone(&outbox));

        let record = dummy_record("sess-1", "topic-1");
        let res = client.record(record);
        assert!(res.is_ok());
        assert_eq!(post_counter.load(Ordering::SeqCst), 1);
        assert_eq!(outbox.pending_count(), 1);

        // Verify outbox.jsonl has 1 record
        let lines: Vec<String> = fs::read_to_string(outbox.outbox_path())
            .unwrap()
            .lines()
            .map(|l| l.to_string())
            .collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("sess-1"));
    }

    #[test]
    fn test_pending_records_fifo_order_and_drain_triggers() {
        let received_events = Arc::new(Mutex::new(Vec::<String>::new()));
        let received_events_clone = Arc::clone(&received_events);

        let server = TestHttpServer::spawn(move |method, path, body| match (method, path) {
            ("GET", "/health") => (200, b"OK".to_vec(), None, false),
            ("GET", p) if p.starts_with("/search") => (200, b"[]".to_vec(), None, false),
            ("POST", "/observations") => {
                if let Some(b) = body {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(b) {
                        if let Some(id) = v.get("event_id").and_then(|i| i.as_str()) {
                            received_events_clone.lock().unwrap().push(id.to_string());
                        }
                    }
                }
                (200, b"{\"status\":\"stored\"}".to_vec(), None, false)
            }
            _ => (404, b"Not Found".to_vec(), None, false),
        });

        let temp = TestDir::new("fifo-drain");
        let outbox = Arc::new(ReverieOutbox::open(temp.path().join("outbox")));

        // Pre-fill outbox with r1 and r2
        let entry1 = OutboxEntry {
            event_id: "ev-r1".into(),
            record: dummy_record("sess-r1", "topic"),
        };
        let entry2 = OutboxEntry {
            event_id: "ev-r2".into(),
            record: dummy_record("sess-r2", "topic"),
        };
        assert!(outbox.append(&entry1));
        assert!(outbox.append(&entry2));
        assert_eq!(outbox.pending_count(), 2);

        let client = HttpReverieClient::new(format!("http://{}", server.addr), Arc::clone(&outbox));

        // Trigger 1: record of r3 appends behind pending and drains all in FIFO order (r1, r2, r3)
        let r3 = dummy_record("sess-r3", "topic");
        assert!(client.record(r3).is_ok());

        assert_eq!(outbox.pending_count(), 0);
        let events = received_events.lock().unwrap().clone();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], "ev-r1");
        assert_eq!(events[1], "ev-r2");
        assert!(events[2].starts_with("uas:sess-r3:"));

        // Trigger 2: search answered 200 drains pending records
        assert!(outbox.append(&OutboxEntry {
            event_id: "ev-r4".into(),
            record: dummy_record("sess-r4", "topic"),
        }));
        assert_eq!(outbox.pending_count(), 1);

        let query = ReverieContextQuery {
            query: "test".into(),
            project: None,
            limit: 5,
        };
        let _ = client.search(&query).unwrap();
        assert_eq!(outbox.pending_count(), 0);
        let events2 = received_events.lock().unwrap().clone();
        assert_eq!(events2.len(), 4);
        assert_eq!(events2[3], "ev-r4");

        // Trigger 3: probe flipping from failed to live drains pending records
        assert!(outbox.append(&OutboxEntry {
            event_id: "ev-r5".into(),
            record: dummy_record("sess-r5", "topic"),
        }));
        assert_eq!(outbox.pending_count(), 1);

        // Mark previous probe failed
        {
            let mut st = client.state.lock().unwrap();
            st.last_probe_success = Some(false);
        }
        // Probe now passes against server
        assert!(client.probe_health());
        assert_eq!(outbox.pending_count(), 0);
        let events3 = received_events.lock().unwrap().clone();
        assert_eq!(events3.len(), 5);
        assert_eq!(events3[4], "ev-r5");
    }

    #[test]
    fn test_backoff_window_suppresses_post() {
        let post_counter = Arc::new(AtomicUsize::new(0));
        let post_counter_clone = Arc::clone(&post_counter);

        let server = TestHttpServer::spawn(move |method, path, _| {
            if method == "POST" && path == "/observations" {
                post_counter_clone.fetch_add(1, Ordering::SeqCst);
                (500, b"error".to_vec(), None, false)
            } else {
                (200, b"OK".to_vec(), None, false)
            }
        });

        let temp = TestDir::new("backoff");
        let outbox = Arc::new(ReverieOutbox::open(temp.path().join("outbox")));
        let clock = Arc::new(MockClock::new(Instant::now()));

        let client = HttpReverieClient::with_deadlines_and_clock(
            format!("http://{}", server.addr),
            Arc::clone(&outbox),
            ReverieDeadlines::default(),
            Arc::clone(&clock) as Arc<dyn Clock>,
        );

        // 1. Initial record fails, triggering backoff
        assert!(client.record(dummy_record("s1", "t")).is_ok());
        assert_eq!(post_counter.load(Ordering::SeqCst), 1);
        assert_eq!(outbox.pending_count(), 1);

        // 2. Advance clock by 0.5s (< 1s backoff duration)
        clock.advance(Duration::from_millis(500));

        // 3. Second record within backoff window does NOT send POST
        assert!(client.record(dummy_record("s2", "t")).is_ok());
        assert_eq!(post_counter.load(Ordering::SeqCst), 1); // No new POST sent!
        assert_eq!(outbox.pending_count(), 2);

        // 4. Advance clock by 1.5s (> 1s backoff duration)
        clock.advance(Duration::from_millis(1500));
        // Backoff has expired
        assert!(!client.is_in_backoff());
    }

    #[test]
    fn test_memory_status_precedence() {
        let temp = TestDir::new("precedence");

        // Case 1: OutboxRefused (symlink outbox dir) with closed port -> OutboxRefused
        let symlink_dir = temp.path().join("symlink-target");
        fs::create_dir_all(&symlink_dir).unwrap();
        let symlink_path = temp.path().join("symlink-link");
        std::os::unix::fs::symlink(&symlink_dir, &symlink_path).unwrap();

        let refused_outbox = Arc::new(ReverieOutbox::open(symlink_path));
        assert!(refused_outbox.is_refused());
        let client_refused = HttpReverieClient::new("http://127.0.0.1:1", refused_outbox);
        assert_eq!(client_refused.memory_status(), MemoryStatus::OutboxRefused);

        // Case 2: Degraded (valid outbox dir, probe failed)
        let valid_outbox = Arc::new(ReverieOutbox::open(temp.path().join("valid-outbox")));
        assert!(!valid_outbox.is_refused());
        let client_degraded =
            HttpReverieClient::new("http://127.0.0.1:1", Arc::clone(&valid_outbox));
        assert!(!client_degraded.probe_health());
        assert_eq!(client_degraded.memory_status(), MemoryStatus::Degraded);

        // Case 3: Outbox(n) (valid outbox, probe healthy, 2 pending records)
        let server = TestHttpServer::spawn(|method, path, _| match (method, path) {
            ("GET", "/health") => (200, b"OK".to_vec(), None, false),
            _ => (200, b"{}".to_vec(), None, false),
        });
        let client_pending =
            HttpReverieClient::new(format!("http://{}", server.addr), Arc::clone(&valid_outbox));
        assert!(valid_outbox.append(&OutboxEntry {
            event_id: "e1".into(),
            record: dummy_record("s1", "t"),
        }));
        assert!(valid_outbox.append(&OutboxEntry {
            event_id: "e2".into(),
            record: dummy_record("s2", "t"),
        }));
        assert_eq!(client_pending.memory_status(), MemoryStatus::Outbox(2));

        // Case 4: Live (probe healthy, 0 pending)
        let empty_outbox = Arc::new(ReverieOutbox::open(temp.path().join("empty-outbox")));
        let client_live = HttpReverieClient::new(format!("http://{}", server.addr), empty_outbox);
        assert_eq!(client_live.memory_status(), MemoryStatus::Live);
    }

    #[test]
    fn test_event_id_uniqueness_and_reuse() {
        let id1 = generate_event_id("session-a");
        let id2 = generate_event_id("session-a");
        assert_ne!(id1, id2);
        assert!(id1.starts_with("uas:session-a:"));
        assert!(id2.starts_with("uas:session-a:"));

        // Two records with same session get distinct event_ids
        let id_rec1 = generate_event_id("sess");
        let id_rec2 = generate_event_id("sess");
        assert_ne!(id_rec1, id_rec2);
    }
}
