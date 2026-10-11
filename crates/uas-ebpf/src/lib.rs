// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Aya eBPF tracepoint programs: build-priority clamping and audit telemetry.
//!
//! Spec 0014 (CER-2719, CER-2720). The kernel program lives in `ebpf/` and is
//! committed as `ebpf/uas-ebpf.o`; `build.rs` checks its hash. This crate holds
//! the userspace side:
//!
//! - [`wire`]: the 168-byte `ExecEvent` record shared with the kernel program.
//! - [`audit`]: the `run_hash` transcript digest and the execution receipt.
//! - [`clamp`]: `PRIO_PGRP` clamping for worker process groups (rustix, no unsafe).
//! - [`probe`]: a host probe that says whether the kernel program can load here.
//! - [`handle`]: the [`handle::ProbeHandle`] trait the daemon drives, plus a fake.
//! - [`loader`]: the aya loader that implements the trait over the live maps.
//! - [`protocol`], [`daemon`], [`client`]: the `uas-ebpfd` socket protocol, server and client.
#![deny(unsafe_code)]

pub mod audit;
pub mod clamp;
pub mod client;
pub mod daemon;
pub mod error;
pub mod handle;
pub mod loader;
pub mod probe;
pub mod protocol;
pub mod tracefs;
pub mod wire;

pub use audit::{AuditReceipt, RunHash};
pub use error::EbpfError;
pub use wire::{ExecEvent, ExecRecord, EXEC_EVENT_SIZE};

/// The committed kernel program (spec 0014 D3). `build.rs` verified its SHA-256
/// against `ebpf/uas-ebpf.o.sha256` before this crate compiled.
pub const BPF_OBJECT: &[u8] = include_bytes!("../ebpf/uas-ebpf.o");

/// Name of the one-entry array holding the daemon's pid namespace as `[dev, ino]`.
pub const PIDNS_CONFIG_MAP: &str = "PIDNS_CONFIG";
/// Name of the BPF hash map of track requests (key namespace tgid, value namespace ppid).
pub const TRACK_REQUESTS_MAP: &str = "TRACK_REQUESTS";
/// Name of the BPF hash map of tracked tasks (key global task id, value parent's namespace tgid).
pub const TRACKED_PIDS_MAP: &str = "TRACKED_PIDS";
/// Name of the BPF hash map from namespace pid to global pid for tracked tasks.
pub const NS_TO_GLOBAL_MAP: &str = "NS_TO_GLOBAL";
/// Name of the five-slot array of tracepoint field offsets the loader writes
/// before attach (spec 0014 D13).
pub const TP_OFFSETS_MAP: &str = "TP_OFFSETS";
/// Name of the BPF ring buffer that carries [`ExecEvent`] records.
pub const AUDIT_EVENTS_MAP: &str = "AUDIT_EVENTS";
/// Name of the per-cpu counter array. Slot [`DROPPED_RING_SLOT`] counts records
/// the kernel could not reserve; slot [`DROPPED_TRACK_SLOT`] counts fork inserts
/// refused because `TRACKED_PIDS` was full.
pub const DROPPED_EVENTS_MAP: &str = "DROPPED_EVENTS";
/// See [`DROPPED_EVENTS_MAP`].
pub const DROPPED_RING_SLOT: u32 = 0;
/// See [`DROPPED_EVENTS_MAP`].
pub const DROPPED_TRACK_SLOT: u32 = 1;
/// Capacity of `TRACKED_PIDS` (spec 0014 D9).
pub const TRACKED_PIDS_CAPACITY: u32 = 1024;
/// Size of `AUDIT_EVENTS` in bytes (spec 0014 D6).
pub const AUDIT_RING_BYTES: u32 = 256 * 1024;

/// Tracepoint programs in the object, as `(program name, category, tracepoint, label)`.
/// The label is the `category/name` form used in errors.
pub const TRACEPOINTS: [(&str, &str, &str, &str); 3] = [
    (
        "uas_sched_process_exec",
        "sched",
        "sched_process_exec",
        "sched/sched_process_exec",
    ),
    (
        "uas_sched_process_fork",
        "sched",
        "sched_process_fork",
        "sched/sched_process_fork",
    ),
    (
        "uas_sched_process_exit",
        "sched",
        "sched_process_exit",
        "sched/sched_process_exit",
    ),
];
