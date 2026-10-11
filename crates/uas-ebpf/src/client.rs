// SPDX-License-Identifier: Apache-2.0 OR MIT
//! A blocking client for `uas-ebpfd` (spec 0014 D8).
//!
//! `uas-pool` uses it to track each worker pid and to read the exec stream.
//!
//! Unprivileged fallback (spec 0014 Deployment, last bullet): when
//! [`DaemonClient::connect`] returns [`EbpfError::DaemonUnavailable`], the
//! caller clamps the worker with [`crate::clamp::enter_clamped_group`] alone
//! and marks the execution receipt unattested. No audit events exist then.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::Path,
};

use crate::{
    error::EbpfError,
    protocol::{ReceiptFrame, Request, Response, StreamLine, MAX_LINE},
};

/// A connection to the daemon. One request at a time.
#[derive(Debug)]
pub struct DaemonClient {
    reader: BufReader<UnixStream>,
}

impl DaemonClient {
    /// Connects to the daemon socket at `path`.
    pub fn connect(path: impl AsRef<Path>) -> Result<Self, EbpfError> {
        let path = path.as_ref();
        let stream = UnixStream::connect(path).map_err(|source| EbpfError::DaemonUnavailable {
            path: path.display().to_string(),
            source,
        })?;
        Ok(Self {
            reader: BufReader::new(stream),
        })
    }

    /// Connects like [`DaemonClient::connect`], and gives every request on the
    /// connection a read and write timeout. A daemon that stops answering then
    /// fails the request with an I/O timeout instead of blocking the caller.
    /// The bound covers requests, not the connect itself: a daemon whose listen
    /// backlog is full blocks `connect` without a bound (Unix sockets have no
    /// connect timeout in std).
    pub fn connect_with_timeout(
        path: impl AsRef<Path>,
        timeout: std::time::Duration,
    ) -> Result<Self, EbpfError> {
        let client = Self::connect(path)?;
        let stream = client.reader.get_ref();
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(client)
    }

    /// Tracks `pid` with parent `ppid`.
    pub fn track(&mut self, pid: u32, ppid: u32) -> Result<(), EbpfError> {
        self.expect_ack(&Request::Track { pid, ppid })
    }

    /// Stops tracking `pid`.
    pub fn untrack(&mut self, pid: u32) -> Result<(), EbpfError> {
        self.expect_ack(&Request::Untrack { pid })
    }

    /// The tracked count and the dropped-event counter.
    pub fn status(&mut self) -> Result<(u64, u64), EbpfError> {
        match self.round_trip(&Request::Status)? {
            Response::Status {
                tracked,
                dropped_events,
                ..
            } => Ok((tracked, dropped_events)),
            Response::Failure { error, .. } => Err(EbpfError::Protocol(error)),
            Response::Ack { .. } | Response::Receipt { .. } => {
                Err(EbpfError::Protocol("status reply had no counts".into()))
            }
        }
    }

    /// Reads the receipt window of `pid`: the totals since its `track` (spec 0014 D14).
    ///
    /// The window stays open. A window is owned by this connection: when the
    /// connection closes, the daemon discards it.
    pub fn receipt(&mut self, pid: u32) -> Result<ReceiptFrame, EbpfError> {
        match self.round_trip(&Request::Receipt { pid })? {
            Response::Receipt { receipt } => Ok(receipt),
            Response::Failure { error, .. } => Err(EbpfError::Protocol(error)),
            _ => Err(EbpfError::Protocol("receipt reply had no receipt".into())),
        }
    }

    /// Discards the receipt window of `pid`. Kernel tracking is unchanged (spec 0014 D14).
    pub fn forget(&mut self, pid: u32) -> Result<(), EbpfError> {
        self.expect_ack(&Request::Forget { pid })
    }

    /// Switches this connection to the event stream.
    ///
    /// The returned iterator ends when the daemon closes the socket. Drop it
    /// to close the connection.
    pub fn stream(mut self) -> Result<EventStream, EbpfError> {
        match self.round_trip(&Request::Stream)? {
            Response::Failure { error, .. } => Err(EbpfError::Protocol(error)),
            _ => Ok(EventStream {
                reader: self.reader,
            }),
        }
    }

    fn expect_ack(&mut self, request: &Request) -> Result<(), EbpfError> {
        match self.round_trip(request)? {
            Response::Failure { error, .. } => Err(EbpfError::Protocol(error)),
            _ => Ok(()),
        }
    }

    fn round_trip(&mut self, request: &Request) -> Result<Response, EbpfError> {
        let mut frame = serde_json::to_vec(request)
            .map_err(|e| EbpfError::Protocol(format!("encode request: {e}")))?;
        frame.push(b'\n');
        let stream = self.reader.get_mut();
        stream.write_all(&frame)?;
        stream.flush()?;
        let line = read_line(&mut self.reader)?
            .ok_or_else(|| EbpfError::Protocol("daemon closed the connection".into()))?;
        serde_json::from_str(&line).map_err(|e| EbpfError::Protocol(format!("bad response: {e}")))
    }
}

/// The event stream of one connection. Each item is one frame.
#[derive(Debug)]
pub struct EventStream {
    reader: BufReader<UnixStream>,
}

impl Iterator for EventStream {
    type Item = Result<StreamLine, EbpfError>;

    fn next(&mut self) -> Option<Self::Item> {
        match read_line(&mut self.reader) {
            Ok(None) => None,
            Ok(Some(line)) => Some(
                serde_json::from_str(&line)
                    .map_err(|e| EbpfError::Protocol(format!("bad stream line: {e}"))),
            ),
            Err(e) => Some(Err(e)),
        }
    }
}

/// Reads one line, at most [`MAX_LINE`] bytes. `None` on a clean close.
fn read_line(reader: &mut BufReader<UnixStream>) -> Result<Option<String>, EbpfError> {
    let mut buf = Vec::new();
    let mut limited = std::io::Read::take(&mut *reader, MAX_LINE as u64 + 1);
    let n = limited.read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
    } else if buf.len() > MAX_LINE {
        return Err(EbpfError::Protocol("daemon line too long".into()));
    }
    String::from_utf8(buf)
        .map(Some)
        .map_err(|_| EbpfError::Protocol("daemon line is not utf-8".into()))
}
