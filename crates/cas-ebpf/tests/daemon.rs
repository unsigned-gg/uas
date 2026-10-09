//! Integration tests for `cas-ebpfd` over a fake probe handle (no privileges).

use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use cas_ebpf::{
    client::DaemonClient,
    daemon::Daemon,
    handle::FakeProbeHandle,
    protocol::{StreamLine, MAX_LINE},
    EbpfError, ExecEvent,
};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A running daemon on a fresh socket. Dropping it aborts the server and removes the file.
struct Fixture {
    path: PathBuf,
    handle: Arc<Mutex<FakeProbeHandle>>,
    server: tokio::task::JoinHandle<Result<(), EbpfError>>,
}

impl Fixture {
    async fn start() -> Self {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("cas-ebpfd-test-{}-{n}.sock", std::process::id()));
        let handle = Arc::new(Mutex::new(FakeProbeHandle::new().expect("fake")));
        let daemon = Daemon::new(Arc::clone(&handle), path.clone());
        let server = tokio::spawn(daemon.serve());
        let fixture = Self {
            path,
            handle,
            server,
        };
        fixture.wait_for_socket().await;
        fixture
    }

    async fn wait_for_socket(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while UnixStream::connect(&self.path).is_err() {
            assert!(Instant::now() < deadline, "daemon never listened");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

fn raw_exchange(path: &Path, lines: &[&str]) -> Vec<String> {
    let mut stream = UnixStream::connect(path).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut replies = Vec::new();
    for line in lines {
        stream.write_all(line.as_bytes()).expect("write");
        stream.write_all(b"\n").expect("write");
        let mut reply = String::new();
        reader.read_line(&mut reply).expect("read");
        replies.push(reply.trim_end().to_string());
    }
    replies
}

#[tokio::test(flavor = "multi_thread")]
async fn track_then_untrack_changes_the_count() {
    let fx = Fixture::start().await;
    let path = fx.path.clone();
    tokio::task::spawn_blocking(move || {
        let mut client = DaemonClient::connect(&path).expect("connect");
        assert_eq!(client.status().expect("status"), (0, 0));
        let me = std::process::id();
        client.track(me, 1).expect("track");
        assert_eq!(client.status().expect("status").0, 1);
        client.untrack(me).expect("untrack");
        assert_eq!(client.status().expect("status").0, 0);
    })
    .await
    .expect("join");
    assert!(fx.handle.lock().expect("lock").tracked().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn track_reaches_the_handle() {
    let fx = Fixture::start().await;
    let path = fx.path.clone();
    tokio::task::spawn_blocking(move || {
        let mut client = DaemonClient::connect(&path).expect("connect");
        client.track(std::process::id(), 5).expect("track");
    })
    .await
    .expect("join");
    assert_eq!(
        fx.handle.lock().expect("lock").tracked(),
        vec![(std::process::id(), 5)]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn track_of_another_users_pid_is_refused() {
    if rustix::process::geteuid().is_root() {
        eprintln!("skipped: running as root, every pid is trackable");
        return;
    }
    let fx = Fixture::start().await;
    let path = fx.path.clone();
    let err = tokio::task::spawn_blocking(move || {
        let mut client = DaemonClient::connect(&path).expect("connect");
        // pid 1 belongs to root. The connection is still usable afterwards.
        let err = client.track(1, 0).expect_err("init is not ours");
        assert_eq!(client.status().expect("status"), (0, 0));
        err
    })
    .await
    .expect("join");
    assert!(
        matches!(&err, EbpfError::Protocol(reason) if reason.contains("belongs to uid 0")),
        "{err}"
    );
    assert!(fx.handle.lock().expect("lock").tracked().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn track_of_a_missing_pid_is_refused() {
    let fx = Fixture::start().await;
    let path = fx.path.clone();
    let err = tokio::task::spawn_blocking(move || {
        let mut client = DaemonClient::connect(&path).expect("connect");
        client.track(4_000_000, 1).expect_err("no such pid")
    })
    .await
    .expect("join");
    assert!(
        matches!(&err, EbpfError::Protocol(reason) if reason.starts_with("pid 4000000:")),
        "{err}"
    );
    assert!(fx.handle.lock().expect("lock").tracked().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_op_gets_an_error_and_the_connection_stays_usable() {
    let fx = Fixture::start().await;
    let path = fx.path.clone();
    let replies = tokio::task::spawn_blocking(move || {
        raw_exchange(
            &path,
            &[
                r#"{"op":"shell","cmd":"id"}"#,
                "not json",
                r#"{"op":"status"}"#,
            ],
        )
    })
    .await
    .expect("join");
    assert!(
        replies[0].starts_with(r#"{"ok":false,"error":""#),
        "{replies:?}"
    );
    assert!(
        replies[1].starts_with(r#"{"ok":false,"error":""#),
        "{replies:?}"
    );
    assert_eq!(replies[2], r#"{"ok":true,"tracked":0,"dropped_events":0}"#);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_line_over_the_limit_closes_the_connection() {
    let fx = Fixture::start().await;
    let path = fx.path.clone();
    let closed = tokio::task::spawn_blocking(move || {
        let mut stream = UnixStream::connect(&path).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        let mut line = vec![b'x'; MAX_LINE + 1];
        line.push(b'\n');
        stream.write_all(&line).expect("write");
        let mut buf = Vec::new();
        // EOF (Ok) or a reset (Err) both mean closed. A timeout would be an error
        // of kind WouldBlock or TimedOut, which means the server kept it open.
        match stream.read_to_end(&mut buf) {
            Ok(_) => buf.is_empty(),
            Err(e) => !matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ),
        }
    })
    .await
    .expect("join");
    assert!(closed, "daemon kept an oversized line open or replied");

    // A line of exactly MAX_LINE bytes is still read, and answered as an error frame.
    let path = fx.path.clone();
    let reply = tokio::task::spawn_blocking(move || {
        let line = "x".repeat(MAX_LINE);
        raw_exchange(&path, &[&line]).remove(0)
    })
    .await
    .expect("join");
    assert!(reply.starts_with(r#"{"ok":false"#), "{reply}");
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_delivers_an_event_even_when_reclamp_fails() {
    let fx = Fixture::start().await;
    let path = fx.path.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let reader = tokio::task::spawn_blocking(move || {
        let client = DaemonClient::connect(&path).expect("connect");
        let mut stream = client.stream().expect("stream");
        ready_tx.send(()).expect("ready");
        stream.next().expect("a line").expect("a frame")
    });
    tokio::task::spawn_blocking(move || ready_rx.recv().expect("subscribed"))
        .await
        .expect("join");
    // The pid does not exist, so reclamp_if_needed fails with ESRCH. The event must still arrive.
    let fake_pid = 4_000_000;
    fx.handle
        .lock()
        .expect("lock")
        .push(ExecEvent::new(
            9,
            fake_pid,
            1,
            1000,
            "rustc",
            "/usr/bin/rustc",
        ))
        .expect("push");
    let line = tokio::time::timeout(Duration::from_secs(5), reader)
        .await
        .expect("event arrived in time")
        .expect("join");
    match line {
        StreamLine::Event(record) => {
            assert_eq!(record.pid, fake_pid);
            assert_eq!(record.filename, "/usr/bin/rustc");
            assert_eq!(record.comm, "rustc");
        }
        other => panic!("expected an event, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn dropped_counter_growth_produces_a_dropped_line() {
    let fx = Fixture::start().await;
    let path = fx.path.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let reader = tokio::task::spawn_blocking(move || {
        let client = DaemonClient::connect(&path).expect("connect");
        let mut stream = client.stream().expect("stream");
        ready_tx.send(()).expect("ready");
        stream.next().expect("a line").expect("a frame")
    });
    tokio::task::spawn_blocking(move || ready_rx.recv().expect("subscribed"))
        .await
        .expect("join");
    fx.handle.lock().expect("lock").add_dropped(3);
    let line = tokio::time::timeout(Duration::from_secs(5), reader)
        .await
        .expect("dropped line arrived in time")
        .expect("join");
    assert_eq!(line, StreamLine::Dropped(3));
}

#[tokio::test(flavor = "multi_thread")]
async fn socket_mode_is_0660() {
    let fx = Fixture::start().await;
    let mode = std::fs::metadata(&fx.path)
        .expect("stat")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o660, "mode was {mode:o}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stale_socket_file_is_replaced() {
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("cas-ebpfd-test-{}-{n}.sock", std::process::id()));
    std::fs::write(&path, b"stale").expect("stale file");
    let handle = Arc::new(Mutex::new(FakeProbeHandle::new().expect("fake")));
    let server = tokio::spawn(Daemon::new(handle, path.clone()).serve());
    let probe = path.clone();
    let ok = tokio::task::spawn_blocking(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(mut c) = DaemonClient::connect(&probe) {
                return c.status().is_ok();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    })
    .await
    .expect("join");
    server.abort();
    let _ = std::fs::remove_file(&path);
    assert!(ok);
}

#[test]
fn connect_to_a_missing_socket_is_daemon_unavailable() {
    let path = std::env::temp_dir().join("cas-ebpfd-test-missing.sock");
    let err = DaemonClient::connect(&path).expect_err("no daemon");
    assert!(matches!(err, EbpfError::DaemonUnavailable { .. }), "{err}");
}
