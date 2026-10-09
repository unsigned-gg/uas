//! Kernel-side eBPF tracepoint probes for priority clamping and execution audit.
//!
//! Declares `Dual MIT/GPL` in the ELF license section to satisfy Linux kernel
//! BPF verifier requirements for GPL-compatible helper functions.

#![no_std]
#![no_main]
#![allow(linker_messages)]

use aya_ebpf::{
    macros::{map, tracepoint},
    maps::{HashMap, RingBuf},
    programs::TracePointContext,
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
    /// Alignment padding to 8-byte boundary.
    pub _pad: u32,
}

/// Map of tracked process IDs to inherit priority clamping.
#[map]
static TRACKED_PIDS: HashMap<u32, u8> = HashMap::with_max_entries(1024, 0);

/// Ring buffer for streaming audit execution events to userspace.
#[map]
static AUDIT_EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

/// Tracepoint probe for `sched/sched_process_exec`.
#[tracepoint]
pub fn cas_sched_process_exec(ctx: TracePointContext) -> u32 {
    match try_sched_process_exec(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_sched_process_exec(_ctx: TracePointContext) -> Result<u32, u32> {
    Ok(0)
}

/// Tracepoint probe for `sched/sched_process_fork`.
#[tracepoint]
pub fn cas_sched_process_fork(ctx: TracePointContext) -> u32 {
    match try_sched_process_fork(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_sched_process_fork(_ctx: TracePointContext) -> Result<u32, u32> {
    Ok(0)
}

/// Tracepoint probe for `sched/sched_process_exit`.
#[tracepoint]
pub fn cas_sched_process_exit(ctx: TracePointContext) -> u32 {
    match try_sched_process_exit(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_sched_process_exit(_ctx: TracePointContext) -> Result<u32, u32> {
    Ok(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

/// Declares Dual MIT/GPL license for the Linux kernel BPF verifier.
#[link_section = "license"]
#[no_mangle]
static LICENSE: [u8; 13] = *b"Dual MIT/GPL\0";
