//! The aya loader: loads `BPF_OBJECT`, attaches the tracepoints and owns the maps.
//!
//! Needs `CAP_BPF` and `CAP_PERFMON`. [`Probes::load`] runs the host probe first
//! and returns a typed error instead of a raw `EPERM` when the host cannot load.

use std::os::fd::{AsFd, BorrowedFd};

use aya::{
    maps::{HashMap, MapData, MapError, PerCpuArray, RingBuf},
    programs::TracePoint,
    Ebpf,
};

use crate::{
    error::EbpfError, handle::ProbeHandle, probe, wire::ExecEvent, AUDIT_EVENTS_MAP, BPF_OBJECT,
    DROPPED_EVENTS_MAP, TRACEPOINTS, TRACKED_PIDS_MAP,
};

/// The loaded kernel program and its maps (spec 0014 D3, D9).
///
/// Dropping it detaches the tracepoints and frees the maps.
pub struct Probes {
    // Held so the programs and their links stay attached.
    _ebpf: Ebpf,
    tracked: HashMap<MapData, u32, u32>,
    events: RingBuf<MapData>,
    dropped: PerCpuArray<MapData, u64>,
}

impl Probes {
    /// Loads the committed object, attaches the three sched tracepoints and opens the maps.
    ///
    /// # Errors
    ///
    /// [`EbpfError::PermissionDenied`] or [`EbpfError::BtfMissing`] when the host probe
    /// says the object cannot load here. Otherwise the error for the step that failed.
    pub fn load() -> Result<Self, EbpfError> {
        let report = probe::probe();
        if !report.can_load() {
            if !report.cap_bpf || !report.cap_perfmon {
                return Err(EbpfError::PermissionDenied);
            }
            if !report.btf_present {
                return Err(EbpfError::BtfMissing);
            }
        }

        let mut ebpf = Ebpf::load(BPF_OBJECT)?;

        for (name, category, tracepoint) in TRACEPOINTS {
            let program: &mut TracePoint = ebpf
                .program_mut(name)
                .ok_or(EbpfError::ProgramMissing { name })?
                .try_into()
                .map_err(|_| EbpfError::ProgramMissing { name })?;
            program
                .load()
                .map_err(|source| EbpfError::ProgramLoadFailed { name, source })?;
            program.attach(category, tracepoint).map_err(|source| {
                EbpfError::AttachTracepointFailed {
                    name: attach_name(tracepoint),
                    source,
                }
            })?;
        }

        let tracked = HashMap::try_from(take(&mut ebpf, TRACKED_PIDS_MAP)?)
            .map_err(|source| map_err(TRACKED_PIDS_MAP, source))?;
        let events = RingBuf::try_from(take(&mut ebpf, AUDIT_EVENTS_MAP)?)
            .map_err(|source| map_err(AUDIT_EVENTS_MAP, source))?;
        let dropped = PerCpuArray::try_from(take(&mut ebpf, DROPPED_EVENTS_MAP)?)
            .map_err(|source| map_err(DROPPED_EVENTS_MAP, source))?;

        Ok(Self {
            _ebpf: ebpf,
            tracked,
            events,
            dropped,
        })
    }
}

impl std::fmt::Debug for Probes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Probes").finish_non_exhaustive()
    }
}

/// The `category/name` label for an attach error. The set is fixed by `TRACEPOINTS`.
fn attach_name(tracepoint: &str) -> &'static str {
    match tracepoint {
        "sched_process_exec" => "sched/sched_process_exec",
        "sched_process_fork" => "sched/sched_process_fork",
        _ => "sched/sched_process_exit",
    }
}

fn take(ebpf: &mut Ebpf, name: &'static str) -> Result<aya::maps::Map, EbpfError> {
    ebpf.take_map(name).ok_or(EbpfError::MapMissing { name })
}

fn map_err(name: &'static str, source: MapError) -> EbpfError {
    EbpfError::Map { name, source }
}

impl ProbeHandle for Probes {
    fn track(&mut self, pid: u32, ppid: u32) -> Result<(), EbpfError> {
        self.tracked
            .insert(pid, ppid, 0)
            .map_err(|e| map_err(TRACKED_PIDS_MAP, e))
    }

    fn untrack(&mut self, pid: u32) -> Result<(), EbpfError> {
        match self.tracked.remove(&pid) {
            // An absent pid is already untracked.
            Ok(()) | Err(MapError::KeyNotFound) => Ok(()),
            Err(e) => Err(map_err(TRACKED_PIDS_MAP, e)),
        }
    }

    fn is_tracked(&self, pid: u32) -> Result<bool, EbpfError> {
        match self.tracked.get(&pid, 0) {
            Ok(_) => Ok(true),
            Err(MapError::KeyNotFound) => Ok(false),
            Err(e) => Err(map_err(TRACKED_PIDS_MAP, e)),
        }
    }

    fn dropped_events(&self) -> Result<u64, EbpfError> {
        let values = self
            .dropped
            .get(&0, 0)
            .map_err(|e| map_err(DROPPED_EVENTS_MAP, e))?;
        Ok(values.iter().sum())
    }

    fn next_event(&mut self) -> Option<ExecEvent> {
        // A short item is not an ExecEvent. Skip it and read on.
        while let Some(item) = self.events.next() {
            if let Some(event) = ExecEvent::from_bytes(&item) {
                return Some(event);
            }
        }
        None
    }

    fn event_fd(&self) -> BorrowedFd<'_> {
        self.events.as_fd()
    }
}
