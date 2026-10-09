//! Kernel-side eBPF tracepoint probes for priority clamping and execution audit.
//!
//! Declares `Dual MIT/GPL` in the ELF license section to satisfy Linux kernel
//! BPF verifier requirements for GPL-compatible helper functions.

#![no_std]
#![no_main]
#![allow(linker_messages)]

use aya_ebpf::{
    helpers::{
        bpf_get_current_comm, bpf_get_current_pid_tgid, bpf_get_current_uid_gid, bpf_ktime_get_ns,
        bpf_probe_read_kernel_str_bytes,
    },
    macros::{map, tracepoint},
    maps::{HashMap, PerCpuArray, RingBuf},
    programs::TracePointContext,
    EbpfContext,
};

/// Process execution event record passed over the ring buffer to userspace.
///
/// 164 bytes of fields padded to 168 bytes under 8-byte `#[repr(C)]` alignment.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExecEvent {
    /// Monotonic kernel timestamp in nanoseconds.
    pub timestamp_ns: u64,
    /// Process ID.
    pub pid: u32,
    /// Parent process ID.
    pub ppid: u32,
    /// User ID.
    pub uid: u32,
    /// Process command name.
    pub comm: [u8; 16],
    /// Executed binary path.
    pub filename: [u8; 128],
    /// Explicit padding to 168 bytes.
    pub _pad: u32,
}

const _: () = assert!(core::mem::size_of::<ExecEvent>() == 168);

/// Tracked task ids. Key is a task id, value is the parent tgid.
#[map]
static TRACKED_PIDS: HashMap<u32, u32> = HashMap::with_max_entries(1024, 0);

/// Ring buffer for streaming audit execution events to userspace.
#[map]
static AUDIT_EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

/// Count of events lost because the ring buffer was full.
#[map]
static DROPPED_EVENTS: PerCpuArray<u64> = PerCpuArray::with_max_entries(1, 0);

/// Offset of the `__data_loc` filename field in `sched_process_exec`.
const EXEC_FILENAME_LOC: usize = 8;
/// Offset of `pid` in `sched_process_exec`.
const EXEC_PID: usize = 12;
/// Offset of `child_pid` in `sched_process_fork` (measured on kernel 6.6.87.2).
const FORK_CHILD_PID: usize = 44;
/// Offset of `pid` in `sched_process_exit` (measured on kernel 6.6.87.2).
const EXIT_PID: usize = 24;

/// Tracepoint probe for `sched/sched_process_exec`.
#[tracepoint(category = "sched", name = "sched_process_exec")]
pub fn cas_sched_process_exec(ctx: TracePointContext) -> u32 {
    try_sched_process_exec(&ctx);
    0
}

fn try_sched_process_exec(ctx: &TracePointContext) {
    // SAFETY: read_at copies from the tracepoint record with a checked kernel read.
    let Ok(pid) = (unsafe { ctx.read_at::<i32>(EXEC_PID) }) else {
        return;
    };
    let pid = pid as u32;
    // SAFETY: the reference is used immediately, before any map update can free it.
    let ppid = match unsafe { TRACKED_PIDS.get(&pid) } {
        Some(v) => *v,
        None => return,
    };
    let Some(mut entry) = AUDIT_EVENTS.reserve::<ExecEvent>(0) else {
        if let Some(slot) = DROPPED_EVENTS.get_ptr_mut(0) {
            // SAFETY: the pointer comes from a per-cpu map slot valid for this call.
            unsafe { *slot += 1 };
        }
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
        pid,
        ppid,
        uid: bpf_get_current_uid_gid() as u32,
        comm: bpf_get_current_comm().unwrap_or([0u8; 16]),
        filename,
        _pad: 0,
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
    let parent_tgid = (bpf_get_current_pid_tgid() >> 32) as u32;
    // SAFETY: only the presence of the key is used.
    if unsafe { TRACKED_PIDS.get(&parent_tgid) }.is_none() {
        return;
    }
    // SAFETY: read_at copies from the tracepoint record with a checked kernel read.
    let Ok(child_pid) = (unsafe { ctx.read_at::<i32>(FORK_CHILD_PID) }) else {
        return;
    };
    let _ = TRACKED_PIDS.insert(&(child_pid as u32), &parent_tgid, 0);
}

/// Tracepoint probe for `sched/sched_process_exit`.
#[tracepoint(category = "sched", name = "sched_process_exit")]
pub fn cas_sched_process_exit(ctx: TracePointContext) -> u32 {
    try_sched_process_exit(&ctx);
    0
}

fn try_sched_process_exit(ctx: &TracePointContext) {
    // SAFETY: read_at copies from the tracepoint record with a checked kernel read.
    if let Ok(pid) = unsafe { ctx.read_at::<i32>(EXIT_PID) } {
        let _ = TRACKED_PIDS.remove(&(pid as u32));
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
