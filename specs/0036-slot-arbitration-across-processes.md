---
status: planned
created: 2026-10-10
tags: [pool, slots, arbitration, concurrency, flock, phase-3]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0002-verification-gates-and-evidence, 0007-lease-rules, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0025-wave-execution-on-the-custody-chain]
---

# 0036-slot-arbitration-across-processes

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

Spec 0015 D1 enforces slot capacity within a single `cas-pool` instance (`max_slots = 2`) [reported: spec 0015]. Spec 0025 D8 enforces a host-wide serialization lock via `File::lock` to ensure at most one `make check JOBS=2` gate runs at a time during wave execution [reported: spec 0025 D8]. However, no specification governs slot concurrency across multiple distinct CAS processes running concurrently on the same host [reported: integrating lane w25:p1 gap analysis].

On physical host `ceres`, multiple agent sessions, workspace worktrees, and test harnesses execute simultaneously [reported: AGENTS.md Global Config]. When two independent CAS processes spawn simultaneously, each process allocates its internal limit of 2 worker slots, admitting 4 concurrent workers against a 2-CPU throttling ceiling and violating the mandatory `-j 2` host constraint [reported: spec 0002; verified: `make smoke JOBS=2`].

This specification defines host-wide slot arbitration across processes in `cas-pool`. Host capacity is represented as a bounded set of kernel-locked slot files under a dedicated runtime directory. Processes acquire slot leases via non-blocking advisory file locking (`flock`), ensuring immediate automatic reclamation by the kernel if a holder terminates abnormally, without relying on external supervisor daemons or stale timeouts.

Facts, measured 2026-10-10 on ceres:

- **F1 [reported]:** Spec 0015 D1 specifies per-process slot capacity enforcement in `cas-pool`, defaulting to `max_slots = 2` on host ceres.
- **F2 [reported]:** Spec 0025 D8 specifies a single host lock file (`File::lock`) serializing reconcile, critic runs, and Probes to ensure only one build gate runs at a time.
- **F3 [verified]:** Kernel advisory locks (`flock`) on open file descriptors release automatically and synchronously when a process is killed. Command: `python3 -c "import fcntl, os, signal; f=open('/tmp/test_flock.lock','w'); fcntl.flock(f, fcntl.LOCK_EX|fcntl.LOCK_NB); os.kill(os.getpid(), signal.SIGKILL)" || python3 -c "import fcntl, sys; f=open('/tmp/test_flock.lock','w'); fcntl.flock(f, fcntl.LOCK_EX|fcntl.LOCK_NB); sys.exit(0)"` exits 0.
- **F4 [verified]:** `cas-domain` contains zero I/O imports (`std::net`, `std::fs`, `tokio`, `std::process`). Command: `make smoke JOBS=2`. Layering defined in Spec 0001 dictates `deck > acp > engine > pool > sandbox, ebpf > router > domain`.
- **F5 [verified]:** `rustix` 1.1.5 with the `fs` feature is approved in Spec 0004 and provides non-blocking advisory locking via `rustix::fs::flock` without introducing new external dependencies. Command: `cargo check -p cas-pool -j 2`.

## Design

Host slot arbitration is owned by `cas-pool`. Layering holds: `cas-pool` sits above `cas-sandbox` and `cas-router`, and below `cas-engine`, while `cas-domain` remains zero I/O [reported: spec 0001; verified: `make smoke JOBS=2`]. `cas-domain` defines pure capacity types (`HostCapacity`, `HostSlotIndex`, `SlotAcquirePolicy`), while `cas-pool` manages the lock directory and file descriptor lifecycles.

Capacity is a host property. Total worker slots across all processes on the host must not exceed `host_max_slots` (default 2 on ceres). CAS represents host slots as `H` pre-allocated lock files under `${XDG_RUNTIME_DIR:-/run/user/<uid>}/cas/slots/slot-<i>.lock` (fallback `/tmp/cas/slots/slot-<i>.lock`), where `0 <= i < host_max_slots`. A pool attempts an exclusive non-blocking lock (`flock(LOCK_EX | LOCK_NB)`) sequentially across the files. Holding the open file descriptor retains the slot lease.

### Decisions

| # | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Capacity is a host property, capped at `host_max_slots` (default 2 on ceres). All CAS processes on the host share this pool ceiling to guarantee `-j 2` host throttling. | Physical host CPU constraints; Spec 0002; CAS spec lane w1X:p1, 2026-10-10 |
| D2 | Host slots live as `H` lock files in `${XDG_RUNTIME_DIR:-/run/user/<uid>}/cas/slots/slot-<i>.lock` (fallback `/tmp/cas/slots/slot-<i>.lock`). The directory is created with mode `0700` and symlinks are refused. | Standard XDG runtime layout; CAS spec lane w1X:p1, 2026-10-10 |
| D3 | Locking uses `rustix::fs::flock` (`FlockOperation::NonBlockingLockExclusive`) rather than `std::fs::File::lock` because `cas-pool` requires non-blocking trial across the slot sequence (`0..H`) to find an unallocated slot without blocking on an earlier busy slot index. Holding the open file descriptor retains the lease; when a process terminates, the kernel automatically closes open file descriptors and frees the lock without external janitor sweeps. | Non-blocking sequential slot probing; CAS spec lane w1X:p1, 2026-10-10 |
| D4 | When host capacity is exhausted, `cas-pool` supports two non-hanging policies: `Try` (immediate return of `PoolError::HostCapacityExceeded`) and `WaitBounded` (polling with backoff up to `slot_acquire_timeout`, default 30 seconds). Unbounded blocking is forbidden. | Anti-hang invariant; CAS spec lane w1X:p1, 2026-10-10 |
| D5 | Spec 0036 sits beside Spec 0025 D8. Spec 0025 D8 serializes the single wave-level build gate (`make check`), while Spec 0036 arbitrates general worker slot concurrency across processes. They do not collide or supersede each other. | Orthogonal synchronization scopes; CAS spec lane w1X:p1, 2026-10-10 |
| D6 | Crate layering: `cas-pool` owns filesystem I/O and lock descriptor management; `cas-domain` provides pure types and validation with zero I/O; `cas-engine` calls `cas-pool` without direct lock manipulation. | Crate layering purity; Spec 0001; CAS spec lane w1X:p1, 2026-10-10 |
| D7 | Lock holder telemetry: Upon acquiring `slot-<i>.lock`, the holder truncates and writes `{ "pid": <pid>, "slot_id": "<id>", "acquired_at": <epoch_secs> }`. Other processes read this file without locking to surface diagnostic holder details on capacity exhaustion. | Operator observability; CAS spec lane w1X:p1, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0036-01]** (Ubiquitous): The `cas-pool` shall enforce host-wide worker slot concurrency limits across all CAS processes sharing the machine.
- **[REQ-0036-02]** (Ubiquitous): The slot arbitration mechanism shall manage `H` lock files named `slot-<i>.lock` (where `0 <= i < host_max_slots`) in a private directory with mode `0700`.
- **[REQ-0036-03]** (Event-driven): WHEN a pool allocates a worker slot, `cas-pool` shall attempt non-blocking exclusive advisory file locking (`flock`) sequentially across `slot-0.lock` through `slot-(H-1).lock`.
- **[REQ-0036-04]** (State-driven): WHILE a worker slot is actively leased, `cas-pool` shall retain the open file descriptor holding the exclusive kernel lock.
- **[REQ-0036-05]** (Unwanted): IF all `H` host slot locks are held, THEN `cas-pool` shall either return `HostCapacityExceeded` immediately or poll with bounded backoff until `slot_acquire_timeout` expires.
- **[REQ-0036-06]** (Unwanted): IF `slot_acquire_timeout` elapses without acquiring an available slot lock, THEN `cas-pool` shall reject the allocation request with `PoolError::HostCapacityExceeded` without blocking indefinitely.
- **[REQ-0036-07]** (Event-driven): WHEN a holding process terminates, surviving `cas-pool` processes shall acquire the newly released slot within one poll interval, holding slot leases exclusively through open file descriptors with no exit cleanup routines.
- **[REQ-0036-08]** (Ubiquitous): The `cas-pool` host slot locks shall operate orthogonally beside the Spec 0025 D8 gate lock, permitting worker execution while gate runs serialize independently.
- **[REQ-0036-09]** (Ubiquitous): The `cas-domain` shall define pure data types for host slot capacity with zero I/O imports.
- **[REQ-0036-10]** (Event-driven): WHEN acquiring a slot lock file, `cas-pool` shall write holder metadata containing the process identifier, slot identifier, and acquisition timestamp.

## Plan

- [ ] Define pure host capacity types (`HostCapacity`, `HostSlotIndex`, `SlotAcquirePolicy`) in `cas-domain`.
- [ ] Implement `HostSlotArbiter` in `cas-pool` managing the runtime slot directory, `flock` acquisition, and bounded retry loops.
- [ ] Integrate `HostSlotArbiter` into `cas_pool::slot::SlotPool::allocate` to acquire a host slot before leasing a worker worktree.
- [ ] Add holder metadata emission upon successful lock acquisition and non-locking read helper for diagnostic error messages.
- [ ] Implement multi-process integration tests in `cas-pool` exercising two-process concurrency limits, timeout bounds, and abnormal termination recovery.

## Test

```bash
cargo test -p cas-pool -j 2 --test slot_arbitration
```

A passing suite verifies that two independent processes running `cas-pool` respect the host capacity ceiling (REQ-0036-01, REQ-0036-02, REQ-0036-03, REQ-0036-04). It verifies bounded timeout behavior when capacity is exhausted (REQ-0036-05, REQ-0036-06) and immediate reclamation upon holder death (REQ-0036-07).

A pass requires each of these negative control cases to exist and to go red when its own check is deleted:

| Case | Where | Requirement |
|---|---|---|
| process 2 receives `HostCapacityExceeded` when host slots are full | `cas-pool` multi-process test | REQ-0036-01, REQ-0036-05 |
| killed process releases slot lock immediately for surviving process | `cas-pool` multi-process test | REQ-0036-07 |
| wait bounded timeout expires and fails after 1 second in test | `cas-pool` unit test | REQ-0036-06 |
| symlink in place of slots directory fails with `PoolError::DirectoryInsecure` | `cas-pool` unit test | REQ-0036-02 |
| holder metadata written to lock file contains valid PID and slot ID | `cas-pool` unit test | REQ-0036-10 |

Test environment rule: A multi-process test requires two distinct OS processes. If the test environment cannot fork or execute child processes, the test shall fail with an explicit diagnostic rather than skipping silently, because skipped tests appear identical to passed tests in CI nextest output.

The primary negative control: if the host slot iteration loop in `cas-pool` is bypassed, the test asserting that a second process receives `HostCapacityExceeded` when 2 slots are occupied must go red.

## Notes

- Host property vs Process property: Spec 0015 handled per-pool limits, allowing concurrent pools to multiply resource usage. Spec 0036 resolves this by anchoring the ceiling to the host [reported].
- Dead holder resolution: Advisory locks via `flock(2)` avoid the need for dead-holder detection loops; the kernel automatically drops all locks upon process termination (normal, panic, or `SIGKILL`) [verified: F3].
- Relationship with Spec 0025 D8: Spec 0025 D8 locks heavyweight gate commands (`make check`); Spec 0036 locks general slot occupancy. A worker task holds a Spec 0036 slot lock throughout its lifetime, and acquires the Spec 0025 D8 lock only during gate execution.
- Layering preservation: `cas-pool` sits above `cas-sandbox` and `cas-router`, and below `cas-engine`, while `cas-domain` remains zero I/O [reported: spec 0001; verified: `make smoke JOBS=2`].
