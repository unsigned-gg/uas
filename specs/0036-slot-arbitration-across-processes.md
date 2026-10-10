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

Spec 0015 D1 enforces slot limits within a single `cas-pool` process (`max_slots = 2`) [reported: spec 0015 D1]. Spec 0025 D8 serializes wave build gates via `File::lock` [reported: spec 0025 D8]. However, no specification governs slot concurrency across multiple distinct CAS processes running on the same host [reported: integrating lane w25:p1 gap analysis].

When two independent CAS processes run concurrently on physical host `ceres`, each pool allocates its own 2 slots, admitting 4 concurrent workers against a 2-CPU throttling ceiling and violating the mandatory `-j 2` host constraint [reported: spec 0002; verified: `make smoke JOBS=2` grep check].

This specification defines host-wide slot arbitration in `cas-pool`. Capacity is anchored to the host using a fixed set of advisory-locked slot files under a canonical runtime directory. Processes acquire slot leases via non-blocking `File::try_lock`. When a process terminates, the kernel closes file descriptors and releases locks automatically, eliminating stale leases without supervisor daemons or reaping timeouts.

Facts, measured 2026-10-10 on ceres:

- **F1 [reported]:** Spec 0015 D1 specifies per-process slot capacity in `cas-pool`, defaulting to `max_slots = 2`.
- **F2 [reported]:** Spec 0025 D8 specifies a host lock (`File::lock`) serializing wave reconcile, critic runs, and Probes.
- **F3 [verified, 2026-10-10]:** Kernel advisory locks (`flock`) on open file descriptors release automatically when a process is killed. Command: `python3 -c "import fcntl, os, signal; f=open('/tmp/test_flock.lock','w'); fcntl.flock(f, fcntl.LOCK_EX|fcntl.LOCK_NB); os.kill(os.getpid(), signal.SIGKILL)" || python3 -c "import fcntl, sys; f=open('/tmp/test_flock.lock','w'); fcntl.flock(f, fcntl.LOCK_EX|fcntl.LOCK_NB); sys.exit(0)"` exits 0 (child exit 137, lock re-acquired immediately).
- **F4 [verified, 2026-10-10]:** `cas-domain` contains zero I/O imports. Command: `make smoke JOBS=2` grep check prints `smoke: ok`. Layering defined in Spec 0001 dictates `deck > acp > engine > pool > sandbox, ebpf > router > domain`.
- **F5 [reported]:** `rustix` 1.1.5 with `fs` is approved in Spec 0004 line 173 for `cas-pool`.
- **F6 [verified, 2026-10-10]:** `std::fs::File::try_lock` compiles and runs on the pinned 1.94.1 toolchain, providing non-blocking advisory file locking with `O_CLOEXEC`.

## Design

Host slot arbitration is owned by `cas-pool`. Layering holds: `cas-pool` sits above `cas-sandbox` and `cas-router`, and below `cas-engine`, while `cas-domain` remains zero I/O [reported: spec 0001; verified: `make smoke JOBS=2`]. `cas-domain` defines pure capacity types (`HostCapacity`, `HostSlotIndex`, `SlotAcquirePolicy`), while `cas-pool` manages the canonical slot directory and file descriptors.

Capacity is a host property. Total worker slots across all processes on the host must not exceed `host_max_slots` (default 2 on ceres). CAS represents host slots as `H` pre-allocated lock files under `/tmp/cas-slots/slot-<i>.lock` (configurable via `CAS_SLOT_DIR`, defaulting to `/tmp/cas-slots/`), where `0 <= i < host_max_slots`. A pool attempts an exclusive non-blocking lock (`File::try_lock`) sequentially across the files. Holding the open file descriptor retains the slot lease. Slot lock files are never unlinked.

### Decisions

| # | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Capacity is a host property, capped at `host_max_slots` (default 2 on ceres). All CAS processes on the host share this pool ceiling to guarantee `-j 2` host throttling. | Physical host CPU constraints; Spec 0002; CAS spec lane w1X:p1, 2026-10-10 |
| D2 | Host slots live as `H` lock files in canonical directory `/tmp/cas-slots/slot-<i>.lock` (override via `CAS_SLOT_DIR`). The directory is created with mode `0700` owned by the process UID; symlinks and looser permissions are refused with `PoolError::DirectoryInsecure`. Disagreements on `host_max_slots` clamp to the minimum value in `capacity.json`. | Multi-process directory security and consensus; CAS spec lane w1X:p1, 2026-10-10 |
| D3 | Locking uses `std::fs::File::try_lock` (wrapping `flock(2)` with `O_CLOEXEC`), matching Spec 0025 D8. Holding the open file descriptor retains the lease; when a process terminates, the kernel closes file descriptors and frees the lock without external janitor sweeps. | Consistent locking primitive across specs; CAS spec lane w1X:p1, 2026-10-10 |
| D4 | When host capacity is exhausted, `cas-pool` supports two non-hanging policies: `Try` (immediate return of `PoolError::HostCapacityExceeded`) and `WaitBounded` (polling with backoff starting at 50 ms up to a maximum 500 ms interval, bounded by configurable `slot_acquire_timeout`, default 30 s). Unbounded blocking is forbidden. | Anti-hang invariant; CAS spec lane w1X:p1, 2026-10-10 |
| D5 | Spec 0036 and Spec 0025 D8 govern disjoint execution domains and do not overlap. Wave steps have no CAS slot (Spec 0025 F2, D9); wave gates run under Spec 0025 D8, while worker slots run under Spec 0036. | Disjoint execution domains; Spec 0025 F2, D9; CAS spec lane w1X:p1, 2026-10-10 |
| D6 | Crate layering: `cas-pool` owns filesystem I/O and lock descriptor management; `cas-domain` provides pure types and validation with zero I/O; `cas-engine` calls `cas-pool` without direct lock manipulation. `PoolError::HostCapacityExceeded` is added distinct from `PoolError::CapacityExhausted` (host-wide vs internal pool capacity). | Crate layering purity; Spec 0001; CAS spec lane w1X:p1, 2026-10-10 |
| D7 | Lock descriptor hygiene and telemetry: Slot files are never unlinked. Descriptors are opened with `O_CLOEXEC` and never passed to child processes. Upon acquisition, the holder writes a fixed-size advisory JSON metadata record (`pid`, `slot_id`, `acquired_at`) at offset 0 without truncating. Readers treat metadata as purely diagnostic and unauthenticated; allocation validity relies strictly on `File::try_lock`. | File descriptor safety and leak prevention; CAS spec lane w1X:p1, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0036-01]** (Ubiquitous): The `cas-pool` shall enforce host-wide worker slot concurrency limits across all CAS processes sharing the machine.
- **[REQ-0036-02]** (Ubiquitous): The slot arbitration mechanism shall manage `H` lock files named `slot-<i>.lock` (where `0 <= i < host_max_slots`) in a canonical directory `/tmp/cas-slots` with mode `0700` owned by the process UID.
- **[REQ-0036-03]** (Event-driven): WHEN a pool allocates a worker slot, `cas-pool` shall attempt non-blocking exclusive advisory file locking (`File::try_lock`) sequentially across `slot-0.lock` through `slot-(H-1).lock`.
- **[REQ-0036-04]** (State-driven): WHILE a worker slot is actively leased, `cas-pool` shall retain the open file descriptor holding the exclusive kernel lock with `O_CLOEXEC`, never passing the descriptor to worker child processes.
- **[REQ-0036-05]** (Unwanted): IF all `H` host slot locks are held, THEN `cas-pool` under policy `Try` shall reject the allocation immediately with `PoolError::HostCapacityExceeded`.
- **[REQ-0036-06]** (Unwanted): IF all `H` host slot locks are held under policy `WaitBounded`, THEN `cas-pool` shall poll with backoff (50 ms initial, 500 ms maximum) and fail with `PoolError::HostCapacityExceeded` if configurable `slot_acquire_timeout` elapses.
- **[REQ-0036-07]** (Event-driven): WHEN a holding process terminates, a surviving `cas-pool` process waiting under `SlotAcquirePolicy::WaitBounded` shall acquire the newly released slot within one maximum poll interval (500 ms), with leases held exclusively through open descriptors and no exit cleanup routines.
- **[REQ-0036-08]** (Ubiquitous): The `cas-pool` host slot locks shall operate disjointly from the Spec 0025 D8 wave lock, since wave runs allocate no CAS slots per Spec 0025 D9.
- **[REQ-0036-09]** (Ubiquitous): The `cas-domain` shall define pure data types for host slot capacity with zero I/O imports.
- **[REQ-0036-10]** (Event-driven): WHEN acquiring a slot lock file, `cas-pool` shall write fixed-length advisory holder metadata at offset 0 without unlinking the file, and readers shall treat metadata as advisory while relying strictly on `File::try_lock` for slot status.
- **[REQ-0036-11]** (Unwanted): IF the slot directory exists with mode looser than `0700`, with a foreign owner UID, or as a symlink, THEN `cas-pool` shall reject initialization with `PoolError::DirectoryInsecure`.
- **[REQ-0036-12]** (Unwanted): IF a multi-process test environment cannot fork or execute child processes via `std::env::current_exe()`, THEN the test suite shall fail explicitly rather than skip silently.

## Plan

- [ ] Define pure host capacity types (`HostCapacity`, `HostSlotIndex`, `SlotAcquirePolicy`) in `cas-domain`.
- [ ] Add `HostCapacityExceeded(usize)` and `DirectoryInsecure(PathBuf, String)` to `PoolError` in `cas-pool`.
- [ ] Add `host_max_slots` and `slot_acquire_timeout` configuration fields to `PoolConfig` in `cas-pool`.
- [ ] Implement `HostSlotArbiter` in `cas-pool` managing canonical directory creation, `File::try_lock` acquisition, `O_CLOEXEC` descriptor retention, and polling backoff (50 ms to 500 ms).
- [ ] Integrate `HostSlotArbiter` into `cas_pool::slot::SlotPool::allocate` to acquire a host slot before leasing a worker worktree.
- [ ] Implement multi-process integration tests in `cas-pool` using `std::env::current_exe()` helper sub-processes to verify two-process capacity limits, timeout failure, and abnormal termination recovery.

## Test

```bash
cargo test -p cas-pool -j 2 --test slot_arbitration && \
  cargo test -p cas-pool -j 2 --lib slot::arbitration
```

A passing suite verifies that two independent processes running `cas-pool` respect the host capacity ceiling (REQ-0036-01, REQ-0036-02, REQ-0036-03, REQ-0036-04). It verifies bounded timeout behavior when capacity is exhausted (REQ-0036-05, REQ-0036-06), immediate reclamation upon holder death (REQ-0036-07), disjoint lock operation (REQ-0036-08), domain purity (REQ-0036-09), advisory metadata (REQ-0036-10), directory security (REQ-0036-11), and non-skipping execution (REQ-0036-12).

A pass requires each of these negative control cases to exist and to go red when its own check is deleted:

| Case | Where | Requirement |
|---|---|---|
| process 2 receives `HostCapacityExceeded` when host slots are full | `cas-pool` multi-process test | REQ-0036-01, REQ-0036-05 |
| killed process releases slot lock, acquired by waiting process within 500 ms | `cas-pool` multi-process test | REQ-0036-07 |
| wait bounded timeout expires and fails after configured duration | `cas-pool` unit test | REQ-0036-06 |
| symlink or non-0700 mode on slot directory fails with `DirectoryInsecure` | `cas-pool` unit test | REQ-0036-02, REQ-0036-11 |
| slot lock files are retained on drop and never unlinked | `cas-pool` unit test | REQ-0036-02, REQ-0036-10 |
| lock file descriptor is marked close-on-exec (`O_CLOEXEC`) | `cas-pool` unit test | REQ-0036-04 |
| sequential slot acquisition claims index 0 then index 1 | `cas-pool` unit test | REQ-0036-03 |
| wave step executes without acquiring host slot lock | `cas-pool` unit test | REQ-0036-08 |
| domain types compile without `std::net`, `std::fs`, or `tokio` | `cas-domain` unit test | REQ-0036-09 |
| multi-process test fails when child process spawn fails | `cas-pool` test harness | REQ-0036-12 |

The primary negative control: if the host slot iteration loop in `cas-pool` is bypassed, the test asserting that a second process receives `HostCapacityExceeded` when 2 slots are occupied must go red.

## Notes

- Host property vs Process property: Spec 0015 handled per-pool limits, allowing concurrent pools to multiply resource usage. Spec 0036 resolves this by anchoring the ceiling to the host [reported: spec 0015].
- Dead holder resolution: Advisory locks via `File::try_lock` avoid the need for dead-holder detection loops; the kernel automatically drops all locks upon process termination (normal, panic, or `SIGKILL`) [verified: F3].
- Relationship with Spec 0025 D8: Spec 0025 D8 locks heavyweight wave gate commands (`make check`); Spec 0036 locks general worker slots. Because wave runs allocate no CAS slots (Spec 0025 F2, D9), the two synchronization mechanisms govern disjoint scopes and never overlap.
- Layering preservation: `cas-pool` sits above `cas-sandbox` and `cas-router`, and below `cas-engine`, while `cas-domain` remains zero I/O [reported: spec 0001; verified: `make smoke JOBS=2`].
