// SPDX-License-Identifier: Apache-2.0 OR MIT
//! The interface the daemon drives, and a fake for unprivileged tests.
//!
//! [`crate::loader::Probes`] implements [`ProbeHandle`] over the live BPF maps.
//! [`FakeProbeHandle`] implements it over a queue in memory, so the daemon,
//! the protocol and the client have tests that run without `CAP_BPF`.

use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, Read, Write},
    os::{
        fd::{AsFd, BorrowedFd},
        unix::net::UnixStream,
    },
};

use crate::{error::EbpfError, wire::ExecEvent};

/// What the daemon needs from the kernel side.
pub trait ProbeHandle {
    /// Starts tracking `pid`; its children inherit tracking at fork.
    fn track(&mut self, pid: u32, ppid: u32) -> Result<(), EbpfError>;
    /// Stops tracking `pid`.
    fn untrack(&mut self, pid: u32) -> Result<(), EbpfError>;
    /// `true` when `pid` is in the tracked map.
    fn is_tracked(&self, pid: u32) -> Result<bool, EbpfError>;
    /// Records the kernel could not reserve space for, summed over all cpus.
    fn dropped_events(&self) -> Result<u64, EbpfError>;
    /// Fork inserts and track promotions refused because `TRACKED_PIDS` was
    /// full, summed over all cpus (spec 0014 D14: the second counter slot).
    fn tracking_failures(&self) -> Result<u64, EbpfError>;
    /// The next record, or `None` when the ring buffer is empty.
    fn next_event(&mut self) -> Option<ExecEvent>;
    /// A descriptor that becomes readable when a record is available.
    fn event_fd(&self) -> BorrowedFd<'_>;
}

/// An in-memory [`ProbeHandle`]: a tracked map and a queue of events.
///
/// Readiness is signalled over a socket pair, one byte per pushed event, so an
/// `AsyncFd` on [`ProbeHandle::event_fd`] wakes the same way as on a ring buffer.
#[derive(Debug)]
pub struct FakeProbeHandle {
    tracked: BTreeMap<u32, u32>,
    queue: VecDeque<ExecEvent>,
    dropped: u64,
    tracking_failures: u64,
    reader: UnixStream,
    writer: UnixStream,
}

impl FakeProbeHandle {
    /// An empty fake.
    pub fn new() -> io::Result<Self> {
        let (reader, writer) = UnixStream::pair()?;
        reader.set_nonblocking(true)?;
        Ok(Self {
            tracked: BTreeMap::new(),
            queue: VecDeque::new(),
            dropped: 0,
            tracking_failures: 0,
            reader,
            writer,
        })
    }

    /// Queues an event and signals readiness.
    pub fn push(&mut self, event: ExecEvent) -> io::Result<()> {
        self.queue.push_back(event);
        self.writer.write_all(&[1])
    }

    /// Adds to the dropped counter.
    pub fn add_dropped(&mut self, count: u64) {
        self.dropped += count;
    }

    /// Adds to the tracking-failure counter.
    pub fn add_tracking_failures(&mut self, count: u64) {
        self.tracking_failures += count;
    }

    /// The tracked map as `(pid, ppid)` pairs, in pid order.
    pub fn tracked(&self) -> Vec<(u32, u32)> {
        self.tracked.iter().map(|(k, v)| (*k, *v)).collect()
    }
}

impl ProbeHandle for FakeProbeHandle {
    fn track(&mut self, pid: u32, ppid: u32) -> Result<(), EbpfError> {
        self.tracked.insert(pid, ppid);
        Ok(())
    }

    fn untrack(&mut self, pid: u32) -> Result<(), EbpfError> {
        self.tracked.remove(&pid);
        Ok(())
    }

    fn is_tracked(&self, pid: u32) -> Result<bool, EbpfError> {
        Ok(self.tracked.contains_key(&pid))
    }

    fn dropped_events(&self) -> Result<u64, EbpfError> {
        Ok(self.dropped)
    }

    fn tracking_failures(&self) -> Result<u64, EbpfError> {
        Ok(self.tracking_failures)
    }

    fn next_event(&mut self) -> Option<ExecEvent> {
        let event = self.queue.pop_front()?;
        let mut byte = [0u8; 1];
        // Consume one readiness byte per event. WouldBlock means the signal
        // was already consumed, which is harmless.
        let _ = self.reader.read(&mut byte);
        Some(event)
    }

    fn event_fd(&self) -> BorrowedFd<'_> {
        self.reader.as_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_tracks_and_queues() {
        let mut fake = FakeProbeHandle::new().expect("socket pair");
        fake.track(10, 1).expect("track");
        fake.track(11, 10).expect("track");
        assert!(fake.is_tracked(10).expect("lookup"));
        fake.untrack(10).expect("untrack");
        assert!(!fake.is_tracked(10).expect("lookup"));
        assert_eq!(fake.tracked(), vec![(11, 10)]);

        assert!(fake.next_event().is_none());
        fake.push(ExecEvent::new(1, 11, 10, 0, "cc", "/usr/bin/cc"))
            .expect("push");
        let event = fake.next_event().expect("queued event");
        assert_eq!(event.pid, 11);
        assert!(fake.next_event().is_none());

        fake.add_dropped(4);
        assert_eq!(fake.dropped_events().expect("count"), 4);
    }

    #[test]
    fn fake_signals_readiness_on_push() {
        let mut fake = FakeProbeHandle::new().expect("socket pair");
        let mut byte = [0u8; 1];
        assert!(fake.reader.read(&mut byte).is_err(), "empty before push");
        fake.push(ExecEvent::new(1, 2, 3, 4, "a", "/a"))
            .expect("push");
        let n = fake.reader.read(&mut byte).expect("readable after push");
        assert_eq!(n, 1);
    }
}
