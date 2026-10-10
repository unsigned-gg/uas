//! Kernel-side eBPF tracepoint probes for priority clamping and execution audit.
//!
//! Declares `Dual MIT/GPL` in the ELF license section to satisfy Linux kernel
//! BPF verifier requirements for GPL-compatible helper functions.
//!
//! Pid namespaces (spec 0014 T13): the tracepoint fields and
//! `bpf_get_current_pid_tgid` carry global pids, while the daemon and
//! `cas-pool` see pids of their own pid namespace (on ceres, systemd's). The
//! daemon writes its namespace's `(dev, ino)` into `PIDNS_CONFIG` at load, and
//! every probe translates `current` with `bpf_get_ns_current_pid_tgid`. A track
//! request is keyed by the namespace tgid and becomes a tracked global tgid at
//! that process's next fork or exec. `TRACKED_PIDS` is keyed by global task id;
//! `NS_TO_GLOBAL` lets userspace resolve a namespace pid it knows.

#![no_std]
#![no_main]
#![allow(linker_messages)]

use aya_ebpf::{
    helpers::generated::bpf_get_ns_current_pid_tgid,
    bindings::bpf_pidns_info,
    helpers::{
        bpf_get_current_comm, bpf_get_current_pid_tgid, bpf_get_current_uid_gid, bpf_ktime_get_ns,
        bpf_probe_read_kernel_str_bytes,
    },
    macros::{map, tracepoint},
    maps::{Array, HashMap, PerCpuArray, RingBuf},
    programs::TracePointContext,
    EbpfContext,
};

/// Process execution event record passed over the ring buffer to userspace.
///
/// 164 bytes of fields plus `ns_pid` makes 168 bytes under 8-byte `#[repr(C)]`
/// alignment. The userspace twin is `crates/cas-ebpf/src/wire.rs`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExecEvent {
    /// Monotonic kernel timestamp in nanoseconds.
    pub timestamp_ns: u64,
    /// Process ID (global tgid).
    pub pid: u32,
    /// Parent process ID as seen in the daemon's pid namespace.
    pub ppid: u32,
    /// User ID.
    pub uid: u32,
    /// Process command name.
    pub comm: [u8; 16],
    /// Executed binary path.
    pub filename: [u8; 128],
    /// Process ID as seen in the daemon's pid namespace.
    pub ns_pid: u32,
}

const _: () = assert!(core::mem::size_of::<ExecEvent>() == 168);
const _: () = assert!(core::mem::offset_of!(ExecEvent, timestamp_ns) == 0);
const _: () = assert!(core::mem::offset_of!(ExecEvent, pid) == 8);
const _: () = assert!(core::mem::offset_of!(ExecEvent, ppid) == 12);
const _: () = assert!(core::mem::offset_of!(ExecEvent, uid) == 16);
const _: () = assert!(core::mem::offset_of!(ExecEvent, comm) == 20);
const _: () = assert!(core::mem::offset_of!(ExecEvent, filename) == 36);
const _: () = assert!(core::mem::offset_of!(ExecEvent, ns_pid) == 164);

/// The daemon's pid namespace, as `[dev, ino]` of `/proc/self/ns/pid`. `[0, 0]`
/// means no translation: global pids are used as namespace pids.
#[map]
static PIDNS_CONFIG: Array<[u64; 2]> = Array::with_max_entries(1, 0);

/// Track requests from userspace: namespace tgid to namespace parent tgid.
#[map]
static TRACK_REQUESTS: HashMap<u32, u32> = HashMap::with_max_entries(1024, 0);

/// Tracked tasks: global task id to the parent's namespace tgid.
#[map]
static TRACKED_PIDS: HashMap<u32, u32> = HashMap::with_max_entries(1024, 0);

/// Namespace pid to global pid, for tracked tasks that have exec'd or forked.
#[map]
static NS_TO_GLOBAL: HashMap<u32, u32> = HashMap::with_max_entries(1024, 0);

/// Ring buffer for streaming audit execution events to userspace.
#[map]
static AUDIT_EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

/// Per-cpu counters. Slot 0: events lost because the ring buffer was full.
/// Slot 1: fork inserts refused because `TRACKED_PIDS` was full.
#[map]
static DROPPED_EVENTS: PerCpuArray<u64> = PerCpuArray::with_max_entries(2, 0);

/// Slot of the ring-buffer drop counter.
const DROPPED_RING: u32 = 0;
/// Slot of the tracking-insert failure counter.
const DROPPED_TRACK: u32 = 1;

/// Offset of the `__data_loc` filename field in `sched_process_exec`.
const EXEC_FILENAME_LOC: usize = 8;
/// Offset of `child_pid` in `sched_process_fork` (measured on kernel 6.6.87.2).
const FORK_CHILD_PID: usize = 44;

/// Adds one to a `DROPPED_EVENTS` slot. Per-cpu, so no atomic is needed.
fn bump(slot: u32) {
    if let Some(counter) = DROPPED_EVENTS.get_ptr_mut(slot) {
        // SAFETY: the pointer comes from a per-cpu map slot valid for this call.
        unsafe { *counter += 1 };
    }
}

/// The ids of `current`: global, and as seen in the configured namespace.
struct Ids {
    gpid: u32,
    gtgid: u32,
    nspid: u32,
    nstgid: u32,
}

/// Global ids always; namespace ids when `current` is visible in the
/// configured namespace. `None` when it is not (a task outside the daemon's
/// namespace), in which case it cannot be ours.
fn current_ids() -> Option<Ids> {
    let g = bpf_get_current_pid_tgid();
    let gpid = g as u32;
    let gtgid = (g >> 32) as u32;
    let cfg = PIDNS_CONFIG.get(0)?;
    if cfg[1] == 0 {
        return Some(Ids {
            gpid,
            gtgid,
            nspid: gpid,
            nstgid: gtgid,
        });
    }
    let mut info = bpf_pidns_info { pid: 0, tgid: 0 };
    // SAFETY: the helper writes at most `size` bytes into `info`, which is a
    // valid, properly sized stack buffer for the whole call.
    let rc = unsafe {
        bpf_get_ns_current_pid_tgid(
            cfg[0],
            cfg[1],
            &mut info as *mut bpf_pidns_info,
            core::mem::size_of::<bpf_pidns_info>() as u32,
        )
    };
    if rc != 0 {
        return None;
    }
    Some(Ids {
        gpid,
        gtgid,
        nspid: info.pid,
        nstgid: info.tgid,
    })
}

/// `true` when the current process is tracked, promoting a pending track
/// request for it on the way.
fn promote(ids: &Ids) -> bool {
    // SAFETY: only the presence of the key is used.
    if unsafe { TRACKED_PIDS.get(&ids.gtgid) }.is_some() {
        return true;
    }
    // SAFETY: the value is copied out before any map update.
    let Some(ppid) = (unsafe { TRACK_REQUESTS.get(&ids.nstgid) }).copied() else {
        return false;
    };
    if TRACKED_PIDS.insert(&ids.gtgid, &ppid, 0).is_err() {
        bump(DROPPED_TRACK);
        return false;
    }
    let _ = NS_TO_GLOBAL.insert(&ids.nstgid, &ids.gtgid, 0);
    true
}

/// Tracepoint probe for `sched/sched_process_exec`.
#[tracepoint(category = "sched", name = "sched_process_exec")]
pub fn cas_sched_process_exec(ctx: TracePointContext) -> u32 {
    try_sched_process_exec(&ctx);
    0
}

fn try_sched_process_exec(ctx: &TracePointContext) {
    let Some(ids) = current_ids() else {
        return;
    };
    if !promote(&ids) {
        return;
    }
    // After exec the task is its group leader: pid == tgid.
    // SAFETY: the value is copied out before any map update.
    let ppid = (unsafe { TRACKED_PIDS.get(&ids.gtgid) })
        .copied()
        .unwrap_or(0);
    let _ = NS_TO_GLOBAL.insert(&ids.nstgid, &ids.gtgid, 0);
    let Some(mut entry) = AUDIT_EVENTS.reserve::<ExecEvent>(0) else {
        bump(DROPPED_RING);
        return;
    };
    let mut filename = [0u8; 128];
    // SAFETY: read_at is a checked kernel read of the __data_loc word.
    if let Ok(loc) = unsafe { ctx.read_at::<u32>(EXEC_FILENAME_LOC) } {
        let off = (loc & 0xffff) as usize;
        // SAFETY: the string lives inside the tracepoint record; the helper bounds the copy.
        let src = unsafe { ctx.as_ptr().cast::<u8>().add(off) };
        // SAFETY: a failed read leaves the zeroed buffer unchanged.
        let _ = unsafe { bpf_probe_read_kernel_str_bytes(src, &mut filename) };
    }
    // SAFETY: the helper has no memory preconditions.
    let timestamp_ns = unsafe { bpf_ktime_get_ns() };
    let event = ExecEvent {
        timestamp_ns,
        pid: ids.gtgid,
        ppid,
        uid: bpf_get_current_uid_gid() as u32,
        comm: bpf_get_current_comm().unwrap_or([0u8; 16]),
        filename,
        ns_pid: ids.nstgid,
    };
    entry.write(event);
    entry.submit(0);
}

/// Tracepoint probe for `sched/sched_process_fork`.
#[tracepoint(category = "sched", name = "sched_process_fork")]
pub fn cas_sched_process_fork(ctx: TracePointContext) -> u32 {
    try_sched_process_fork(&ctx);
    0
}

fn try_sched_process_fork(ctx: &TracePointContext) {
    // `current` is the parent at this tracepoint.
    let Some(ids) = current_ids() else {
        return;
    };
    if !promote(&ids) {
        return;
    }
    // SAFETY: read_at copies from the tracepoint record with a checked kernel read.
    let Ok(child_pid) = (unsafe { ctx.read_at::<i32>(FORK_CHILD_PID) }) else {
        return;
    };
    // The child is keyed by its global task id (a thread clone inserts its
    // tid; exit removes it). The value is the parent's namespace tgid. A full
    // map refuses the insert, and the child escapes tracking: count it.
    if TRACKED_PIDS
        .insert(&(child_pid as u32), &ids.nstgid, 0)
        .is_err()
    {
        bump(DROPPED_TRACK);
    }
}

/// Tracepoint probe for `sched/sched_process_exit`.
#[tracepoint(category = "sched", name = "sched_process_exit")]
pub fn cas_sched_process_exit(_ctx: TracePointContext) -> u32 {
    try_sched_process_exit();
    0
}

fn try_sched_process_exit() {
    // `current` is the exiting task. Evict by global id even when the task is
    // outside the namespace (then only the global key can exist).
    let g = bpf_get_current_pid_tgid();
    let gpid = g as u32;
    let gtgid = (g >> 32) as u32;
    let _ = TRACKED_PIDS.remove(&gpid);
    if let Some(ids) = current_ids() {
        let _ = NS_TO_GLOBAL.remove(&ids.nspid);
        if gpid == gtgid {
            let _ = TRACK_REQUESTS.remove(&ids.nstgid);
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

/// Declares Dual MIT/GPL license for the Linux kernel BPF verifier.
#[link_section = "license"]
#[no_mangle]
static LICENSE: [u8; 13] = *b"Dual MIT/GPL\0";
