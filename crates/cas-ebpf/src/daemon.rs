//! The `cas-ebpfd` server (spec 0014 D8, ruling R3).
//!
//! One reader task watches the probe handle's event descriptor. It drains the
//! ring buffer, re-clamps every tracked pid that executed, and fans each record
//! out on a broadcast channel. One task per connection answers requests. The
//! handle sits behind a std mutex that is held for a map call only, never
//! across an await.

use std::{
    collections::BTreeSet,
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

struct Shared<H> {
    handle: Arc<Mutex<H>>,
    /// Pids tracked through this daemon. The handle has no count call, so
    /// `status` prunes this set against the kernel map (exit removes entries).
    tracked: Mutex<BTreeSet<u32>>,
    events: broadcast::Sender<StreamLine>,
}

/// The daemon: a probe handle served on a Unix socket.
pub struct Daemon<H: ProbeHandle + Send + 'static> {
    handle: Arc<Mutex<H>>,
    path: PathBuf,
}

impl<H: ProbeHandle + Send + 'static> Daemon<H> {
    /// A daemon over `handle` that will listen at `path`.
    pub fn new(handle: Arc<Mutex<H>>, path: impl Into<PathBuf>) -> Self {
        Self {
            handle,
            path: path.into(),
        }
    }

    /// Binds the socket and serves until an error ends the reader or the listener.
    pub async fn serve(self) -> Result<(), EbpfError> {
        remove_stale(&self.path)?;
        let listener = UnixListener::bind(&self.path)?;
        std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(SOCKET_MODE))?;
        tracing::info!(socket = %self.path.display(), "cas-ebpfd listening");

        let (events, _) = broadcast::channel(STREAM_BACKLOG);
        let shared = Arc::new(Shared {
            handle: self.handle,
            tracked: Mutex::new(BTreeSet::new()),
            events,
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
) -> Result<(), EbpfError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let daemon = Daemon::new(Arc::new(Mutex::new(handle)), path.as_ref());
    runtime.block_on(daemon.serve())
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
        match clamp::reclamp_if_needed(event.pid) {
            Ok(true) => tracing::info!(pid = event.pid, "re-clamped a tracked pid after exec"),
            Ok(false) => {}
            Err(e) => tracing::debug!(pid = event.pid, error = %e, "re-clamp skipped"),
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
    if let Err(e) = serve_connection(&shared, stream).await {
        tracing::debug!(error = %e, "connection ended");
    }
}

async fn serve_connection<H: ProbeHandle + Send + 'static>(
    shared: &Shared<H>,
    stream: UnixStream,
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
            return stream_events(rx, reader, writer).await;
        }
        let response = dispatch(shared, request);
        write_json(&mut writer, &response).await?;
    }
}

fn dispatch<H: ProbeHandle>(shared: &Shared<H>, request: Request) -> Response {
    match request {
        Request::Track { pid, ppid } => {
            let mut tracked = lock(&shared.tracked);
            match lock(&shared.handle).track(pid, ppid) {
                Ok(()) => {
                    tracked.insert(pid);
                    Response::ack()
                }
                Err(e) => Response::failure(e.to_string()),
            }
        }
        Request::Untrack { pid } => {
            let mut tracked = lock(&shared.tracked);
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
            tracked.retain(|pid| handle.is_tracked(*pid).unwrap_or(false));
            match handle.dropped_events() {
                Ok(dropped) => Response::status(tracked.len() as u64, dropped),
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
) -> Result<(), EbpfError> {
    let mut scratch = [0u8; 256];
    loop {
        tokio::select! {
            received = rx.recv() => {
                let line = match received {
                    Ok(line) => line,
                    Err(RecvError::Lagged(missed)) => StreamLine::Dropped(missed),
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
