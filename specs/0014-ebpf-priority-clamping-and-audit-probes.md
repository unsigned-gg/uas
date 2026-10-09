---
status: in-progress
created: 2026-10-09
tags: [ebpf, aya, tracepoint, autonice, audit, phase-2]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0004-dependency-and-licence-policy, 0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation]
---

# 0014-ebpf-priority-clamping-and-audit-probes

## Overview

Host `ceres` executes many concurrent agent tasks. Heavy compiler and test
workloads must not starve interactive developer shells or Cortex mesh daemons.
Spec 0001 invariant 4.2 requires host build throttling. The Failure Matrix
specifies clamping build process trees to `nice 19`.

This spec defines eBPF tracepoint instrumentation and priority clamping in `cas-ebpf`:
- **CER-2719**: Worker process priority clamping via process groups (`PRIO_PGRP`)
  and eBPF anti-escape monitoring (`sched_process_exec`, `sched_process_fork`).
- **CER-2720**: Kernel execution audit ring buffer (`ExecEvent` on
  `sched_process_exec` and `sched_process_exit`) and SHA-256 `run_hash`
  transcript digest logger. Per operator ruling, `openat` and `connect`
  are excluded from Phase 2.
- **Privileged Daemon Architecture**: A root systemd service (`cas-ebpfd`)
  loads BPF programs, owns maps, and serves unprivileged `cas-pool` clients
  over a Unix Domain Socket (UDS).

Parent tickets: CER-2682, CER-2683.

Facts, measured 2026-10-09 on ceres (kernel `6.6.87.2-microsoft-standard-WSL2`):

- **T1: Toolchain Installation [verified]:** `bpf-linker 0.11.1` is installed via
  `mise use -g github:aya-rs/bpf-linker@v0.11.1` (musl prebuilt binary with SLSA
  provenance, binary sha256 `c974140662f783c37bf0267436409caa8f4708ec10b558d9d1c594385489fe2f`).
  Standard `cargo install bpf-linker` fails on ceres because system LLVM 23 and
  `llvm-config` are absent.
- **T2: Toolchain Inventory [verified]:** `rustup toolchain list` contains `stable`
  (default), `nightly`, and `1.94.1` (pinned monorepo toolchain).
- **T3: Nightly rust-src Component [verified]:** Nightly `rust-src` was installed via
  `rustup component add rust-src --toolchain nightly`. Nightly version is
  `1.101.0-nightly (ea137335b 2026-10-05)`.
- **T4: Bytecode Compilation [verified]:** Subcrate `crates/cas-ebpf/ebpf` compiles
  cleanly under nightly targeting `bpfel-unknown-none` with `-Z build-std=core` into
  a 1.4 KB relocatable ELF (`cas-ebpf.o`). Sections: `tracepoint`, `license`, `maps`.
  No `.BTF` section is required for tracepoints.
- **T5: Subcrate Manifests [verified]:** Subcrate contains `rust-toolchain.toml` (nightly),
  `.cargo/config.toml` (target `bpfel-unknown-none`, linker `bpf-linker`), and an
  independent `[workspace]` root in `Cargo.toml`.
- **T6: Autogroup Configuration [verified]:** `CONFIG_SCHED_AUTOGROUP` is NOT set on
  the WSL2 kernel (`/proc/self/autogroup` ENOENT). Autogroup nice cannot be used.
- **T7: Tracepoint Layouts [verified]:**
  - `/sys/kernel/tracing/events/sched/sched_process_exec/format`: `filename` is
    `__data_loc char[]` at offset 8 (size 4), `pid` at 12, `old_pid` at 16. Reading
    `filename`: low 16 bits = offset, high 16 bits = length; read via
    `bpf_probe_read_kernel_str_bytes`. At exec tracepoint, `comm` already contains
    the new basename (15 chars + null); `bpf_get_current_comm` matches without
    in-kernel path parsing.
  - `/sys/kernel/tracing/events/sched/sched_process_fork/format`: `parent_comm[16]` at
    offset 8, `parent_pid` at 24, `child_comm[16]` at 28, `child_pid` at 44. Corrected
    2026-10-09 by lane Claude from the format file; an earlier draft said 16 and 20.
    Both pids are task ids: a thread clone reports the new tid. The probe takes the
    parent tgid from `bpf_get_current_pid_tgid() >> 32`, not from the field.
  - `/sys/kernel/tracing/events/sched/sched_process_exit/format`: `comm[16]` at 8,
    `pid` at 24, `prio` at 28. Corrected the same way; an earlier draft said 12.
    The pid is the exiting task id. For a main thread it equals the tgid.
- **T8: Priority Clamping Behavior on Linux [verified]:** `setpriority(PRIO_PROCESS, tgid, 19)`
  modifies only the targeted main thread. A 4-thread process after
  `renice -n 19 -p` yielded `{main: 19, t1: 0, t2: 0, t3: 0}` (python probe, ceres,
  2026-10-09), and worker pools (rustc and cargo) spawn threads immediately.
  `setpriority(PRIO_PGRP, pgid, 19)` sets every thread of every process in the group
  and all future children to nice 19. Both halves are re-proven on every CI run by
  `crates/cas-ebpf/tests/clamp.rs`, which spawns a 4-task child and reads each
  tid's nice from `/proc/<pid>/task/*/stat`.
- **T9: Kernel Capability Requirements [verified]:** Tracefs and BPF loading require
  `CAP_BPF`, `CAP_PERFMON`, and `CAP_SYS_NICE`. `kernel.unprivileged_bpf_disabled` is `2`.
- **T10: Clean-Room Boundary [verified]:** Clean-room implementation using Aya (`aya 0.14.0`,
  `aya-ebpf 0.2.1`). Zero code is copied from `neutral-engineering/autonice` (GPL-2.0).
- **T11: Dual Licensing Position [verified]:** Decided by Christian on 2026-10-09: the
  kernel-side BPF program object declares `Dual MIT/GPL` in its `license` section.
  The userspace loader and crate remain proprietary.
- **T12: CI Capability Invariant [verified]:** Unprivileged `ci-rust` container lacks BPF
  capabilities and nightly toolchains. Pre-compiled object `cas-ebpf.o` is committed
  and embedded. Integration tests run via `make ebpf-test` under sudo on ceres.

## Design

### Requirements (EARS)

- **Ubiquitous:** The `cas-ebpf` crate shall implement clean-room eBPF probes using Aya without copyleft dependencies.
- **Ubiquitous:** The root daemon `cas-ebpfd` shall hold `CAP_BPF`, `CAP_PERFMON` and `CAP_SYS_NICE` while serving unprivileged clients over a UDS socket, and shall accept `track` only for a pid owned by the peer's uid.
- **Event-driven:** WHEN a worker slot spawns, `cas-pool` shall configure a dedicated process group and clamp priority to `nice 19`.
- **Event-driven:** WHEN process exec or exit events fire, the eBPF probe shall stream structured `ExecEvent` records and update `run_hash`.
- **State-driven:** WHILE tracking worker processes, `cas-ebpfd` shall read the nice value of every tracked pid that execs and re-clamp it to `nice 19` when it is above 19. The kernel probe does not read the process group.
- **Unwanted behavior:** IF a tracked process exits, THEN the probe shall evict the process ID from `TRACKED_PIDS` to prevent map leaks.

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | Clean-room Aya implementation: Build probe loader with `aya` 0.14.0 (MIT OR Apache-2.0). Never import or vendor `neutral-engineering/autonice` (GPL-2.0). | Spec 0004 & Christian, 2026-10-09 |
| D2 | Licensing boundary: The kernel-side BPF program declares `Dual MIT/GPL` in its license section to permit GPL-only BPF helpers. Crate `cas-ebpf` remains proprietary. Subcrate uses `MIT OR Apache-2.0`. | Christian, 2026-10-09 |
| D3 | Compile and packaging boundary: Kernel BPF bytecode crate lives at `crates/cas-ebpf/ebpf` outside monorepo workspace. Pre-compiled object `cas-ebpf.o` is committed next to `cas-ebpf.o.sha256`; `make ebpf-build` rebuilds both (reproducible: two builds on ceres gave the same hash, 2026-10-09). `build.rs` never rebuilds; it refuses to compile the crate when the object and the recorded hash disagree. The crate embeds `cas-ebpf.o` via `include_bytes!`. Workspace builds on stable Rust. | CAS lanes, 2026-10-09 |
| D4 | Priority clamping mechanism (R2): `cas-pool` creates a dedicated process group (`setpgid(0, 0)`) for each worker slot and calls `setpriority(PRIO_PGRP, pgid, 19)` once before exec. All threads and child processes inherit nice 19 without privileges or race conditions. | Christian ruling, 2026-10-09 |
| D5 | Narrowed scope & anti-escape monitoring (R1): No host-wide autonice. Clamping applies exclusively to worker process trees registered in `TRACKED_PIDS` by `cas-pool`. The eBPF probe detects if a child process leaves its group (`setsid` or `setpgid`) and triggers re-clamping. Target compiler basenames serve as optional audit tags. | Christian ruling, 2026-10-09 |
| D6 | Execution audit ring buffer (R4): CER-2720 narrowed to exec and exit events. BPF ring buffer (`AUDIT_EVENTS`, 256 KiB) streams structured 168-byte `ExecEvent` records. Openat and connect are out of Phase 2. | Christian ruling, 2026-10-09 |
| D7 | SHA-256 `run_hash` execution transcript digest (D-E): Incremental SHA-256 digest over 168-byte `#[repr(C)]` little-endian records in ring-buffer delivery order. Serves as execution transcript proof in Saga receipts. Dropped-event counter tracks ring-buffer overflows. | Christian ruling, 2026-10-09 |
| D8 | Privileged daemon architecture (`cas-ebpfd`, R3): A dedicated root systemd unit `cas-ebpfd` runs with `CAP_BPF`, `CAP_PERFMON`, and `CAP_SYS_NICE`. It owns maps and ring buffers, serving unprivileged `cas-pool` over a UDS with strict allowlist (`track`, `untrack`, `stream_events`). `cas-pool` remains unprivileged. | Christian ruling, 2026-10-09 |
| D9 | Map lifecycle & eviction (D-F): Loader API provides `track(pid)` and `untrack(pid)`. `sched_process_exit` probe automatically evicts exited PIDs from `TRACKED_PIDS` (1024 entries), preventing map leaks. | CAS lanes, 2026-10-09 |
| D10 | Dependency pins & error handling: Pinned: `aya = "0.14.0"`, `aya-ebpf = "0.2.1"`, `sha2 = "0.11.0"`, `libc = "0.2.169"`. Uses `thiserror` for typed `EbpfError`. | Spec 0004 & CAS lanes, 2026-10-09 |

### Architecture and Flow

```mermaid
flowchart TD
    subgraph Pool["cas-pool (Unprivileged Worker Pool)"]
        SlotLaunch["Launch Worker Slot (setpgid + setpriority PGRP 19)"]
        PoolClient["cas-ebpfd UDS Client"]
    end

    subgraph Daemon["cas-ebpfd (Root Service, CAP_BPF CAP_SYS_NICE)"]
        UdsServer["UDS Socket: /run/cas/ebpf.sock"]
        Loader["Aya BPF Loader"]
        Consumer["RingBuf Event Consumer"]
        Reclamper["Escapee Reclamper (setpriority nice 19)"]
        Hasher["SHA-256 RunHash Transcript Digest"]
    end

    subgraph Kernel["Linux Kernel (ceres)"]
        TraceExec["Tracepoint: sched_process_exec"]
        TraceFork["Tracepoint: sched_process_fork"]
        TraceExit["Tracepoint: sched_process_exit"]
        BpfProg["cas-ebpf Probes (Dual MIT/GPL)"]
        PidMap["BPF Map: TRACKED_PIDS (1024 entries)"]
        RingBuf["BPF RingBuf: AUDIT_EVENTS (256 KiB)"]
    end

    SlotLaunch -->|Seed slot PID| PoolClient
    PoolClient -->|UDS track pid| UdsServer
    UdsServer --> PidMap
    Loader -.->|Loads cas-ebpf.o| BpfProg
    TraceExec --> BpfProg
    TraceFork --> BpfProg
    TraceExit --> BpfProg
    BpfProg --> PidMap
    BpfProg --> RingBuf
    TraceExit -->|Auto-evict PID| PidMap
    RingBuf -->|Stream 168B ExecEvent| Consumer
    Consumer --> Hasher
    BpfProg -->|Detect group escape| Reclamper
    Reclamper -->|setpriority nice 19| Kernel
    Consumer -->|Stream events to slot| PoolClient
    Hasher -->|Saga receipt run_hash| PoolClient
```

### Tracepoint Probes & Maps

1. **`sched_process_fork` Probe:**
   - Takes the parent tgid from `bpf_get_current_pid_tgid() >> 32` and reads
     `child_pid` (offset 44).
   - If the parent tgid exists in `TRACKED_PIDS`, inserts `child_pid` with the parent
     tgid as its value.
   - Propagates tracking to all child worker processes automatically. The key of
     `TRACKED_PIDS` is a task id; the main thread's tid equals the tgid, so the exec
     lookup by tgid hits. Thread tids enter at clone and leave at thread exit, so a
     thread-heavy worker (rustc, cargo) spends map entries while it runs. The map
     holds 1024 entries for every client together; the daemon caps each non-root uid
     at 256 tracked roots, and a refused insert is counted, not silent.

2. **`sched_process_exec` Probe:**
   - Reads `pid` (offset 12) and `filename` from `__data_loc char[]` (offset 8).
   - If `pid` exists in `TRACKED_PIDS`:
     - Reads command basename via `bpf_get_current_comm`, the uid via
       `bpf_get_current_uid_gid`, the timestamp via `bpf_ktime_get_ns`, and `ppid`
       from the `TRACKED_PIDS` value.
     - Reserves an `ExecEvent` in the `AUDIT_EVENTS` ring buffer and submits it.
       When `reserve` fails, increments `DROPPED_EVENTS[0]` (a two-slot
       `PerCpuArray<u64>`) instead. Slot 1 counts fork inserts refused because
       `TRACKED_PIDS` was full; `Probes::tracking_failures` reads it.
     - The kernel does not read the process group. The daemon checks the nice value
       of the pid on every event and re-clamps when it is above 19 (see below).

3. **`sched_process_exit` Probe:**
   - Reads `pid` (offset 24).
   - If `pid` exists in `TRACKED_PIDS`, deletes `pid` from `TRACKED_PIDS`.
   - Prevents stale entries from exhausting the 1024-entry hash map.

4. **`ExecEvent` Memory Layout (168 bytes):**
   - `timestamp_ns: u64` (8 bytes, offset 0): Monotonic kernel timestamp.
   - `pid: u32` (4 bytes, offset 8): Process ID.
   - `ppid: u32` (4 bytes, offset 12): Parent Process ID.
   - `uid: u32` (4 bytes, offset 16): User ID.
   - `comm: [u8; 16]` (16 bytes, offset 20): Process command name.
   - `filename: [u8; 128]` (128 bytes, offset 36): Executed binary path.
   - `_pad: u32` (4 bytes, offset 164): Explicit padding to 8-byte boundary.

### Priority Clamping Invariants

- **Process Group Clamping:** `cas-pool` sets worker process group (`setpgid(0, 0)`)
  and invokes `setpriority(PRIO_PGRP, pgid, 19)` before `execve`.
- **Thread Inheritance:** All threads created by rustc and cargo inherit nice 19
  from process group launch.
- **Anti-Escape Defense:** `setsid()` and `setpgid()` do not change a nice value.
  With the default `RLIMIT_NICE` of 0 an unprivileged process cannot raise its own
  priority, so the clamp survives a group escape on its own; a PAM or systemd limit
  that raises `RLIMIT_NICE` weakens that, which is why the second line exists.
  `cas-ebpfd` reads the nice value of the pid on every exec event of a tracked
  process and re-clamps it with `setpriority(PRIO_PROCESS, pid, 19)` when the nice
  value is below 19, that is, when the process got a higher priority than the
  worker floor (`clamp::reclamp_if_needed`). The daemon skips a process whose
  effective uid differs from its real uid (a setuid binary), so it never renices a
  privileged process on a tracked worker's behalf. The 200 ms poll bounds the delay.
- **System Call Safety:** The userspace side calls `setpgid`, `setpriority` and
  `getpgid` through `rustix` (`clamp.rs`), so it needs no `unsafe`. The one unsafe
  call site is `CommandExt::pre_exec` in the caller (`cas-pool`, and the gate in
  `tests/clamp.rs`), annotated with `#[allow(unsafe_code)]` and a `// SAFETY:`
  comment per AGENTS.md rule 14. The kernel program's `unsafe` blocks carry the
  same comments.

### Audit Hash (`run_hash`) Specification

The audit digest records exact execution sequence for Saga receipts:
1. `run_hash` is a per-run execution transcript digest, not a static content hash.
2. Events are serialized in strict monotonic ring buffer delivery order.
3. For each `ExecEvent`, serialize the 168-byte `#[repr(C)]` record in little-endian format.
4. Incremental SHA-256 hasher (`sha2` 0.11.0) digests each record sequentially.
5. On slot session completion, the hasher finalizes to hex string `run_hash`.
6. Dropped event counter (`dropped_events`) is included in the execution receipt.
   Any non-zero drop count invalidates deterministic execution claims.

### Deployment: `cas-ebpfd` Daemon

- **Systemd Unit File:** `/etc/systemd/system/cas-ebpfd.service`:
  ```ini
  [Unit]
  Description=Cerebral Agent Substrate eBPF Clamping and Audit Daemon
  After=network.target

  [Service]
  Type=simple
  ExecStart=/usr/local/bin/cas-ebpfd --socket /run/cas/ebpf.sock
  Restart=on-failure
  User=root
  Group=root
  RuntimeDirectory=cas
  RuntimeDirectoryMode=0755
  AmbientCapabilities=CAP_BPF CAP_PERFMON CAP_SYS_NICE
  CapabilityBoundingSet=CAP_BPF CAP_PERFMON CAP_SYS_NICE
  ProtectSystem=strict
  ProtectHome=read-only

  [Install]
  WantedBy=multi-user.target
  ```
- **Socket Permissions:** `/run/cas/ebpf.sock` is bound under umask `0117` and then set
  to mode `0660`, group `docker` or `ctodie`. It never exists with a wider mode.
- **Peer Rules:** the daemon reads `SO_PEERCRED` on every connection and refuses one
  without it. `track` is accepted only for a pid whose real uid (`/proc/<pid>/status`)
  equals the peer's uid, and only up to the per-uid quota (256 by default). `untrack`
  is accepted only for a pid that peer tracked. `stream` delivers only events whose
  uid is the peer's; `status` counts only the peer's pids. A root peer is exempt from
  all of these. Without the rules a socket-group member could make the root daemon
  renice another user's process tree, untrack another user's worker, fill the map,
  or read every user's exec records.
- **Socket Group:** `cas-ebpfd --socket-group <name>` resolves the group in
  `/etc/group` and `chown`s the socket to it after bind. The unit passes `ctodie`.
  Without it the socket is `root:root` and an unprivileged `cas-pool` gets `EACCES`.
- **UDS Protocol** (`protocol.rs`): one JSON object per line, request then
  response, at most 4096 bytes per line. A longer line closes the connection.
  Anything not on this list gets `{"ok":false,"error":"..."}` and the connection
  stays open.

  | request | response |
  |---|---|
  | `{"op":"track","pid":N,"ppid":P}` | `{"ok":true}` |
  | `{"op":"untrack","pid":N}` | `{"ok":true}` |
  | `{"op":"status"}` | `{"ok":true,"tracked":K,"dropped_events":D}` |
  | `{"op":"stream"}` | `{"ok":true}`, then one `{"event":{...}}` line per `ExecEvent` of the peer's uid (fields `timestamp_ns`, `pid`, `ppid`, `uid`, `comm`, `filename`), a `{"dropped":N}` line with the kernel's running drop total whenever it grows, and a `{"lagged":N}` line when this client fell behind by `N` event lines, until the client closes |

  `cas-pool` calls `track` right after `spawn` returns, with the child pid and its
  own pid as `ppid`. The fork probe then covers everything the worker starts. An
  exec the worker performs before the `track` call lands is not recorded; the
  worker's own first exec is the launcher's, which `cas-pool` knows already.
- **Unprivileged Fallback:** When `/run/cas/ebpf.sock` is absent, `cas-pool` falls back
  to `PRIO_PGRP` clamping alone and records audit status as `unattested`.

### Failure Modes & Errors

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EbpfError {
    #[error("kernel lacks BTF debug info")]
    BtfMissing,
    #[error("missing required capabilities (CAP_BPF, CAP_PERFMON, CAP_SYS_NICE)")]
    PermissionDenied,
    #[error("failed to attach to tracepoint {name}: {source}")]
    AttachTracepointFailed {
        name: &'static str,
        #[source]
        source: aya::ProgramError,
    },
    #[error("audit ring buffer dropped {dropped} events")]
    RingBufferOverflow { dropped: u64 },
    #[error("failed to connect to cas-ebpfd socket at {path}: {source}")]
    DaemonUnavailable {
        path: String,
        #[source]
        source: std::io::Error,
    },
}
```

## Plan

- [x] Create standalone eBPF subcrate `crates/cas-ebpf/ebpf/`.
- [x] Configure nightly toolchain and `bpf-linker` via mise.
- [x] Implement kernel tracepoint probes (`sched_process_exec`, `sched_process_fork`, `sched_process_exit`).
- [x] Compile and verify relocatable ELF object (`cas-ebpf.o`, 5,976 bytes, Dual MIT/GPL, reproducible hash).
- [x] Add `cas-ebpf.o` artifact, SHA-256 hash file, and `make ebpf-build` recipe.
- [x] Implement `build.rs` in `cas-ebpf` that refuses to build when the object and the hash file disagree.
- [x] Implement userspace BPF loader and map bindings in `cas-ebpf` (`loader::Probes`).
- [x] Implement `cas-ebpfd` daemon with Unix domain socket listener, line protocol and blocking client.
- [x] Implement `run_hash` transcript digest generator with `sha2 = "0.11.0"` (`audit::RunHash`).
- [x] Add `make ebpf-test` target: builds the live test binary unprivileged, runs it under sudo.
- [x] Unprivileged tests (47, in CI): wire layout and roundtrip; `run_hash` equal for identical
  streams and different for reordered or re-timed ones; dropped count unattests the receipt;
  a real child spawned through `enter_clamped_group` reads nice 19 in its own group; the daemon
  over a fake handle (track, untrack, status, allowlist, line bound, stream, dropped line,
  socket mode, peer rule).
- [ ] Live gate on ceres (`make ebpf-test`, sudo, operator-run): load and attach; exec of a
  tracked child is reported with the right pid, ppid, comm and filename; fork propagates
  tracking; untracked processes are silent; dropped counter starts at zero. Not run yet.
- [ ] Wire `cas-pool` (spec 0015): `enter_clamped_group` in `pre_exec`, `DaemonClient::track`
  after spawn, `AuditReceipt` into the Saga receipt, `unattested` when the daemon is absent.
- [ ] Install the unit on ceres (`crates/cas-ebpf/cas-ebpfd.service`), operator-run.
- [x] Update `specs/README.md` (status in-progress).

## Test

```bash
# Unit tests in workspace (unprivileged, probe-and-skip)
cargo test -p cas-ebpf -j 2 --lib

# Real eBPF probe integration tests (requires sudo on ceres)
make ebpf-test JOBS=2
```

## Notes

- **Aya Licence:** `aya` and `aya-ebpf` are licensed `MIT OR Apache-2.0`.
- **GPL Helper Compatibility:** Declaring `Dual MIT/GPL` in the ELF license section
  permits calling GPL-only kernel helpers without licensing contamination of
  userspace crates.
- **Clean-Room Boundary:** Zero code is copied from `neutral-engineering/autonice`.
- **Committed Object Invariant:** `crates/cas-ebpf/ebpf/cas-ebpf.o` is committed
  in git. Unprivileged CI containers (`ci-rust`) build `cas-ebpf` without nightly
  or `bpf-linker` by embedding the pre-compiled artifact via `include_bytes!`.
