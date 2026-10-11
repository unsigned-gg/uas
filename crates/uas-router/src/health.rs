// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Replica health monitor and probe (spec 0017 D5).
//!
//! [`HealthCounter`] is the pure trip and restore rule. [`probe`] is one TCP health check.
//! [`HealthMonitor::spawn`] runs the probes on an interval and flips the
//! [`AffinityTable`] when a counter flips.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::time::{interval, timeout, MissedTickBehavior};

use crate::affinity::{AffinityTable, ReplicaId};

/// Consecutive failures that mark a healthy replica unhealthy.
const TRIP_AFTER: u32 = 2;
/// Consecutive successes that mark an unhealthy replica healthy again.
const RESTORE_AFTER: u32 = 3;

/// Consecutive-result counter for one replica. New counters start healthy.
#[derive(Debug, Clone)]
pub struct HealthCounter {
    healthy: bool,
    failures: u32,
    successes: u32,
}

impl Default for HealthCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthCounter {
    /// A counter for a healthy replica.
    pub fn new() -> Self {
        Self {
            healthy: true,
            failures: 0,
            successes: 0,
        }
    }

    /// Record one probe result. Returns `Some(new_health)` when the state flips.
    pub fn record(&mut self, ok: bool) -> Option<bool> {
        if ok {
            self.failures = 0;
            self.successes = self.successes.saturating_add(1);
            if !self.healthy && self.successes >= RESTORE_AFTER {
                self.healthy = true;
                self.successes = 0;
                return Some(true);
            }
        } else {
            self.successes = 0;
            self.failures = self.failures.saturating_add(1);
            if self.healthy && self.failures >= TRIP_AFTER {
                self.healthy = false;
                self.failures = 0;
                return Some(false);
            }
        }
        None
    }
}

/// Probe timing.
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Time between probe rounds.
    pub interval: Duration,
    /// Bound on connect, write and read for one request.
    pub timeout: Duration,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_millis(5000),
            timeout: Duration::from_millis(1000),
        }
    }
}

/// Send `GET path` and return the status code, or `None` on any error or timeout.
async fn status_of(addr: SocketAddr, path: &str, limit: Duration) -> Option<u16> {
    let attempt = async {
        let mut stream = TcpStream::connect(addr).await.ok()?;
        let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.ok()?;
        let mut buf = [0u8; 256];
        let mut len = 0;
        // The status line is all we need: "HTTP/1.1 NNN ...".
        while len < buf.len() && !buf[..len].contains(&b'\n') {
            let n = stream.read(&mut buf[len..]).await.ok()?;
            if n == 0 {
                break;
            }
            len += n;
        }
        let line = std::str::from_utf8(&buf[..len]).ok()?.lines().next()?;
        let mut parts = line.split(' ');
        if !parts.next()?.starts_with("HTTP/") {
            return None;
        }
        parts.next()?.parse::<u16>().ok()
    };
    timeout(limit, attempt).await.ok().flatten()
}

/// One health check: `GET /healthz`, with one retry on `/health` after a `404`.
/// A `2xx` status is success. Anything else is failure.
pub async fn probe(addr: SocketAddr, cfg: &HealthConfig) -> bool {
    let mut status = status_of(addr, "/healthz", cfg.timeout).await;
    if status == Some(404) {
        status = status_of(addr, "/health", cfg.timeout).await;
    }
    matches!(status, Some(200..=299))
}

/// Handle to a running monitor task.
#[derive(Debug)]
pub struct HealthHandle {
    task: JoinHandle<()>,
}

impl HealthHandle {
    /// Abort the monitor task.
    pub fn stop(&self) {
        self.task.abort();
    }
}

/// Background prober that keeps an [`AffinityTable`] in step with replica health.
#[derive(Debug)]
pub struct HealthMonitor;

impl HealthMonitor {
    /// Spawn one task that probes every replica each `cfg.interval`. It calls
    /// `set_healthy` on the table when a replica's counter flips. The table lock is
    /// held only for that call. Must run inside a tokio runtime.
    pub fn spawn(
        table: Arc<Mutex<AffinityTable>>,
        replicas: Vec<(ReplicaId, SocketAddr)>,
        cfg: HealthConfig,
    ) -> HealthHandle {
        let task = tokio::spawn(async move {
            let mut counters = vec![HealthCounter::new(); replicas.len()];
            let mut ticker = interval(cfg.interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let probes: Vec<_> = replicas
                    .iter()
                    .map(|(_, addr)| {
                        let (addr, cfg) = (*addr, cfg.clone());
                        tokio::spawn(async move { probe(addr, &cfg).await })
                    })
                    .collect();
                for (((id, _), counter), handle) in
                    replicas.iter().zip(counters.iter_mut()).zip(probes)
                {
                    let ok = handle.await.unwrap_or(false);
                    if let Some(healthy) = counter.record(ok) {
                        table
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .set_healthy(id, healthy);
                    }
                }
            }
        });
        HealthHandle { task }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU16, Ordering};

    use tokio::net::TcpListener;

    use super::*;
    use crate::prefix::PrefixHash;

    fn cfg(timeout_ms: u64) -> HealthConfig {
        HealthConfig {
            interval: Duration::from_millis(20),
            timeout: Duration::from_millis(timeout_ms),
        }
    }

    /// Serve forever. `reply(path)` gives the status to send, or `None` to hold the
    /// connection open without answering.
    async fn serve<F>(reply: F) -> SocketAddr
    where
        F: Fn(&str) -> Option<u16> + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let reply = Arc::new(reply);
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let reply = Arc::clone(&reply);
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 512];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match sock.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let text = String::from_utf8_lossy(&buf).into_owned();
                    let path = text.split(' ').nth(1).unwrap_or("").to_owned();
                    match reply(&path) {
                        Some(code) => {
                            let resp = format!(
                                "HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            );
                            let _ = sock.write_all(resp.as_bytes()).await;
                        }
                        None => tokio::time::sleep(Duration::from_secs(30)).await,
                    }
                });
            }
        });
        addr
    }

    #[test]
    fn counter_sequences() {
        // (results, expected record() output per step)
        let cases: &[(&[bool], &[Option<bool>])] = &[
            (&[false], &[None]),
            (&[false, false], &[None, Some(false)]),
            (&[false, true, false], &[None, None, None]),
            (
                &[false, false, true, true, true],
                &[None, Some(false), None, None, Some(true)],
            ),
            (
                &[false, false, true, true, false, true, true, true],
                &[None, Some(false), None, None, None, None, None, Some(true)],
            ),
            (&[true, true, true], &[None, None, None]),
        ];
        for (input, want) in cases {
            let mut c = HealthCounter::new();
            let got: Vec<_> = input.iter().map(|&ok| c.record(ok)).collect();
            assert_eq!(&got, want, "input {input:?}");
        }
    }

    #[test]
    fn counter_s_s_f_s_s_s_stays_healthy() {
        let mut c = HealthCounter::new();
        let got: Vec<_> = [true, true, false, true, true, true]
            .iter()
            .map(|&ok| c.record(ok))
            .collect();
        assert!(got.iter().all(Option::is_none));
    }

    #[test]
    fn default_config_values() {
        let c = HealthConfig::default();
        assert_eq!(c.interval, Duration::from_millis(5000));
        assert_eq!(c.timeout, Duration::from_millis(1000));
    }

    #[tokio::test]
    async fn probe_true_on_200() {
        let addr = serve(|_| Some(200)).await;
        assert!(probe(addr, &cfg(1000)).await);
    }

    #[tokio::test]
    async fn probe_false_on_503() {
        let addr = serve(|_| Some(503)).await;
        assert!(!probe(addr, &cfg(1000)).await);
    }

    #[tokio::test]
    async fn probe_false_on_closed_port() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        drop(listener);
        assert!(!probe(addr, &cfg(1000)).await);
    }

    #[tokio::test]
    async fn probe_false_when_server_never_answers() {
        let addr = serve(|_| None).await;
        let started = std::time::Instant::now();
        assert!(!probe(addr, &cfg(100)).await);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn probe_retries_health_after_404() {
        let addr = serve(|p| Some(if p == "/healthz" { 404 } else { 200 })).await;
        assert!(probe(addr, &cfg(1000)).await);
    }

    #[tokio::test]
    async fn probe_false_when_both_paths_404() {
        let addr = serve(|_| Some(404)).await;
        assert!(!probe(addr, &cfg(1000)).await);
    }

    #[tokio::test]
    async fn monitor_flips_table_down_and_up() {
        let status = Arc::new(AtomicU16::new(200));
        let s = Arc::clone(&status);
        let addr = serve(move |_| Some(s.load(Ordering::SeqCst))).await;
        let id = ReplicaId::new("r1").expect("id");
        let mut t = AffinityTable::new();
        t.insert(id.clone());
        let table = Arc::new(Mutex::new(t));
        let hash = PrefixHash([7u8; 16]);
        let healthy = |table: &Arc<Mutex<AffinityTable>>| {
            !table
                .lock()
                .expect("lock")
                .choose(&hash)
                .expect("choice")
                .is_fallback()
        };
        let handle = HealthMonitor::spawn(Arc::clone(&table), vec![(id, addr)], cfg(200));

        status.store(503, Ordering::SeqCst);
        wait_until(|| !healthy(&table)).await;
        status.store(200, Ordering::SeqCst);
        wait_until(|| healthy(&table)).await;
        handle.stop();
    }

    /// Poll up to 5 s for `cond`.
    async fn wait_until(mut cond: impl FnMut() -> bool) {
        for _ in 0..250 {
            if cond() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("condition not reached within 5 s");
    }
}
