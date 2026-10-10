---
status: planned
created: 2026-10-10
tags: [pool, cgroup, memory, pids, disk, limits, phase-3]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0020-build-job-cap, 0028-branchless-crdt-worktrees, 0036-slot-arbitration-across-processes, 0042-cas-binary-conventions]
---

# 0041-slot-memory-pids-and-disk-limits

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

Spec 0020 caps a slot's CPU with `cpu.max` and says in so many words that memory is out of
its scope. Spec 0028 names disk exhaustion and memory starvation on ceres as the reason for
Phase 8, which runs last. Between those two, nothing bounds what one worker can allocate,
fork or write today [reported: integrating lane w25:p1 gap analysis, 2026-10-10]. The code
has fields for a memory limit and never sets them.

This spec gives every slot a memory ceiling, a task ceiling and a disk floor, and says what
the pool records when the kernel enforces one.

### Facts

Measured 2026-10-10 on ceres (WSL2 kernel 6.6.87.2) at development `bd31291`.

- **F1 [verified]:** `ResourceLimits` has `cpu_quota_us`, `cpu_period_us`, `memory_high_bytes` and `memory_max_bytes`, and no pids field. Its `Default` sets the CPU pair and leaves both memory fields `None` (`crates/cas-pool/src/freezer.rs` lines 79-99). Command: `grep -nE 'pids_max|pids\.max' crates/cas-pool/src/freezer.rs` prints nothing.
- **F2 [verified]:** `configure_limits` writes `memory.high` and `memory.max` when the fields are set (`freezer.rs` lines 279 to 296). So the knob exists and is unused.
- **F3 [verified]:** The delegated subtree for uid 1000 on ceres exposes `memory.max`, `pids.max`, `memory.events` and `cgroup.kill`. Command: `C=/sys/fs/cgroup$(cut -d: -f3 /proc/self/cgroup); ls $C | grep -E '^(cgroup\.kill|memory\.events|memory\.max|pids\.max)$'` prints all four. Spec 0020 F3 verified the same delegation for `cpu memory pids`.
- **F4 [verified]:** ceres has 54 GiB of RAM and 302 GiB free of 1007 GiB on `/`. Commands: `free -g`, `df -h /home`.
- **F5 [verified]:** A clean checkout of this repo is 3.4 MiB without `.git` and `target`; one debug build of `cas-cli` alone makes `target` 246 MiB. Commands: `du -sh --exclude=.git --exclude=target .`, `du -sh target`. Disk per slot is build output, not source.
- **F6 [reported]:** Spec 0020 F7: `pids.max` is the wrong knob for a job cap because a build with many legitimate processes would fail. It is the right knob for a runaway fork.
- **F7 [verified]:** `reset_clean` removes every entry of the worktree directory except `.git`, ignored files included, then checks the tree out again (`crates/cas-pool/src/worktree.rs` lines 190-208); spec 0015 line 74 names that step. So `target/` goes at `Reclaiming` when it sits inside the worktree.
- **F8 [reported]:** Spec 0036 D1: at most two host slots on ceres.
- **F9 [verified]:** ceres has 20 GiB of swap. Command: `free -g`, the `Swap:` line.

## Design

The limits are cgroup v2 attributes of the slot cgroup `/sys/fs/cgroup/cas/slot-<id>` (spec
0015 D3), written by `cas-pool` before the first `exec` in the slot, like `cpu.max` (spec 0020
D3). The kernel enforces them; the pool observes `memory.events` and reports.

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | `ResourceLimits::default()` sets `memory_max_bytes` to one quarter of `MemTotal` read from `/proc/meminfo`, with a floor of 2 GiB and a ceiling of 16 GiB, and `memory_high_bytes` to 80 percent of that, and writes `memory.swap.max` as 0. On ceres that is `memory.max` 13.5 GiB and `memory.high` 10.8 GiB per slot, with no swap. | Two slots (F8) at a quarter each leave half the host for the harnesses, the editor and the kernel page cache. `memory.high` throttles before `memory.max` kills. Without `memory.swap.max` 0 the ceiling is a trigger for swapping (F9), and a swapping slot thrashes the disk every other slot uses. | proposed: integrating lane w25:p1, 2026-10-10; the fraction awaits Christian |
| D2 | `ResourceLimits` gains `pids_max: Option<u32>`, default `4096`, written to `pids.max`. | F6: a fork bomb is a slot's failure, not the host's. 4096 clears any `cargo` build with `-j 2`. | integrating lane w25:p1, 2026-10-10 |
| D3 | Disk has no per-slot quota. Project quotas need a filesystem mount option and root; the slot's output is reclaimed by spec 0015 at `Reclaiming` (F7). Instead the pool keeps a floor: a slot is not provisioned when the worktree root's filesystem has less than `min_free_bytes` (default 20 GiB) free, and `allocate` returns `PoolError::DiskLow`. | A quota would be enforced by a mount the operator has not made. A floor is enforceable from user space with `statvfs`. | integrating lane w25:p1, 2026-10-10 |
| D4 | The limits are written before the first `exec` in the slot, in the same call as `cpu.max`. A missing controller file logs at `warn` and leaves that one limit unset, as `cpu.max` does today (`freezer.rs` lines 272-273). | One write path, one failure mode. | integrating lane w25:p1, 2026-10-10 |
| D5 | At `Reclaiming` the pool reads `memory.events` and `pids.events` of the slot cgroup. A non-zero `oom_kill` makes the worker outcome `Killed(Oom)`; a non-zero `pids.events max` makes it `Killed(Pids)`. Both are distinct from a non-zero exit and are recorded on the `AuditReceipt` with the counts. | A verifier reading `exit 137` cannot tell an OOM from an operator kill. The receipt must say which (spec 0002 evidence tiers). | integrating lane w25:p1, 2026-10-10 |
| D6 | Limits are per slot, not per worker process: a slot's whole process tree shares one `memory.max` and one `pids.max`. | The cgroup is the slot; spec 0015 D3. | integrating lane w25:p1, 2026-10-10 |
| D7 | Operators override every default with `PoolConfig` fields and the environment twins `CAS_SLOT_MEMORY_MAX`, `CAS_SLOT_PIDS_MAX` and `CAS_MIN_FREE_BYTES` (bytes, decimal). Precedence follows spec 0042. | One way to tune, named in one place. | integrating lane w25:p1, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0041-01]** (Event-driven): WHEN `cas-pool` configures a slot cgroup, it shall write `memory.max`, `memory.high`, `memory.swap.max` and `pids.max` before the first `exec` in that slot.
- **[REQ-0041-02]** (Ubiquitous): The `cas-pool` default `memory_max_bytes` shall be one quarter of `MemTotal`, clamped to the range 2 GiB to 16 GiB, the default `memory_high_bytes` shall be 80 percent of `memory_max_bytes`, and the default `memory.swap.max` shall be 0.
- **[REQ-0041-03]** (Ubiquitous): The `cas-pool` default `pids_max` shall be 4096.
- **[REQ-0041-04]** (Unwanted): IF the filesystem holding the worktree root has less than `min_free_bytes` free, THEN `cas-pool` shall refuse `allocate` with `PoolError::DiskLow` and provision nothing.
- **[REQ-0041-05]** (Unwanted): IF a limit's controller file is absent, THEN `cas-pool` shall log the missing file at `warn`, leave that limit unset, and continue.
- **[REQ-0041-06]** (Event-driven): WHEN a slot enters `Reclaiming`, `cas-pool` shall read `memory.events` and `pids.events` of the slot cgroup and record `oom_kill` and `max` on the `AuditReceipt`.
- **[REQ-0041-07]** (Unwanted): IF `oom_kill` is non-zero, THEN the worker outcome shall be `Killed(Oom)`, and IF `pids.events max` is non-zero, THEN the worker outcome shall be `Killed(Pids)`, in both cases distinct from `Exited(code)`.
- **[REQ-0041-08]** (Ubiquitous): The `cas-pool` crate shall apply memory and pids limits to the slot cgroup as a whole, never to an individual worker process.
- **[REQ-0041-09]** (Ubiquitous): The `cas-domain` crate shall define the `Killed(Oom)` and `Killed(Pids)` outcomes and the receipt fields with no I/O imports.

## Plan

- [ ] `cas-pool`: `pids_max` on `ResourceLimits`; `MemTotal` read and the clamp in `Default`.
- [ ] `cas-pool`: write `pids.max` in `configure_limits`; read `memory.events` and `pids.events` at reclaim.
- [ ] `cas-pool`: `statvfs` floor in `allocate`; `PoolError::DiskLow`.
- [ ] `cas-domain`: `WorkerOutcome::Killed(KillReason)` and the two receipt counters.
- [ ] Spec 0015: add the two receipt fields to the `AuditReceipt` row; spec 0020: cite this spec where it says memory is out of scope.

## Test

| Requirement | Negative Control |
|---|---|
| REQ-0041-01 | A fake freezer records writes; the test asserts `memory.max`, `memory.high` and `pids.max` are written before `exec`. Goes red if any write is removed. |
| REQ-0041-02 | A unit test feeds `MemTotal` of 4 GiB, 54 GiB and 256 GiB and expects 2 GiB, 13.5 GiB and 16 GiB. Goes red if the clamp is dropped. |
| REQ-0041-04 | A fake `statvfs` reports 1 GiB free; `allocate` returns `DiskLow`. Goes red if the check is removed. |
| REQ-0041-07 | Under `make ebpf-test`, a worker runs `python3 -c "x=bytearray(2**31)"` in a slot with `memory.max` 256 MiB; the receipt shows `oom_kill 1` and `Killed(Oom)`. Goes red if the outcome is `Exited(137)`. |

```bash
cargo nextest run -p cas-pool limits -j 2
cargo nextest run -p cas-domain outcome -j 2
```

The live OOM and pids cases run under `make ebpf-test` on a workstation and print a skip
where `/sys/fs/cgroup/cas` cannot be created.

## Notes

- `memory.high` makes the kernel reclaim and throttle a slot before it kills it. A build that touches `memory.high` runs slower; it does not fail. Only `memory.max` kills.
- Swap: ceres has 20 GiB of it (F9). D1 sets `memory.swap.max` to 0 so a slot cannot push the host into swap and so the REQ-0041-07 test can go red. A measured build that needs swap is a change to D1, not an exception.
- Disk accounting per slot (a `du` at reclaim on the receipt) is cheap and useful but is a receipt change; it rides with the spec 0015 amendment in the Plan.
- Phase 8 (spec 0028) replaces the worktree on disk for transient tasks. The limits of this spec still apply to the slot cgroup; only D3 changes shape.
