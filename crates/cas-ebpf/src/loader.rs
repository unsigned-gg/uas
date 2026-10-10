//! The aya loader: loads `BPF_OBJECT`, attaches the tracepoints and owns the maps.
//!
//! Needs `CAP_BPF` and `CAP_PERFMON`. [`Probes::load`] runs the host probe first
//! and returns a typed error instead of a raw `EPERM` when the host cannot load.
//!
//! Pid namespaces (spec 0014 T13): every pid this API takes or returns is a pid
//! of this process's pid namespace, with one exception:
//! [`Probes::is_tracked_global`]. The kernel program keys its tracked map by
//! global task id and translates through `PIDNS_CONFIG`, which `load` fills
//! with the `(dev, ino)` of `/proc/self/ns/pid`. A `track` call is a request
//! that the kernel honours at that process's next fork or exec.

use std::os::fd::{AsFd, BorrowedFd};

use aya::{
    maps::{Array, HashMap, MapData, MapError, PerCpuArray, RingBuf},
    programs::TracePoint,
    Ebpf,
};

use crate::{
    error::EbpfError, handle::ProbeHandle, probe, tracefs, wire::ExecEvent, AUDIT_EVENTS_MAP,
    BPF_OBJECT, DROPPED_EVENTS_MAP, DROPPED_RING_SLOT, DROPPED_TRACK_SLOT, NS_TO_GLOBAL_MAP,
    PIDNS_CONFIG_MAP, TP_OFFSETS_MAP, TRACEPOINTS, TRACKED_PIDS_MAP, TRACK_REQUESTS_MAP,
};

/// The pid namespace file the kernel helper matches against.
const PIDNS_PATH: &str = "/proc/self/ns/pid";

/// The loaded kernel program and its maps (spec 0014 D3, D9).
///
/// Dropping it detaches the tracepoints and frees the maps.
pub struct Probes {
    // Held so the programs and their links stay attached.
    _ebpf: Ebpf,
    requests: HashMap<MapData, u32, u32>,
    tracked: HashMap<MapData, u32, u32>,
    ns_to_global: HashMap<MapData, u32, u32>,
    events: RingBuf<MapData>,
    dropped: PerCpuArray<MapData, u64>,
    tp_offsets: Array<MapData, u32>,
    pidns: [u64; 2],
}

impl Probes {
    /// Loads the committed object, attaches the three sched tracepoints, opens
    /// the maps and records this process's pid namespace in the kernel.
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

        // Read the layouts and fill the slots before any attach, so a bad
        // format file leaves nothing attached (spec 0014 D13).
        let offsets = tracefs::read().map_err(|failure| EbpfError::TracepointFormat {
            tracepoint: failure.tracepoint,
            source: failure.source,
        })?;
        let mut tp_offsets: Array<MapData, u32> = Array::try_from(take(&mut ebpf, TP_OFFSETS_MAP)?)
            .map_err(|source| map_err(TP_OFFSETS_MAP, source))?;
        for (slot, value) in (0u32..).zip(offsets.slots()) {
            tp_offsets
                .set(slot, value, 0)
                .map_err(|source| map_err(TP_OFFSETS_MAP, source))?;
        }

        for (name, category, tracepoint, label) in TRACEPOINTS {
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
                    name: label,
                    source,
                }
            })?;
        }

        let mut config: Array<MapData, [u64; 2]> =
            Array::try_from(take(&mut ebpf, PIDNS_CONFIG_MAP)?)
                .map_err(|source| map_err(PIDNS_CONFIG_MAP, source))?;
        let pidns = pid_namespace()?;
        config
            .set(0, pidns, 0)
            .map_err(|source| map_err(PIDNS_CONFIG_MAP, source))?;

        let requests = HashMap::try_from(take(&mut ebpf, TRACK_REQUESTS_MAP)?)
            .map_err(|source| map_err(TRACK_REQUESTS_MAP, source))?;
        let tracked = HashMap::try_from(take(&mut ebpf, TRACKED_PIDS_MAP)?)
            .map_err(|source| map_err(TRACKED_PIDS_MAP, source))?;
        let ns_to_global = HashMap::try_from(take(&mut ebpf, NS_TO_GLOBAL_MAP)?)
            .map_err(|source| map_err(NS_TO_GLOBAL_MAP, source))?;
        let events = RingBuf::try_from(take(&mut ebpf, AUDIT_EVENTS_MAP)?)
            .map_err(|source| map_err(AUDIT_EVENTS_MAP, source))?;
        let dropped = PerCpuArray::try_from(take(&mut ebpf, DROPPED_EVENTS_MAP)?)
            .map_err(|source| map_err(DROPPED_EVENTS_MAP, source))?;

        Ok(Self {
            _ebpf: ebpf,
            requests,
            tracked,
            ns_to_global,
            events,
            dropped,
            tp_offsets,
            pidns,
        })
    }

    /// The five field offsets in the kernel map, in slot order (spec 0014 D13).
    /// Read back from the kernel, not from the parsed copy.
    ///
    /// # Errors
    ///
    /// [`EbpfError::Map`] when a slot cannot be read.
    pub fn tp_offsets(&self) -> Result<[u32; 5], EbpfError> {
        let mut out = [0u32; 5];
        for (slot, value) in (0u32..).zip(out.iter_mut()) {
            *value = self
                .tp_offsets
                .get(&slot, 0)
                .map_err(|e| map_err(TP_OFFSETS_MAP, e))?;
        }
        Ok(out)
    }

    /// The global task ids in `TRACKED_PIDS`. Used to check the keys against `pid_max`.
    ///
    /// # Errors
    ///
    /// [`EbpfError::Map`] when the map cannot be iterated.
    pub fn tracked_globals(&self) -> Result<Vec<u32>, EbpfError> {
        self.tracked
            .keys()
            .collect::<Result<Vec<u32>, MapError>>()
            .map_err(|e| map_err(TRACKED_PIDS_MAP, e))
    }

    /// The `[dev, ino]` of this process's pid namespace, as written to the kernel.
    pub fn pid_namespace(&self) -> [u64; 2] {
        self.pidns
    }

    /// The global task id the kernel knows `ns_pid` by, once the task has
    /// exec'd or forked under tracking. `None` before that, or when untracked.
    pub fn global_pid(&self, ns_pid: u32) -> Result<Option<u32>, EbpfError> {
        match self.ns_to_global.get(&ns_pid, 0) {
            Ok(global) => Ok(Some(global)),
            Err(MapError::KeyNotFound) => Ok(None),
            Err(e) => Err(map_err(NS_TO_GLOBAL_MAP, e)),
        }
    }

    /// `true` when the global task id `gpid` is in `TRACKED_PIDS`.
    ///
    /// This is the one exception to "every pid is a namespace pid": `gpid` is
    /// the pid the kernel knows, which is the `pid` field of an [`ExecEvent`].
    /// It sees a task that the namespace-keyed lookups cannot, such as a task
    /// in a nested pid namespace (spec 0014 D12) or a forked child that has
    /// not exec'd yet.
    pub fn is_tracked_global(&self, gpid: u32) -> Result<bool, EbpfError> {
        match self.tracked.get(&gpid, 0) {
            Ok(_) => Ok(true),
            Err(MapError::KeyNotFound) => Ok(false),
            Err(e) => Err(map_err(TRACKED_PIDS_MAP, e)),
        }
    }

    /// Fork inserts the kernel refused because `TRACKED_PIDS` was full, summed over cpus.
    /// A non-zero value means some child of a tracked worker escaped tracking.
    pub fn tracking_failures(&self) -> Result<u64, EbpfError> {
        self.counter(DROPPED_TRACK_SLOT)
    }

    fn counter(&self, slot: u32) -> Result<u64, EbpfError> {
        let values = self
            .dropped
            .get(&slot, 0)
            .map_err(|e| map_err(DROPPED_EVENTS_MAP, e))?;
        Ok(values.iter().sum())
    }
}

impl std::fmt::Debug for Probes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Probes")
            .field("pidns", &self.pidns)
            .finish_non_exhaustive()
    }
}

/// `[dev, ino]` of `/proc/self/ns/pid`: what `bpf_get_ns_current_pid_tgid` matches.
fn pid_namespace() -> Result<[u64; 2], EbpfError> {
    let st = rustix::fs::stat(PIDNS_PATH).map_err(std::io::Error::from)?;
    Ok([st.st_dev, st.st_ino])
}

fn take(ebpf: &mut Ebpf, name: &'static str) -> Result<aya::maps::Map, EbpfError> {
    ebpf.take_map(name).ok_or(EbpfError::MapMissing { name })
}

fn map_err(name: &'static str, source: MapError) -> EbpfError {
    EbpfError::Map { name, source }
}

impl ProbeHandle for Probes {
    fn track(&mut self, pid: u32, ppid: u32) -> Result<(), EbpfError> {
        self.requests
            .insert(pid, ppid, 0)
            .map_err(|e| map_err(TRACK_REQUESTS_MAP, e))
    }

    fn untrack(&mut self, pid: u32) -> Result<(), EbpfError> {
        match self.requests.remove(&pid) {
            Ok(()) | Err(MapError::KeyNotFound) => {}
            Err(e) => return Err(map_err(TRACK_REQUESTS_MAP, e)),
        }
        if let Some(global) = self.global_pid(pid)? {
            match self.tracked.remove(&global) {
                Ok(()) | Err(MapError::KeyNotFound) => {}
                Err(e) => return Err(map_err(TRACKED_PIDS_MAP, e)),
            }
            match self.ns_to_global.remove(&pid) {
                Ok(()) | Err(MapError::KeyNotFound) => {}
                Err(e) => return Err(map_err(NS_TO_GLOBAL_MAP, e)),
            }
        }
        Ok(())
    }

    fn is_tracked(&self, pid: u32) -> Result<bool, EbpfError> {
        match self.requests.get(&pid, 0) {
            Ok(_) => return Ok(true),
            Err(MapError::KeyNotFound) => {}
            Err(e) => return Err(map_err(TRACK_REQUESTS_MAP, e)),
        }
        let Some(global) = self.global_pid(pid)? else {
            return Ok(false);
        };
        match self.tracked.get(&global, 0) {
            Ok(_) => Ok(true),
            Err(MapError::KeyNotFound) => Ok(false),
            Err(e) => Err(map_err(TRACKED_PIDS_MAP, e)),
        }
    }

    fn dropped_events(&self) -> Result<u64, EbpfError> {
        self.counter(DROPPED_RING_SLOT)
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
