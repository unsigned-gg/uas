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
    clamp::{nice_of, WORKER_NICE},
    client::DaemonClient,
    daemon::Daemon,
    handle::{FakeProbeHandle, ProbeHandle},
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
        Self::start_with(|daemon| daemon).await
    }

    async fn start_with(
        configure: impl FnOnce(Daemon<FakeProbeHandle>) -> Daemon<FakeProbeHandle>,
    ) -> Self {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("cas-ebpfd-test-{}-{n}.sock", std::process::id()));
        let handle = Arc::new(Mutex::new(FakeProbeHandle::new().expect("fake")));
        let daemon = configure(Daemon::new(Arc::clone(&handle), path.clone()));
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

/// A pid in `/proc` whose real uid is not ours, with that uid. `None` when every
/// visible process is ours (a container where pid 1 is the runner itself).
fn foreign_pid() -> Option<(u32, u32)> {
    let me = rustix::process::geteuid().as_raw();
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let pid: u32 = match entry.file_name().to_string_lossy().parse() {
            Ok(pid) => pid,
            Err(_) => continue,
        };
        let status = match std::fs::read_to_string(entry.path().join("status")) {
            Ok(status) => status,
            Err(_) => continue,
        };
        let uid = status
            .lines()
            .find_map(|line| line.strip_prefix("Uid:"))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|uid| uid.parse::<u32>().ok());
        if let Some(uid) = uid {
            if uid != me {
                return Some((pid, uid));
            }
        }
    }
    None
}

#[tokio::test(flavor = "multi_thread")]
async fn track_of_another_users_pid_is_refused() {
    if rustix::process::geteuid().is_root() {
        eprintln!("skipped: running as root, every pid is trackable");
        return;
    }
    let Some((pid, uid)) = foreign_pid() else {
        eprintln!("skipped: every visible process belongs to this uid");
        return;
    };
    let fx = Fixture::start().await;
    let path = fx.path.clone();
    let err = tokio::task::spawn_blocking(move || {
        let mut client = DaemonClient::connect(&path).expect("connect");
        // The connection is still usable after a refusal.
        let err = client.track(pid, 0).expect_err("not our pid");
        assert_eq!(client.status().expect("status"), (0, 0));
        err
    })
    .await
    .expect("join");
    let wanted = format!("belongs to uid {uid}");
    assert!(
        matches!(&err, EbpfError::Protocol(reason) if reason.contains(&wanted)),
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
    // The pid does not exist, so reclamp_if_needed fails with ESRCH. The event must still
    // arrive. Its uid is ours, or the stream filter would drop it.
    let fake_pid = 4_000_000;
    let me = rustix::process::geteuid().as_raw();
    fx.handle
        .lock()
        .expect("lock")
        .push(ExecEvent::new(
            9,
            fake_pid,
            1,
            me,
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

#[tokio::test(flavor = "multi_thread")]
async fn untrack_of_a_pid_this_peer_did_not_track_is_refused() {
    if rustix::process::geteuid().is_root() {
        eprintln!("skipped: running as root, every pid is untrackable");
        return;
    }
    let fx = Fixture::start().await;
    // The kernel side already tracks this pid, but not through this peer.
    fx.handle
        .lock()
        .expect("lock")
        .track(std::process::id(), 1)
        .expect("track");
    let path = fx.path.clone();
    let err = tokio::task::spawn_blocking(move || {
        let mut client = DaemonClient::connect(&path).expect("connect");
        client
            .untrack(std::process::id())
            .expect_err("not ours to untrack")
    })
    .await
    .expect("join");
    assert!(
        matches!(&err, EbpfError::Protocol(reason) if reason.contains("is not tracked by uid")),
        "{err}"
    );
    assert_eq!(
        fx.handle.lock().expect("lock").tracked().len(),
        1,
        "still tracked"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn track_quota_per_uid_is_enforced() {
    if rustix::process::geteuid().is_root() {
        eprintln!("skipped: running as root, quota does not apply");
        return;
    }
    let fx = Fixture::start_with(|daemon| daemon.with_track_quota(1)).await;
    let path = fx.path.clone();
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn sleep");
    let child_pid = child.id();
    let err = tokio::task::spawn_blocking(move || {
        let mut client = DaemonClient::connect(&path).expect("connect");
        client
            .track(std::process::id(), 1)
            .expect("first track fits the quota");
        // Tracking the same pid again is not a new entry.
        client
            .track(std::process::id(), 1)
            .expect("re-track of the same pid");
        let err = client
            .track(child_pid, std::process::id())
            .expect_err("over quota");
        client
            .untrack(std::process::id())
            .expect("untrack frees the slot");
        client
            .track(child_pid, std::process::id())
            .expect("fits again");
        err
    })
    .await
    .expect("join");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        matches!(&err, EbpfError::Protocol(reason) if reason.contains("track quota 1 reached")),
        "{err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_delivers_only_this_peers_events() {
    if rustix::process::geteuid().is_root() {
        eprintln!("skipped: running as root, the stream is unfiltered");
        return;
    }
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
    let me = rustix::process::geteuid().as_raw();
    {
        let mut handle = fx.handle.lock().expect("lock");
        handle
            .push(ExecEvent::new(
                1,
                4_000_001,
                1,
                me + 1,
                "other",
                "/usr/bin/other",
            ))
            .expect("push");
        handle
            .push(ExecEvent::new(2, 4_000_002, 1, me, "mine", "/usr/bin/mine"))
            .expect("push");
    }
    let line = tokio::time::timeout(Duration::from_secs(5), reader)
        .await
        .expect("event arrived in time")
        .expect("join");
    match line {
        StreamLine::Event(record) => {
            assert_eq!(record.uid, me);
            assert_eq!(
                record.filename, "/usr/bin/mine",
                "the other user's event was filtered"
            );
        }
        other => panic!("expected an event, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn socket_group_is_set_when_asked() {
    use std::os::unix::fs::MetadataExt;
    let gid = rustix::process::getegid().as_raw();
    let fx = Fixture::start_with(move |daemon| daemon.with_socket_gid(Some(gid))).await;
    let meta = std::fs::metadata(&fx.path).expect("stat");
    assert_eq!(meta.gid(), gid);
    assert_eq!(meta.permissions().mode() & 0o777, 0o660);
}

/// A `sleep 30` child with no stdio. The caller kills and reaps it.
fn spawn_sleeper() -> std::process::Child {
    std::process::Command::new("sleep")
        .arg("30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn sleep")
}

/// Connects, sends `stream` and reads the ack. The daemon subscribes before it
/// acks, so no event pushed after this returns is missed.
fn raw_stream(path: &Path) -> BufReader<UnixStream> {
    let mut stream = UnixStream::connect(path).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    stream.write_all(b"{\"op\":\"stream\"}\n").expect("write");
    let mut reader = BufReader::new(stream);
    let mut ack = String::new();
    reader.read_line(&mut ack).expect("ack");
    assert_eq!(ack.trim_end(), r#"{"ok":true}"#);
    reader
}

/// The nice value of every thread of this process, by tid.
fn own_thread_nices() -> std::collections::BTreeMap<u32, i32> {
    std::fs::read_dir("/proc/self/task")
        .expect("task dir")
        .flatten()
        .filter_map(|entry| entry.file_name().to_string_lossy().parse::<u32>().ok())
        .filter_map(|tid| nice_of(tid).ok().map(|nice| (tid, nice)))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn reclamp_follows_ns_pid_and_skips_an_unresolved_one() {
    let mut target = spawn_sleeper();
    let mut decoy = spawn_sleeper();
    let inherited = nice_of(target.id()).expect("nice of target");
    if inherited >= WORKER_NICE {
        let _ = target.kill();
        let _ = decoy.kill();
        let _ = target.wait();
        let _ = decoy.wait();
        eprintln!("skipped: this process already runs at nice {inherited}");
        return;
    }
    assert_eq!(nice_of(decoy.id()).expect("nice of decoy"), inherited);
    let fx = Fixture::start().await;
    let path = fx.path.clone();
    let reader = tokio::task::spawn_blocking(move || raw_stream(&path))
        .await
        .expect("join");
    let me = rustix::process::geteuid().as_raw();

    // The global pid names the decoy. Only the namespace pid names the target.
    let mut event = ExecEvent::new(1, decoy.id(), 1, me, "sleep", "/usr/bin/sleep");
    event.ns_pid = target.id();
    fx.handle.lock().expect("lock").push(event).expect("push");
    let (reader, _line) = tokio::task::spawn_blocking(move || next_line(reader))
        .await
        .expect("join");
    // The daemon re-clamps before it broadcasts, so the frame means it is done.
    assert_eq!(nice_of(target.id()).expect("nice"), WORKER_NICE, "target");
    assert_eq!(nice_of(decoy.id()).expect("nice"), inherited, "decoy");

    // ns_pid 0 is unresolved: the daemon must not touch its own threads.
    let before = own_thread_nices();
    let mut event = ExecEvent::new(2, decoy.id(), 1, me, "sleep", "/usr/bin/sleep");
    event.ns_pid = 0;
    fx.handle.lock().expect("lock").push(event).expect("push");
    let (_reader, line) = tokio::task::spawn_blocking(move || next_line(reader))
        .await
        .expect("join");
    assert!(line.contains(r#""ns_pid":0"#), "{line}");
    assert!(line.contains(&format!(r#""pid":{}"#, decoy.id())), "{line}");
    assert_eq!(own_thread_nices(), before, "the daemon renice itself");
    assert_eq!(nice_of(decoy.id()).expect("nice"), inherited, "decoy");

    let _ = target.kill();
    let _ = decoy.kill();
    let _ = target.wait();
    let _ = decoy.wait();
}

fn next_line(mut reader: BufReader<UnixStream>) -> (BufReader<UnixStream>, String) {
    let mut line = String::new();
    reader.read_line(&mut line).expect("a stream line");
    (reader, line)
}

const NAMESPACE_REFUSAL: &str =
    r#"{"ok":false,"error":"peer pid namespace differs from the daemon's"}"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_peer_in_another_pid_namespace_is_refused_on_its_first_request() {
    let mut child = spawn_sleeper();
    let child_pid = child.id();
    let me = std::process::id();
    let track = format!(r#"{{"op":"track","pid":{child_pid},"ppid":{me}}}"#);

    // The daemon believes its own namespace is (1, 1), so this peer differs.
    let refusing = Fixture::start_with(|daemon| daemon.with_pid_namespace([1, 1])).await;
    let path = refusing.path.clone();
    let (reply, closed) = tokio::task::spawn_blocking(move || {
        let mut stream = UnixStream::connect(&path).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        stream.write_all(track.as_bytes()).expect("write");
        stream.write_all(b"\n").expect("write");
        let mut reader = BufReader::new(stream);
        let mut reply = String::new();
        reader.read_line(&mut reply).expect("reply");
        let mut rest = String::new();
        let n = reader.read_line(&mut rest).unwrap_or(usize::MAX);
        (reply.trim_end().to_string(), n == 0)
    })
    .await
    .expect("join");
    assert_eq!(reply, NAMESPACE_REFUSAL);
    assert!(closed, "the daemon keeps a refused connection open");
    assert!(refusing.handle.lock().expect("lock").tracked().is_empty());

    // Control: the default daemon reads its own namespace, which this peer shares.
    let accepting = Fixture::start().await;
    let path = accepting.path.clone();
    tokio::task::spawn_blocking(move || {
        let mut client = DaemonClient::connect(&path).expect("connect");
        client
            .track(child_pid, me)
            .expect("same namespace is served");
    })
    .await
    .expect("join");
    assert_eq!(
        accepting.handle.lock().expect("lock").tracked(),
        vec![(child_pid, me)]
    );
    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test(flavor = "multi_thread")]
async fn pid_zero_cannot_be_tracked_or_untracked() {
    let fx = Fixture::start().await;
    let path = fx.path.clone();
    let replies = tokio::task::spawn_blocking(move || {
        raw_exchange(
            &path,
            &[
                r#"{"op":"track","pid":0,"ppid":1}"#,
                r#"{"op":"untrack","pid":0}"#,
                r#"{"op":"status"}"#,
            ],
        )
    })
    .await
    .expect("join");
    assert_eq!(
        replies[0],
        r#"{"ok":false,"error":"pid 0 cannot be tracked"}"#
    );
    assert_eq!(
        replies[1],
        r#"{"ok":false,"error":"pid 0 cannot be untracked"}"#
    );
    assert_eq!(replies[2], r#"{"ok":true,"tracked":0,"dropped_events":0}"#);
    assert!(fx.handle.lock().expect("lock").tracked().is_empty());
}
