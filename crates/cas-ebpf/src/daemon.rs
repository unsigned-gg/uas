//! The `cas-ebpfd` server (spec 0014 D8, ruling R3).
//!
//! One reader task watches the probe handle's event descriptor. It drains the
//! ring buffer, re-clamps every tracked pid that executed, and fans each record
//! out on a broadcast channel. One task per connection answers requests. The
//! handle sits behind a std mutex that is held for a map call only, never
//! across an await.
//!
//! Boundary rules: the daemon runs as root and any member of the socket group
//! may connect, so every request is checked against the peer's uid
//! (`SO_PEERCRED`). `track` is accepted only for a pid whose real uid equals
//! the peer's, and only up to a per-uid quota. `untrack` is accepted only for
//! a pid that peer tracked. `stream` delivers only events whose uid is the
//! peer's. A root peer is exempt from all three. The re-clamp after an exec
//! skips a process whose effective uid differs from its real uid (a setuid
//! binary), so the daemon never renices a privileged process.

use std::{
    collections::BTreeMap,
    io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{unix::OwnedWriteHalf, UnixListener, UnixStream},
    sync::broadcast::{self, error::RecvError},
};

use crate::{
    clamp,
    error::EbpfError,
    handle::ProbeHandle,
    protocol::{parse_request, Request, Response, StreamLine, MAX_LINE},
    wire::ExecEvent,
};

/// Capacity of the broadcast channel. A slow stream client lags past this.
const STREAM_BACKLOG: usize = 1024;
/// How often the reader polls when no event arrives (dropped counter check).
const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Socket mode: owner and group may connect (spec 0014 Deployment).
const SOCKET_MODE: u32 = 0o660;
/// Default number of pids one non-root uid may track at once. `TRACKED_PIDS`
/// has 1024 entries shared by every client and every thread clone.
pub const DEFAULT_TRACK_QUOTA: usize = 256;

struct Shared<H> {
    handle: Arc<Mutex<H>>,
    /// Pids tracked through this daemon, with the uid that tracked each. The
    /// handle has no count call, so `status` prunes this map against the
    /// kernel map (exit removes entries).
    tracked: Mutex<BTreeMap<u32, u32>>,
    events: broadcast::Sender<StreamLine>,
    track_quota: usize,
}

/// The daemon: a probe handle served on a Unix socket.
pub struct Daemon<H: ProbeHandle + Send + 'static> {
    handle: Arc<Mutex<H>>,
    path: PathBuf,
    socket_gid: Option<u32>,
    track_quota: usize,
}

impl<H: ProbeHandle + Send + 'static> Daemon<H> {
    /// A daemon over `handle` that will listen at `path`.
    pub fn new(handle: Arc<Mutex<H>>, path: impl Into<PathBuf>) -> Self {
        Self {
            handle,
            path: path.into(),
            socket_gid: None,
            track_quota: DEFAULT_TRACK_QUOTA,
        }
    }

    /// Gives the socket file this group id after bind, so group members can connect.
    pub fn with_socket_gid(mut self, gid: Option<u32>) -> Self {
        self.socket_gid = gid;
        self
    }

    /// Caps the pids one non-root uid may track at once.
    pub fn with_track_quota(mut self, quota: usize) -> Self {
        self.track_quota = quota;
        self
    }

    /// Binds the socket and serves until an error ends the reader or the listener.
    pub async fn serve(self) -> Result<(), EbpfError> {
        remove_stale(&self.path)?;
        // Bind under a restrictive umask, so the socket never exists with a wider
        // mode than SOCKET_MODE, then set the exact mode and group.
        let previous = rustix::process::umask(rustix::fs::Mode::from_bits_truncate(0o117));
        let bound = UnixListener::bind(&self.path);
        rustix::process::umask(previous);
        let listener = bound?;
        if let Some(gid) = self.socket_gid {
            rustix::fs::chown(&self.path, None, Some(rustix::fs::Gid::from_raw(gid)))
                .map_err(io::Error::from)?;
        }
        std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(SOCKET_MODE))?;
        tracing::info!(socket = %self.path.display(), gid = ?self.socket_gid, "cas-ebpfd listening");

        let (events, _) = broadcast::channel(STREAM_BACKLOG);
        let shared = Arc::new(Shared {
            handle: self.handle,
            tracked: Mutex::new(BTreeMap::new()),
            events,
            track_quota: self.track_quota,
        });
        let mut reader = tokio::spawn(reader_task(Arc::clone(&shared)));
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    tokio::spawn(connection(Arc::clone(&shared), stream));
                }
                done = &mut reader => {
                    return match done {
                        Ok(result) => result,
                        Err(e) => Err(EbpfError::Io(io::Error::other(e))),
                    };
                }
            }
        }
    }
}

/// Builds a multi-thread runtime and blocks on [`Daemon::serve`]. For the binary.
pub fn run_with<H: ProbeHandle + Send + 'static>(
    handle: H,
    path: impl AsRef<Path>,
    socket_gid: Option<u32>,
) -> Result<(), EbpfError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let daemon =
        Daemon::new(Arc::new(Mutex::new(handle)), path.as_ref()).with_socket_gid(socket_gid);
    runtime.block_on(daemon.serve())
}

/// The gid of group `name` from `/etc/group`.
pub fn resolve_group(name: &str) -> io::Result<u32> {
    let groups = std::fs::read_to_string("/etc/group")?;
    groups
        .lines()
        .find_map(|line| parse_group_line(line, name))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("group {name} not in /etc/group"),
            )
        })
}

/// `name:x:gid:members` -> gid when the name matches.
fn parse_group_line(line: &str, name: &str) -> Option<u32> {
    let mut fields = line.split(':');
    if fields.next()? != name {
        return None;
    }
    fields.nth(1)?.parse().ok()
}

fn remove_stale(path: &Path) -> Result<(), EbpfError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Locks, treating a poisoned mutex as usable: the maps stay consistent.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

async fn reader_task<H: ProbeHandle + Send + 'static>(
    shared: Arc<Shared<H>>,
) -> Result<(), EbpfError> {
    let fd = lock(&shared.handle).event_fd().try_clone_to_owned()?;
    let afd = tokio::io::unix::AsyncFd::new(fd)?;
    let mut tick = tokio::time::interval(POLL_INTERVAL);
    let mut last_dropped = 0u64;
    loop {
        tokio::select! {
            ready = afd.readable() => {
                let mut guard = ready?;
                drain(&shared, &mut last_dropped);
                guard.clear_ready();
            }
            _ = tick.tick() => drain(&shared, &mut last_dropped),
        }
    }
}

/// Reads every queued record, re-clamps, broadcasts, then checks the dropped counter.
fn drain<H: ProbeHandle>(shared: &Shared<H>, last_dropped: &mut u64) {
    let (events, dropped) = {
        let mut handle = lock(&shared.handle);
        let mut events: Vec<ExecEvent> = Vec::new();
        while let Some(event) = handle.next_event() {
            events.push(event);
        }
        (events, handle.dropped_events())
    };
    for event in events {
        // A setuid binary runs with an effective uid that differs from the real
        // one. Never renice such a process, even if a tracked worker exec'd it.
        // /proc and setpriority take pids of this process's namespace.
        let pid = event.ns_pid;
        match pid_uids(pid) {
            Ok((real, effective)) if real == effective => match clamp::reclamp_if_needed(pid) {
                Ok(true) => {
                    tracing::info!(pid, "re-clamped a tracked pid after exec");
                }
                Ok(false) => {}
                Err(e) => tracing::debug!(pid, error = %e, "re-clamp skipped"),
            },
            Ok((real, effective)) => tracing::warn!(
                pid,
                real,
                effective,
                "re-clamp skipped: effective uid differs from real uid"
            ),
            Err(e) => tracing::debug!(pid, error = %e, "re-clamp skipped"),
        }
        // No receivers is normal: nobody is streaming.
        let _ = shared.events.send(StreamLine::Event(event.record()));
    }
    match dropped {
        Ok(total) if total > *last_dropped => {
            *last_dropped = total;
            let _ = shared.events.send(StreamLine::Dropped(total));
        }
        Ok(_) => {}
        Err(e) => tracing::debug!(error = %e, "dropped counter unreadable"),
    }
}

async fn connection<H: ProbeHandle + Send + 'static>(shared: Arc<Shared<H>>, stream: UnixStream) {
    let peer = match stream.peer_cred() {
        Ok(cred) => Peer { uid: cred.uid() },
        Err(e) => {
            tracing::warn!(error = %e, "refusing a connection without peer credentials");
            return;
        }
    };
    if let Err(e) = serve_connection(&shared, stream, peer).await {
        tracing::debug!(error = %e, "connection ended");
    }
}

/// The credentials of a connected client (`SO_PEERCRED`).
#[derive(Clone, Copy, Debug)]
struct Peer {
    uid: u32,
}

impl Peer {
    /// `Ok(())` when this peer may track `pid`: root may track anything, anyone
    /// else only a pid whose real uid is their own.
    fn may_track(self, pid: u32) -> Result<(), String> {
        if self.uid == 0 {
            return Ok(());
        }
        match pid_owner(pid) {
            Ok(owner) if owner == self.uid => Ok(()),
            Ok(owner) => Err(format!(
                "pid {pid} belongs to uid {owner}, not to uid {}",
                self.uid
            )),
            Err(e) => Err(format!("pid {pid}: {e}")),
        }
    }
}

/// The real uid of `pid`, from the `Uid:` line of `/proc/<pid>/status`.
fn pid_owner(pid: u32) -> io::Result<u32> {
    pid_uids(pid).map(|(real, _)| real)
}

/// The real and effective uids of `pid`, from `/proc/<pid>/status`.
fn pid_uids(pid: u32) -> io::Result<(u32, u32)> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status"))?;
    parse_uid_line(&status).ok_or_else(|| io::Error::other("no Uid line in /proc status"))
}

/// `Uid:\treal\teffective\tsaved\tfs` -> `(real, effective)`.
fn parse_uid_line(status: &str) -> Option<(u32, u32)> {
    let rest = status.lines().find_map(|line| line.strip_prefix("Uid:"))?;
    let mut fields = rest.split_whitespace();
    let real = fields.next()?.parse().ok()?;
    let effective = fields.next()?.parse().ok()?;
    Some((real, effective))
}

async fn serve_connection<H: ProbeHandle + Send + 'static>(
    shared: &Shared<H>,
    stream: UnixStream,
    peer: Peer,
) -> Result<(), EbpfError> {
    let (read_half, mut writer) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    loop {
        let Some(line) = read_bounded_line(&mut reader).await? else {
            return Ok(());
        };
        let request = match parse_request(&line) {
            Ok(request) => request,
            Err(e) => {
                write_json(&mut writer, &Response::failure(e.to_string())).await?;
                continue;
            }
        };
        if request == Request::Stream {
            // Subscribe before the ack, so no event after the ack is missed.
            let rx = shared.events.subscribe();
            write_json(&mut writer, &Response::ack()).await?;
            return stream_events(rx, reader, writer, peer).await;
        }
        let response = dispatch(shared, request, peer);
        write_json(&mut writer, &response).await?;
    }
}

fn dispatch<H: ProbeHandle>(shared: &Shared<H>, request: Request, peer: Peer) -> Response {
    match request {
        Request::Track { pid, ppid } => {
            if let Err(reason) = peer.may_track(pid) {
                tracing::warn!(uid = peer.uid, pid, %reason, "track refused");
                return Response::failure(reason);
            }
            let mut tracked = lock(&shared.tracked);
            let mine = tracked.values().filter(|uid| **uid == peer.uid).count();
            if peer.uid != 0 && !tracked.contains_key(&pid) && mine >= shared.track_quota {
                let reason = format!(
                    "track quota {} reached for uid {}",
                    shared.track_quota, peer.uid
                );
                tracing::warn!(uid = peer.uid, pid, %reason, "track refused");
                return Response::failure(reason);
            }
            match lock(&shared.handle).track(pid, ppid) {
                Ok(()) => {
                    tracked.insert(pid, peer.uid);
                    Response::ack()
                }
                Err(e) => Response::failure(e.to_string()),
            }
        }
        Request::Untrack { pid } => {
            let mut tracked = lock(&shared.tracked);
            let owner = tracked.get(&pid).copied();
            if peer.uid != 0 && owner != Some(peer.uid) {
                let reason = format!("pid {pid} is not tracked by uid {}", peer.uid);
                tracing::warn!(uid = peer.uid, pid, %reason, "untrack refused");
                return Response::failure(reason);
            }
            match lock(&shared.handle).untrack(pid) {
                Ok(()) => {
                    tracked.remove(&pid);
                    Response::ack()
                }
                Err(e) => Response::failure(e.to_string()),
            }
        }
        Request::Status => {
            let mut tracked = lock(&shared.tracked);
            let handle = lock(&shared.handle);
            tracked.retain(|pid, _| handle.is_tracked(*pid).unwrap_or(false));
            let count = if peer.uid == 0 {
                tracked.len()
            } else {
                tracked.values().filter(|uid| **uid == peer.uid).count()
            };
            match handle.dropped_events() {
                Ok(dropped) => Response::status(count as u64, dropped),
                Err(e) => Response::failure(e.to_string()),
            }
        }
        // Handled by the caller; kept total so the match has no wildcard.
        Request::Stream => Response::failure("stream is handled by the connection"),
    }
}

async fn stream_events(
    mut rx: broadcast::Receiver<StreamLine>,
    mut reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    mut writer: OwnedWriteHalf,
    peer: Peer,
) -> Result<(), EbpfError> {
    let mut scratch = [0u8; 256];
    loop {
        tokio::select! {
            received = rx.recv() => {
                let line = match received {
                    // Another user's exec records are not this peer's to see.
                    Ok(StreamLine::Event(record)) if peer.uid != 0 && record.uid != peer.uid => continue,
                    Ok(line) => line,
                    Err(RecvError::Lagged(missed)) => StreamLine::Lagged(missed),
                    Err(RecvError::Closed) => return Ok(()),
                };
                write_json(&mut writer, &line).await?;
            }
            // Anything the client sends now is ignored. EOF or an error ends the stream.
            read = reader.read(&mut scratch) => {
                if matches!(read, Ok(0) | Err(_)) {
                    return Ok(());
                }
            }
        }
    }
}

/// Reads one line of at most [`MAX_LINE`] bytes.
///
/// `Ok(None)` is a clean close. A longer line is an error, which closes the
/// connection. The read is bounded, so a client cannot grow the buffer.
async fn read_bounded_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<String>, EbpfError> {
    let mut buf = Vec::new();
    let mut limited = (&mut *reader).take(MAX_LINE as u64 + 1);
    let n = limited.read_until(b'\n', &mut buf).await?;
    if n == 0 {
        return Ok(None);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
    } else if buf.len() > MAX_LINE {
        return Err(EbpfError::Protocol(format!(
            "line exceeds {MAX_LINE} bytes"
        )));
    }
    String::from_utf8(buf)
        .map(Some)
        .map_err(|_| EbpfError::Protocol("line is not utf-8".into()))
}

async fn write_json<T: Serialize>(writer: &mut OwnedWriteHalf, value: &T) -> Result<(), EbpfError> {
    let mut frame =
        serde_json::to_vec(value).map_err(|e| EbpfError::Protocol(format!("encode frame: {e}")))?;
    frame.push(b'\n');
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_line_gives_real_and_effective() {
        let status = "Name:\tsudo\nUid:\t1000\t0\t0\t0\nGid:\t1000\t1000\t1000\t1000\n";
        assert_eq!(parse_uid_line(status), Some((1000, 0)));
        assert_eq!(parse_uid_line("Name:\tx\n"), None);
        assert_eq!(parse_uid_line("Uid:\tnope\t0\n"), None);
    }

    #[test]
    fn group_line_matches_the_name_only() {
        assert_eq!(parse_group_line("docker:x:999:ctodie", "docker"), Some(999));
        assert_eq!(parse_group_line("docker:x:999:ctodie", "dock"), None);
        assert_eq!(parse_group_line("docker:x:abc:", "docker"), None);
        assert_eq!(parse_group_line("broken", "broken"), None);
    }

    #[test]
    fn root_group_resolves_to_zero() {
        assert_eq!(resolve_group("root").expect("root group"), 0);
        assert!(resolve_group("no-such-group-cas-ebpf").is_err());
    }
}
