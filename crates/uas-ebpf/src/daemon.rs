//! The `uas-ebpfd` server (spec 0014 D8, ruling R3).
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
//!
//! Pid namespaces (spec 0014 D12): every pid a request carries is a pid of the
//! daemon's namespace. A peer in another pid namespace would send pids of its
//! own, so the daemon refuses it, root included: its `SO_PEERCRED` pid must
//! exist and its `/proc/<pid>/ns/pid` must match the daemon's. An exec event
//! with `ns_pid` 0 names a task the kernel could not translate; the daemon
//! logs and streams it but never re-clamps it, and `track` and `untrack` of
//! pid 0 are refused.

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::Duration,
};

use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{unix::OwnedWriteHalf, UnixListener, UnixStream},
    sync::broadcast::{self, error::RecvError},
};

use crate::{
    audit::RunHash,
    clamp,
    error::EbpfError,
    handle::ProbeHandle,
    protocol::{parse_request, ReceiptFrame, Request, Response, StreamLine, MAX_LINE},
    wire::ExecEvent,
};

/// Capacity of the broadcast channel. A slow stream client lags past this.
const STREAM_BACKLOG: usize = 1024;
/// How often the reader polls when no event arrives (dropped counter check).
const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Socket mode: owner and group may connect (spec 0014 Deployment).
const SOCKET_MODE: u32 = 0o660;
/// The error a peer in another pid namespace gets on its first request.
const NAMESPACE_REFUSAL: &str = "peer pid namespace differs from the daemon's";
/// Default number of pids one non-root uid may track at once. `TRACKED_PIDS`
/// has 1024 entries shared by every client and every thread clone.
pub const DEFAULT_TRACK_QUOTA: usize = 256;
/// Most pids one receipt window keeps in its member set (spec 0014 D14).
const MEMBER_CAP: usize = 4096;

/// One root's receipt window (spec 0014 D14).
struct Window {
    /// The uid that tracked the root.
    uid: u32,
    /// The connection that opened the window. Closing it discards the window.
    conn: u64,
    /// Namespace pids of the tree: the root, and every non-zero `ns_pid` absorbed.
    members: BTreeSet<u32>,
    /// The transcript. Its counters are the window's `events` and `dropped_events`.
    transcript: RunHash,
    first_ns: u64,
    last_ns: u64,
    /// `false` after `untrack`: the window stays readable and absorbs nothing more.
    open: bool,
}

impl Window {
    fn open(root: u32, uid: u32, conn: u64) -> Self {
        Self {
            uid,
            conn,
            members: BTreeSet::from([root]),
            transcript: RunHash::new(),
            first_ns: 0,
            last_ns: 0,
            open: true,
        }
    }

    /// Absorbs one record into the transcript and grows the member set.
    fn absorb(&mut self, event: &ExecEvent) {
        if self.transcript.events() == 0 {
            self.first_ns = event.timestamp_ns;
        }
        self.last_ns = event.timestamp_ns;
        self.transcript.absorb(event);
        if event.ns_pid != 0 && !self.members.contains(&event.ns_pid) {
            if self.members.len() >= MEMBER_CAP {
                self.transcript.record_dropped(1);
            } else {
                self.members.insert(event.ns_pid);
            }
        }
    }

    fn frame(&self) -> ReceiptFrame {
        ReceiptFrame {
            run_hash: self.transcript.clone().finish().run_hash,
            events: self.transcript.events(),
            dropped_events: self.transcript.dropped_events(),
            first_ns: self.first_ns,
            last_ns: self.last_ns,
        }
    }
}

/// Everything the reader task and the requests share. One lock, so one drain
/// runs at a time and a drain is always taken before the answer (spec 0014 D14).
#[derive(Default)]
struct Ledger {
    /// Pids tracked through this daemon, with the uid that tracked each. The
    /// handle has no count call, so `status` prunes this map against the
    /// kernel map (exit removes entries).
    tracked: BTreeMap<u32, u32>,
    /// Receipt windows, keyed by root pid.
    windows: BTreeMap<u32, Window>,
    /// The last dropped total (slot 0) applied to the windows.
    last_dropped: u64,
    /// The last tracking-failure total (slot 1) applied to the windows.
    last_tracking: u64,
}

impl Ledger {
    /// Root pids whose member set holds `pid`, in any window state.
    fn holders(&self, pid: u32) -> Vec<u32> {
        self.windows
            .iter()
            .filter(|(_, w)| w.members.contains(&pid))
            .map(|(root, _)| *root)
            .collect()
    }

    /// Places one record (spec 0014 D14). The parent pid is tried first, then
    /// the event's own pid. A single match that is an open window absorbs the
    /// record. A single match that is closed discards it. Anything else is
    /// unplaced and charged to every open window of its uid, or of uid 0.
    fn place(&mut self, event: &ExecEvent) {
        let mut matched = self.holders(event.ppid);
        if matched.is_empty() && event.ns_pid != 0 {
            matched = self.holders(event.ns_pid);
        }
        match matched.as_slice() {
            [root] => {
                if let Some(window) = self.windows.get_mut(root) {
                    if window.open {
                        window.absorb(event);
                    }
                }
            }
            _ => {
                for window in self
                    .windows
                    .values_mut()
                    .filter(|w| w.open && (w.uid == event.uid || w.uid == 0))
                {
                    window.transcript.record_dropped(1);
                }
            }
        }
    }
}

struct Shared<H> {
    handle: Arc<Mutex<H>>,
    ledger: Mutex<Ledger>,
    events: broadcast::Sender<StreamLine>,
    track_quota: usize,
    /// The pid namespace a peer must share, as `[dev, ino]`.
    pid_namespace: [u64; 2],
    /// The id the next connection gets. Windows carry the id of their connection.
    next_conn: AtomicU64,
}

/// The daemon: a probe handle served on a Unix socket.
pub struct Daemon<H: ProbeHandle + Send + 'static> {
    handle: Arc<Mutex<H>>,
    path: PathBuf,
    socket_gid: Option<u32>,
    track_quota: usize,
    pid_namespace: Option<[u64; 2]>,
}

impl<H: ProbeHandle + Send + 'static> Daemon<H> {
    /// A daemon over `handle` that will listen at `path`.
    pub fn new(handle: Arc<Mutex<H>>, path: impl Into<PathBuf>) -> Self {
        Self {
            handle,
            path: path.into(),
            socket_gid: None,
            track_quota: DEFAULT_TRACK_QUOTA,
            pid_namespace: None,
        }
    }

    /// Sets the pid namespace, as `[dev, ino]`, that a peer must share. The
    /// default is the `stat` of `/proc/self/ns/pid` at [`Daemon::serve`].
    pub fn with_pid_namespace(mut self, namespace: [u64; 2]) -> Self {
        self.pid_namespace = Some(namespace);
        self
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
        tracing::info!(socket = %self.path.display(), gid = ?self.socket_gid, "uas-ebpfd listening");

        let pid_namespace = match self.pid_namespace {
            Some(namespace) => namespace,
            None => namespace_of("self")?,
        };
        let (events, _) = broadcast::channel(STREAM_BACKLOG);
        let shared = Arc::new(Shared {
            handle: self.handle,
            ledger: Mutex::new(Ledger::default()),
            events,
            track_quota: self.track_quota,
            pid_namespace,
            next_conn: AtomicU64::new(1),
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
    loop {
        tokio::select! {
            ready = afd.readable() => {
                let mut guard = ready?;
                {
                    let mut ledger = lock(&shared.ledger);
                    drain(&shared.handle, &mut ledger, &shared.events);
                }
                guard.clear_ready();
            }
            _ = tick.tick() => {
                let mut ledger = lock(&shared.ledger);
                drain(&shared.handle, &mut ledger, &shared.events);
            }
        }
    }
}

/// Reads every queued record, re-clamps, places each in the receipt windows,
/// broadcasts, then applies the growth of the dropped counter to the open windows.
fn drain<H: ProbeHandle>(
    handle: &Mutex<H>,
    ledger: &mut Ledger,
    events: &broadcast::Sender<StreamLine>,
) {
    let (batch, dropped, tracking) = {
        let mut handle = lock(handle);
        let mut batch: Vec<ExecEvent> = Vec::new();
        while let Some(event) = handle.next_event() {
            batch.push(event);
        }
        (batch, handle.dropped_events(), handle.tracking_failures())
    };
    for event in batch {
        // A setuid binary runs with an effective uid that differs from the real
        // one. Never renice such a process, even if a tracked worker exec'd it.
        // /proc and setpriority take pids of this process's namespace.
        let pid = event.ns_pid;
        if pid == 0 {
            // Unresolved (D12): the task has no pid the daemon can name. `who`
            // 0 in setpriority names the caller, so never pass it on.
            tracing::warn!(
                global_pid = event.pid,
                "exec of a tracked task with no pid in the daemon's namespace; not re-clamped"
            );
        } else {
            reclamp_event(pid);
        }
        ledger.place(&event);
        // No receivers is normal: nobody is streaming.
        let _ = events.send(StreamLine::Event(event.record()));
    }
    // Both DROPPED_EVENTS slots grow every open window (spec 0014 D14).
    let mut grown = 0;
    match dropped {
        Ok(total) if total > ledger.last_dropped => {
            grown += total - ledger.last_dropped;
            ledger.last_dropped = total;
            let _ = events.send(StreamLine::Dropped(total));
        }
        Ok(_) => {}
        Err(e) => tracing::debug!(error = %e, "dropped counter unreadable"),
    }
    match tracking {
        Ok(total) if total > ledger.last_tracking => {
            grown += total - ledger.last_tracking;
            ledger.last_tracking = total;
        }
        Ok(_) => {}
        Err(e) => tracing::debug!(error = %e, "tracking-failure counter unreadable"),
    }
    if grown > 0 {
        for window in ledger.windows.values_mut().filter(|w| w.open) {
            window.transcript.record_dropped(grown);
        }
    }
}

/// Re-clamps `pid` (a pid of the daemon's namespace) unless it runs setuid.
fn reclamp_event(pid: u32) {
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
}

async fn connection<H: ProbeHandle + Send + 'static>(shared: Arc<Shared<H>>, stream: UnixStream) {
    let conn = shared.next_conn.fetch_add(1, Ordering::Relaxed);
    let _windows = ConnWindows {
        shared: Arc::clone(&shared),
        conn,
    };
    let peer = match stream.peer_cred() {
        Ok(cred) => Peer {
            uid: cred.uid(),
            pid: cred.pid().and_then(|pid| u32::try_from(pid).ok()),
            conn,
        },
        Err(e) => {
            tracing::warn!(error = %e, "refusing a connection without peer credentials");
            return;
        }
    };
    if let Err(e) = serve_connection(&shared, stream, peer).await {
        tracing::debug!(error = %e, "connection ended");
    }
}

/// Discards the receipt windows of a connection when it ends, on every path.
struct ConnWindows<H> {
    shared: Arc<Shared<H>>,
    conn: u64,
}

impl<H> Drop for ConnWindows<H> {
    fn drop(&mut self) {
        lock(&self.shared.ledger)
            .windows
            .retain(|_, window| window.conn != self.conn);
    }
}

/// The credentials of a connected client (`SO_PEERCRED`), and its connection id.
#[derive(Clone, Copy, Debug)]
struct Peer {
    uid: u32,
    /// The peer's pid in the daemon's namespace, when the kernel reports one.
    pid: Option<u32>,
    /// The daemon's id for this connection. Its windows carry it.
    conn: u64,
}

impl Peer {
    /// `true` when this peer's pid namespace is `namespace` (`[dev, ino]`).
    /// A peer with no pid, pid 0, or an unreadable namespace never matches.
    fn shares_namespace(self, namespace: [u64; 2]) -> bool {
        match self.pid {
            None | Some(0) => false,
            Some(pid) => namespace_of(&pid.to_string()).is_ok_and(|ns| ns == namespace),
        }
    }

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

/// `[dev, ino]` of `/proc/<who>/ns/pid`, where `who` is a pid or `self`.
fn namespace_of(who: &str) -> io::Result<[u64; 2]> {
    let st = rustix::fs::stat(format!("/proc/{who}/ns/pid")).map_err(io::Error::from)?;
    Ok([st.st_dev, st.st_ino])
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
    let same_namespace = peer.shares_namespace(shared.pid_namespace);
    loop {
        let Some(line) = read_bounded_line(&mut reader).await? else {
            return Ok(());
        };
        if !same_namespace {
            // Its pids are not ours (D12). Answer the first request, then close.
            tracing::warn!(uid = peer.uid, pid = ?peer.pid, "refused a peer in another pid namespace");
            write_json(&mut writer, &Response::failure(NAMESPACE_REFUSAL)).await?;
            return Ok(());
        }
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
        Request::Track { pid: 0, .. } => Response::failure("pid 0 cannot be tracked"),
        Request::Untrack { pid: 0 } => Response::failure("pid 0 cannot be untracked"),
        Request::Receipt { pid: 0 } | Request::Forget { pid: 0 } => {
            Response::failure("pid 0 has no receipt window")
        }
        Request::Track { pid, ppid } => {
            if let Err(reason) = peer.may_track(pid) {
                tracing::warn!(uid = peer.uid, pid, %reason, "track refused");
                return Response::failure(reason);
            }
            let mut ledger = lock(&shared.ledger);
            drain(&shared.handle, &mut ledger, &shared.events);
            // A pid held by another uid's window is never merged into this uid's tree,
            // and its window is never replaced. Only that uid's forget frees it, or its
            // disconnect (lane decision 2026-10-10, spec 0014 D14).
            if let Some(holder) = ledger.windows.get(&pid).map(|w| w.uid) {
                if holder != peer.uid {
                    let reason = format!("pid {pid} is held by a receipt window of uid {holder}");
                    tracing::warn!(uid = peer.uid, pid, %reason, "track refused");
                    return Response::failure(reason);
                }
            }
            if ledger.windows.get(&pid).is_some_and(|w| !w.open) {
                let reason = format!("pid {pid} has a closed receipt window; forget it first");
                tracing::warn!(uid = peer.uid, pid, %reason, "track refused");
                return Response::failure(reason);
            }
            // A window counts toward the quota until it is forgotten, once per pid.
            let held: BTreeSet<u32> = ledger
                .tracked
                .iter()
                .filter(|(_, uid)| **uid == peer.uid)
                .map(|(pid, _)| *pid)
                .chain(
                    ledger
                        .windows
                        .iter()
                        .filter(|(_, w)| w.uid == peer.uid)
                        .map(|(pid, _)| *pid),
                )
                .collect();
            if peer.uid != 0 && !held.contains(&pid) && held.len() >= shared.track_quota {
                let reason = format!(
                    "track quota {} reached for uid {}",
                    shared.track_quota, peer.uid
                );
                tracing::warn!(uid = peer.uid, pid, %reason, "track refused");
                return Response::failure(reason);
            }
            match lock(&shared.handle).track(pid, ppid) {
                Ok(()) => {
                    ledger.tracked.insert(pid, peer.uid);
                    ledger
                        .windows
                        .entry(pid)
                        .or_insert_with(|| Window::open(pid, peer.uid, peer.conn));
                    Response::ack()
                }
                Err(e) => Response::failure(e.to_string()),
            }
        }
        Request::Untrack { pid } => {
            let mut ledger = lock(&shared.ledger);
            drain(&shared.handle, &mut ledger, &shared.events);
            let owner = ledger
                .tracked
                .get(&pid)
                .copied()
                .or_else(|| ledger.windows.get(&pid).map(|w| w.uid));
            if peer.uid != 0 && owner != Some(peer.uid) {
                let reason = format!("pid {pid} is not tracked by uid {}", peer.uid);
                tracing::warn!(uid = peer.uid, pid, %reason, "untrack refused");
                return Response::failure(reason);
            }
            match lock(&shared.handle).untrack(pid) {
                Ok(()) => {
                    ledger.tracked.remove(&pid);
                    if let Some(window) = ledger.windows.get_mut(&pid) {
                        window.open = false;
                    }
                    Response::ack()
                }
                Err(e) => Response::failure(e.to_string()),
            }
        }
        Request::Receipt { pid } => {
            let mut ledger = lock(&shared.ledger);
            drain(&shared.handle, &mut ledger, &shared.events);
            match ledger.windows.get(&pid) {
                None => Response::failure(format!("pid {pid} has no receipt window")),
                Some(window) if peer.uid != 0 && window.uid != peer.uid => {
                    Response::failure(format!("pid {pid} is not tracked by uid {}", peer.uid))
                }
                Some(window) => Response::Receipt {
                    receipt: window.frame(),
                },
            }
        }
        Request::Forget { pid } => {
            let mut ledger = lock(&shared.ledger);
            let owner = ledger.windows.get(&pid).map(|w| w.uid);
            match owner {
                None => Response::failure(format!("pid {pid} has no receipt window")),
                Some(uid) if peer.uid != 0 && uid != peer.uid => {
                    Response::failure(format!("pid {pid} is not tracked by uid {}", peer.uid))
                }
                Some(_) => {
                    ledger.windows.remove(&pid);
                    Response::ack()
                }
            }
        }
        Request::Status => {
            let mut ledger = lock(&shared.ledger);
            let handle = lock(&shared.handle);
            ledger
                .tracked
                .retain(|pid, _| handle.is_tracked(*pid).unwrap_or(false));
            let count = if peer.uid == 0 {
                ledger.tracked.len()
            } else {
                ledger
                    .tracked
                    .values()
                    .filter(|uid| **uid == peer.uid)
                    .count()
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
    fn a_peer_must_have_a_pid_in_the_daemons_namespace() {
        let own = namespace_of("self").expect("own namespace");
        let me = std::process::id();
        let peer = |pid| Peer {
            uid: 0,
            pid,
            conn: 0,
        };
        assert!(peer(Some(me)).shares_namespace(own));
        assert!(!peer(Some(me)).shares_namespace([own[0], own[1] + 1]));
        assert!(!peer(None).shares_namespace(own));
        assert!(!peer(Some(0)).shares_namespace(own));
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
        assert!(resolve_group("no-such-group-uas-ebpf").is_err());
    }

    /// A daemon state with no windows and a fake handle, for dispatch-level tests.
    fn test_shared() -> Shared<crate::handle::FakeProbeHandle> {
        let (events, _) = broadcast::channel(STREAM_BACKLOG);
        Shared {
            handle: Arc::new(Mutex::new(
                crate::handle::FakeProbeHandle::new().expect("fake"),
            )),
            ledger: Mutex::new(Ledger::default()),
            events,
            track_quota: DEFAULT_TRACK_QUOTA,
            pid_namespace: [0, 0],
            next_conn: AtomicU64::new(1),
        }
    }

    /// A window of `uid` rooted at `root`, with extra member pids.
    fn window(root: u32, uid: u32, members: &[u32]) -> (u32, Window) {
        let mut window = Window::open(root, uid, 0);
        window.members.extend(members.iter().copied());
        (root, window)
    }

    fn event_at(ppid: u32, ns_pid: u32, uid: u32) -> ExecEvent {
        let mut event = ExecEvent::new(5, 900, ppid, uid, "x", "/x");
        event.ns_pid = ns_pid;
        event
    }

    #[test]
    fn a_foreign_uid_cannot_read_or_forget_another_uids_window() {
        let shared = test_shared();
        let root = Peer {
            uid: 0,
            pid: None,
            conn: 1,
        };
        let foreign = Peer {
            uid: 4242,
            pid: None,
            conn: 2,
        };
        assert!(matches!(
            dispatch(&shared, Request::Track { pid: 77, ppid: 1 }, root),
            Response::Ack { .. }
        ));
        let read = dispatch(&shared, Request::Receipt { pid: 77 }, foreign);
        assert!(matches!(read, Response::Failure { .. }), "{read:?}");
        let forgot = dispatch(&shared, Request::Forget { pid: 77 }, foreign);
        assert!(matches!(forgot, Response::Failure { .. }), "{forgot:?}");
        // Positive control: the root peer still reads the window the foreign peer could not touch.
        assert!(matches!(
            dispatch(&shared, Request::Receipt { pid: 77 }, root),
            Response::Receipt { .. }
        ));
    }

    #[test]
    fn a_pid_held_by_another_uids_window_is_refused_until_it_is_forgotten() {
        let uid = rustix::process::getuid().as_raw();
        if uid == 0 {
            eprintln!("skipped: running as root, the owner is root");
            return;
        }
        let shared = test_shared();
        let me = std::process::id();
        let root = Peer {
            uid: 0,
            pid: None,
            conn: 1,
        };
        let owner = Peer {
            uid,
            pid: None,
            conn: 2,
        };
        assert!(matches!(
            dispatch(&shared, Request::Track { pid: me, ppid: 1 }, root),
            Response::Ack { .. }
        ));
        let refused = dispatch(&shared, Request::Track { pid: me, ppid: 1 }, owner);
        assert!(
            matches!(&refused, Response::Failure { error, .. } if error.contains("held by")),
            "{refused:?}"
        );
        assert!(matches!(
            dispatch(&shared, Request::Forget { pid: me }, root),
            Response::Ack { .. }
        ));
        assert!(matches!(
            dispatch(&shared, Request::Track { pid: me, ppid: 1 }, owner),
            Response::Ack { .. }
        ));
    }

    #[test]
    fn a_record_whose_ppid_and_ns_pid_name_different_windows_goes_by_ppid() {
        let mut ledger = Ledger::default();
        ledger
            .windows
            .extend([window(100, 7, &[101]), window(200, 7, &[201])]);
        ledger.place(&event_at(101, 201, 7));
        assert_eq!(ledger.windows[&100].transcript.events(), 1, "ppid window");
        assert_eq!(ledger.windows[&200].transcript.events(), 0, "ns_pid window");
    }

    #[test]
    fn a_record_whose_ppid_names_two_windows_is_unplaced() {
        // The rule (D14): a ppid that matches more than one window leaves the record
        // unplaced. It does not fall through to ns_pid, and it does not take the first match.
        let mut ledger = Ledger::default();
        ledger
            .windows
            .extend([window(100, 7, &[300]), window(200, 7, &[300])]);
        // ns_pid 7 would place it in both windows by namespace pid; the ppid rule wins.
        ledger.place(&event_at(300, 100, 7));
        for root in [100, 200] {
            assert_eq!(ledger.windows[&root].transcript.events(), 0, "root {root}");
            assert_eq!(
                ledger.windows[&root].transcript.dropped_events(),
                1,
                "root {root}"
            );
        }
    }

    #[test]
    fn a_stray_record_from_another_uid_is_not_charged_to_this_window() {
        let mut ledger = Ledger::default();
        ledger.windows.extend([window(100, 7, &[100])]);
        ledger.place(&event_at(401, 400, 8));
        assert_eq!(ledger.windows[&100].transcript.events(), 0);
        assert_eq!(
            ledger.windows[&100].transcript.dropped_events(),
            0,
            "uid 7 window"
        );
    }
}
