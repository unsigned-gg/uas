//! The aya loader: loads `BPF_OBJECT`, attaches the tracepoints and owns the maps.
//!
//! Placeholder with the final signatures. A builder fills it in this branch.

use std::os::fd::BorrowedFd;

use crate::{error::EbpfError, handle::ProbeHandle, wire::ExecEvent};

/// The loaded kernel program and its maps (spec 0014 D3, D9).
#[derive(Debug)]
pub struct Probes {
    _private: (),
}

impl Probes {
    /// Loads the committed object, attaches the three sched tracepoints and opens the maps.
    pub fn load() -> Result<Self, EbpfError> {
        Err(EbpfError::PermissionDenied)
    }
}

impl ProbeHandle for Probes {
    fn track(&mut self, _pid: u32, _ppid: u32) -> Result<(), EbpfError> {
        Err(EbpfError::PermissionDenied)
    }

    fn untrack(&mut self, _pid: u32) -> Result<(), EbpfError> {
        Err(EbpfError::PermissionDenied)
    }

    fn is_tracked(&self, _pid: u32) -> Result<bool, EbpfError> {
        Err(EbpfError::PermissionDenied)
    }

    fn dropped_events(&self) -> Result<u64, EbpfError> {
        Err(EbpfError::PermissionDenied)
    }

    fn next_event(&mut self) -> Option<ExecEvent> {
        None
    }

    fn event_fd(&self) -> BorrowedFd<'_> {
        // Placeholder: stdin. Replaced by the ring buffer descriptor.
        #[allow(unsafe_code)]
        // SAFETY: descriptor 0 stays open for the life of the process in this placeholder.
        unsafe {
            BorrowedFd::borrow_raw(0)
        }
    }
}
