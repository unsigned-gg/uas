---
status: planned
created: 2026-10-10
tags: [pool, runtime, tokio, heph, threads, phase-3]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0004-dependency-and-licence-policy, 0013-llm-gateway-socket-proxy, 0014-ebpf-priority-clamping-and-audit-probes, 0015-slot-pool-cgroup-freezer-and-gitoxide]
---

# 0019-pool-runtime-decision

## Overview

Spec 0001 open gap 1 says nobody chose the runtime for `cas-pool`. The research
design shows thread-per-core actors (heph). Spec 0015 D5 then picked standard OS
threads for the pool, as a CAS lane decision, not an operator ruling. Spec 0001
Plan item 3 still asks for the spec that chooses the pool runtime. This spec is
that record. It lays out the three options with measured facts and leaves the
ruling to the operator.

Facts, measured 2026-10-10 on ceres unless stated:

- **F1 [verified]:** `tokio` 1.53.1 is pinned in the workspace with features
  `rt-multi-thread`, `macros`, `net`, `time`, `sync`, `io-util`. Two crates use
  it: `cas-router` (the gateway socket proxy, spec 0013) and `cas-ebpf` (the
  `cas-ebpfd` socket daemon, spec 0014). Command: `grep -l tokio crates/*/Cargo.toml`.
- **F2 [verified]:** spec 0015 D5: "`cas-pool` uses standard OS threads with
  `std::sync` primitives for slot management. Thread blocking operations handle
  synchronous `gix` disk operations and cgroup file I/O." Decided by the CAS lane
  on 2026-10-09.
- **F3 [verified]:** spec 0017 names `futures-util` 0.3.34 and `tokio` 1.53.1 for
  `cas-router`; spec 0018 names `tokio-stream` 0.1.19 and Axum SSE for `cas-deck`.
  Every I/O server in CAS is on Tokio.
- **F4 [verified]:** ceres: 16 CPUs (`nproc`), kernel `6.6.87.2-microsoft-standard-WSL2`,
  `CONFIG_IO_URING=y`, `/proc/sys/kernel/io_uring_disabled` is `0`. heph can run here.
- **F5 [verified]:** crates.io: `heph` 0.4.0, `heph-rt` 0.4.1 (`cargo search`).
  Licence MIT per `cerebral-work/research@4b3ed94:repos/heph.md`. Neither is pinned
  in spec 0004.
- **F6 [reported]:** `repos/heph.md` section 4 verdict: Tokio stays the choice for
  cortex (Tonic gRPC ecosystem, cross-platform WSL2 dev); heph is a reference
  blueprint. The evaluation has `status: draft` and an empty `approved_by`.
- **F7 [verified]:** the pool's work is blocking syscalls and file I/O: `gix`
  checkouts and resets (spec 0015 D2), cgroup writes and `cgroup.events` polls
  (0015 D3), `setpgid` and `setpriority` before exec (0014 D4), `pre_exec` hooks
  for Landlock and netns (0010, 0011), and one `DaemonClient` socket per slot
  (0014). None of it is high-connection-count network I/O.
- **F8 [verified]:** the pool has one hard concurrency ceiling already: the host
  throttling invariant, `-j 2` on shared workstations (AGENTS.md rule 2). Slot
  count is small by design; it is not thousands of actors.
- **F9 [reported]:** the research lane (mesh reply 1b8213f2, 2026-10-10) confirms
  the fork is open, that no benchmark has measured `cas-pool`, and recommends
  keeping `cas-domain` runtime-neutral until criterion benchmarks measure the pool.
  Approval of the design itself follows the same benchmarks (research `AGENTS.md`,
  the operator by deck merge).

## Design

### Requirements (EARS)

- **Ubiquitous:** `cas-domain` shall stay runtime-neutral: no `tokio`, no `heph`,
  no thread spawning (spec 0001).
- **Ubiquitous:** The pool shall run every blocking syscall and file operation on
  a thread that may block, never on an async executor's worker thread.
- **Event-driven:** WHEN a crate serves a socket or HTTP endpoint, it shall use
  the workspace's pinned `tokio` runtime, so one I/O stack exists in the workspace.
- **State-driven:** WHILE the runtime ruling is open, no crate shall add `heph`,
  `heph-rt` or a second async runtime to the workspace.
- **Unwanted behavior:** IF a future measurement shows the pool's control loop is
  the bottleneck, THEN a new spec shall record the measurement before any runtime
  change.

### Options

| Option | What `cas-pool` would use | For | Against |
|---|---|---|---|
| A. OS threads (spec 0015 D5, the status quo) | `std::thread`, `std::sync`, `rustix` syscalls; one thread per slot plus a control thread | Matches the workload (F7): blocking I/O, few slots (F8). No new dependency. The sandbox launcher already works this way (0010, 0011). Simplest to reason about with `pre_exec`. | Hand-written channels and lifecycle. No shared timer or cancellation primitives. |
| B. Tokio (already in the workspace, F1, F3) | `tokio` multi-thread runtime; blocking work through `spawn_blocking` | One runtime for pool, router, ebpfd and deck. Timers, cancellation, `AsyncFd` for the ring buffer already used by `cas-ebpfd`. Same stack as cortex (F6). | `spawn_blocking` for every gix and cgroup call adds a thread pool anyway. `pre_exec` and fork from an async worker need care. |
| C. heph thread-per-core actors (research design) | `heph` 0.4, `heph-rt` 0.4 on io_uring | Share-nothing actors per slot. io_uring is available on ceres (F4). | Second runtime next to Tokio (F1). Linux only. Not pinned, not approved (F5, spec 0004 rule). No estate code uses it. The workload is not the one it is built for (F7, F8). The research verdict itself keeps Tokio (F6). |

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | Recommendation of this spec: **Option A** for the pool's slot lifecycle and control loop, as spec 0015 D5 already states; **Option B** only where a crate serves a socket or HTTP endpoint (router, ebpfd, acp, deck). No heph. | CAS lane w25:p1, 2026-10-10, pending the operator |
| D2 | The runtime for `cas-pool`: A, B or C. | **the operator to rule** |
| D3 | If D2 is A or B, spec 0001 gap 1 and Plan item 3 close, and 0015 D5 gains the citation "ruled in spec 0019". If D2 is C, spec 0004 gets the `heph` pin and licence row first, and a measurement plan precedes code. | Follows D2 |
| D4 | Any later runtime change needs a measurement recorded in a new spec (the Unwanted-behavior requirement above). A criterion benchmark of the slot lifecycle (lease, launch, freeze, thaw, reclaim) under Option A is the first such measurement, and it is the evidence a move to C would need (F9). | CAS lane, 2026-10-10 |

### What does not change with the ruling

- `cas-acp` (spec 0016) serves JSON-RPC to editors and runs on Tokio under every
  option.
- The clamp and the audit path (spec 0014) are syscalls and one socket client; they
  work from a plain thread or from `spawn_blocking`.
- `cas-domain` stays pure types and zero I/O.

## Plan

- [ ] the operator rules D2.
- [ ] Record the ruling in spec 0001 (gap 1, Plan item 3) and spec 0015 D5.
- [ ] If C: pin `heph` and `heph-rt` in spec 0004 and write the measurement plan.

## Test

```bash
make spec-check
grep -L 'tokio\|heph' crates/cas-domain/Cargo.toml   # prints the path: domain stays runtime-neutral
```

The second command prints `crates/cas-domain/Cargo.toml` when the file names
neither runtime. An empty result means the invariant broke.

## Notes

- The research design (`agentic-systems/designs/rust-native-agent-substrate.md`
  section 3.1) draws the pool as actors. Spec 0001 gap 8 applies: the design is a
  draft until measured, and this spec treats it as intent.
- `io_uring_disabled = 0` is a fact about ceres today; a hardened host may set it
  to 2, which would break Option C there.
