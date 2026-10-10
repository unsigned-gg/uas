---
status: planned
created: 2026-10-09
tags: [pool, gitoxide, cgroup, freezer, ebpf, phase-3]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0004-dependency-and-licence-policy, 0007-lease-rules, 0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation, 0014-ebpf-priority-clamping-and-audit-probes]
---

# 0015-slot-pool-cgroup-freezer-and-gitoxide

## Overview

Host `ceres` executes many concurrent agent tasks. Spawning unbounded workers
or allocating physical Git worktrees on host disk causes disk exhaustion and
CPU starvation. Spec 0001 invariant 4.2 and Global Config mandate strict host
throttling (`-j 2`).

This spec defines the worker slot manager in `cas-pool` (Phase 3, CER-2682):
- **CER-2723**: Slot allocation, lease tracking, and concurrency limits.
- **CER-2724**: Warm worktree forks with in-process gitoxide (`gix`).
- **CER-2722**: Worker suspend and resume using the Linux cgroup v2 freezer.
- **eBPF Clamping & Audit Hooks**: Process priority clamping (`nice 19`) via
  `cas_ebpf::clamp::enter_clamped_group` and kernel execution audit via
  `cas_ebpf::client::DaemonClient` (spec 0014).

## Design

### Requirements (EARS)

- **Ubiquitous:** The `cas-pool` slot manager shall enforce host concurrency limits (`max_slots = 2`) on worker allocations.
- **Ubiquitous:** The pool shall manage ephemeral worktrees using in-process `gix` operations without external `git` subprocesses.
- **Event-driven:** WHEN freezing an inactive worker, the pool shall write `1` to `cgroup.freeze`.
- **Event-driven:** WHEN spawning a worker process, the pool shall call `cas_ebpf::clamp::enter_clamped_group` in `pre_exec` and register the child PID via `DaemonClient::track`.
- **State-driven:** WHILE a worker executes, the pool shall consume `EventStream` and incorporate `AuditReceipt` into the task's Saga execution receipt.
- **State-driven:** WHILE reclaiming an expired slot, the pool shall reap child processes and reset worktree state.
- **Unwanted behavior:** IF cgroup v2 operations fail, THEN the pool shall fallback to POSIX `SIGSTOP` and `SIGCONT` signals.
- **Unwanted behavior:** IF `cas-ebpfd` is unavailable, THEN the pool shall proceed with process group clamping and mark the audit receipt as `unattested`.

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | Slot concurrency limit: `cas-pool` enforces a strict capacity limit (`max_slots`, default 2 on host ceres). Requests exceeding capacity are rejected or placed in a bounded queue. | CAS lane, 2026-10-09 |
| D2 | Worktree isolation via `gix`: Worktrees share the base repository object store. `gix` creates ephemeral checkouts in `/workspace/slots/<slot-id>`. It resets dirty files on reclaim without invoking `git` CLI sub-processes. | CAS lane, 2026-10-09 |
| D3 | Process suspension and resource quota via cgroup v2: `cas-pool` creates `/sys/fs/cgroup/cas/slot-<id>`. It writes `1` to `cgroup.freeze` to suspend workers, and `0` to thaw them. It configures CPU quotas via `cpu.max` (defaulting to 2 CPUs: 200,000us / 100,000us per Spec 0020 D3). | Spec 0020 & CAS lane, 2026-10-09 |
| D4 | Graceful freezer fallback: If cgroup v2 is not writable, `cas-pool` falls back to POSIX `SIGSTOP` and `SIGCONT` signals sent to the worker process group. | CAS lane, 2026-10-09 |
| D5 | Runtime choice: `cas-pool` uses standard OS threads with `std::sync` primitives for slot management. Thread blocking operations handle synchronous `gix` disk operations and cgroup file I/O. | CAS lane, 2026-10-09; ruled in spec 0019 (D2, Option A, the operator, 2026-10-10 [reported, relayed by the integrator, mesh e42efbc2]) |
| D6 | eBPF clamping and audit integration: Slot worker spawn sets `enter_clamped_group` in `pre_exec` (sets process group and `nice 19`). Post-spawn registers PID via `DaemonClient::track(child_pid, own_pid)` over UDS. Execution streams `ExecEvent` records into `AuditReceipt` with `run_hash`. Daemon unavailability yields `unattested` audit receipt without failing worker execution. | Spec 0014 & CAS lane, 2026-10-09 |

### Architecture and Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> Leased: allocate(lease_id)
    Leased --> Running: spawn_worker() [pre_exec clamp + eBPF track]
    Running --> Suspended: freeze()
    Suspended --> Running: thaw()
    Running --> Reclaiming: exit / terminate() [seal AuditReceipt]
    Suspended --> Reclaiming: terminate()
    Reclaiming --> Idle: gix_reset_worktree()
    Reclaiming --> [*]: pool_shutdown()
```

### Slot States and Operations

1. **`Idle`**: The slot has an initialized, clean worktree directory. No process runs.
2. **`Leased`**: A caller acquired a `SlotLease` governed by `cas-domain` lease rules (spec 0007).
3. **`Running`**: Sandboxed worker process executes with Landlock (spec 0010) and private network namespace (spec 0011).
   - In `pre_exec`: invokes `cas_ebpf::clamp::enter_clamped_group` to create process group and clamp to `nice 19`.
   - Post-spawn: connects to `cas-ebpfd` via `/run/cas/ebpf.sock` and calls `track(child_pid, own_pid)`.
   - Background thread consumes `EventStream` to build execution transcript digest (`run_hash`).
4. **`Suspended`**: Process execution halted via `cgroup.freeze` (or `SIGSTOP`). Memory and file descriptors remain intact.
5. **`Reclaiming`**: Worker reaped. `AuditReceipt` (with `run_hash`, `dropped_events`, and `cpu_cap` per Spec 0020) sealed and returned to caller. In-process `gix` restores index and removes untracked files.

### Cgroup v2 Freezer Interface

- **Cgroup Path**: `/sys/fs/cgroup/cas/slot-<slot-id>`
- **Freeze Command**:
  1. Open `/sys/fs/cgroup/cas/slot-<slot-id>/cgroup.freeze`.
  2. Write `"1\n"`.
  3. Poll `/sys/fs/cgroup/cas/slot-<slot-id>/cgroup.events` until `frozen 1` appears.
- **Thaw Command**:
  1. Open `/sys/fs/cgroup/cas/slot-<slot-id>/cgroup.freeze`.
  2. Write `"0\n"`.
  3. Poll `cgroup.events` until `frozen 0` appears.
- **Latency Target**: Suspend and thaw operations complete within 10 milliseconds.

### In-Process Gitoxide Worktrees

- `cas-pool` depends on `gix = "0.88.0"` (spec 0004 approved pin).
- Base repository stays read-only at `/workspace/base-repo`.
- On slot provision:
  - Create slot workspace at `/workspace/slots/<slot-id>`.
  - Initialize Git working directory linked to the shared object database.
  - Check out target branch head without disk duplication.
- On slot reclaim:
  - Discard untracked files and revert unstaged modifications.
  - Return worktree to pristine HEAD state in under 100 milliseconds.

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("slot pool capacity exhausted (max {0})")]
    CapacityExhausted(usize),
    #[error("slot {0} lease expired")]
    LeaseExpired(cas_domain::SlotId),
    #[error("cgroup freezer error: {0}")]
    Freezer(String),
    #[error("gitoxide operation failed: {0}")]
    Gitoxide(String),
    #[error("worker process spawn failed: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("ebpf audit client error: {0}")]
    Ebpf(String),
}
```

## Plan

- [ ] Add `gix = "0.88.0"`, `rustix = "1.1.5"`, and internal `cas-ebpf` dependency to `crates/cas-pool/Cargo.toml`.
- [ ] Implement `cas_pool::slot::Slot` state machine and `SlotPool` allocation queue (CER-2723).
- [ ] Implement `cas_pool::worktree::WarmWorktree` using in-process `gix` checkouts and resets (CER-2724).
- [ ] Implement `cas_pool::freezer::CgroupFreezer` with `SIGSTOP` fallback (CER-2722).
- [ ] Wire `cas_ebpf` hooks: `enter_clamped_group` in worker `pre_exec`, `DaemonClient::track` post-spawn, and `EventStream` audit capture into `AuditReceipt`.
- [ ] Implement unprivileged fallback returning `unattested` audit receipt when `cas-ebpfd` is unavailable.
- [ ] Write integration test `tests/slot_lifecycle.rs` verifying:
  - Allocation respects `max_slots` concurrency ceiling.
  - Freeze suspends process CPU execution and thaw resumes it.
  - Reclaim restores modified files to clean commit state via `gix`.
  - Worker processes spawn with `nice 19` and produce valid `AuditReceipt`.
- [ ] Update `specs/README.md` and `CHANGELOG.md`.

## Test

Run integration and unit tests for `cas-pool`:

```bash
cargo test -p cas-pool -j 2
```

## Notes

- **Kernel Requirement**: Cgroup v2 unified hierarchy must be mounted at `/sys/fs/cgroup`. On WSL2 kernel 6.6, systemd mounts cgroup v2 by default.
- **Resource Protection**: Host throttling is preserved because active slots never exceed the configured worker cap.
- **Audit Resilience**: If `cas-ebpfd` is not running, workers execute successfully with `PRIO_PGRP` clamping while marking audit receipts `unattested`.
