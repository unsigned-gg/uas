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

- **F1 [verified]:** `ResourceLimits` has `cpu_quota_us`, `cpu_period_us`, `memory_high_bytes` and `memory_max_bytes`, and no pids field. Its `Default` sets the CPU pair and leaves both memory fields `None` (`crates/uas-pool/src/freezer.rs` lines 79-99). Command: `grep -nE 'pids_max|pids\.max' crates/uas-pool/src/freezer.rs` prints nothing.
- **F2 [verified]:** `configure_limits` writes `memory.high` and `memory.max` when the fields are set (`freezer.rs` lines 279 to 296). So the knob exists and is unused.
- **F3 [verified]:** The delegated subtree for uid 1000 on ceres exposes `memory.max`, `pids.max`, `memory.events` and `cgroup.kill`. Command: `C=/sys/fs/cgroup$(cut -d: -f3 /proc/self/cgroup); ls $C | grep -E '^(cgroup\.kill|memory\.events|memory\.max|pids\.max)$'` prints all four. Spec 0020 F3 verified the same delegation for `cpu memory pids`.
- **F4 [verified]:** ceres has 54 GiB of RAM and 302 GiB free of 1007 GiB on `/`. Commands: `free -g`, `df -h /home`.
- **F5 [verified]:** A clean checkout of this repo is 3.4 MiB without `.git` and `target`; one debug build of `uas-cli` alone makes `target` 246 MiB. Commands: `du -sh --exclude=.git --exclude=target .`, `du -sh target`. Disk per slot is build output, not source.
- **F6 [reported]:** Spec 0020 F7: `pids.max` is the wrong knob for a job cap because a build with many legitimate processes would fail. It is the right knob for a runaway fork.
- **F7 [verified]:** `reset_clean` removes every entry of the worktree directory except `.git`, ignored files included, then checks the tree out again (`crates/uas-pool/src/worktree.rs` lines 190-208); spec 0015 line 74 names that step. So `target/` goes at `Reclaiming` when it sits inside the worktree.
- **F8 [reported]:** Spec 0036 D1: at most two host slots on ceres.
- **F9 [verified]:** ceres has 20 GiB of swap. Command: `free -g`, the `Swap:` line.

## Design

The limits are cgroup v2 attributes of the slot cgroup `/sys/fs/cgroup/uas/slot-<id>` (spec
0015 D3), written by `uas-pool` before the first `exec` in the slot, like `cpu.max` (spec 0020
D3). The kernel enforces them; the pool observes `memory.events` and reports.

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | The pool's default `memory_max_bytes` is `MemTotal`, read from `/proc/meminfo`, divided by twice `host_max_slots` (spec 0036 D1): half the host stays reserved and the rest is split by the host slot count. There is no ceiling. The floor is 2 GiB and is a refusal, not a clamp: when the formula gives less, `allocate` returns the new `PoolError::MemoryLow`. `memory_high_bytes` is 80 percent of the max, and `memory.swap.max` is written as 0. On ceres (54 GiB, F4; 2 slots, F8) that is `memory.max` 13.5 GiB and `memory.high` 10.8 GiB per slot, with no swap. | The reserved half covers the harnesses, the editor and the kernel page cache. Splitting by `host_max_slots` follows spec 0036 instead of assuming two slots. `memory.high` throttles before `memory.max` kills. Without `memory.swap.max` 0 the ceiling is a trigger for swapping (F9), and a swapping slot thrashes the disk every other slot uses. | ruled: Christian, lane w25:p1, 2026-10-11 about 00:10Z (the formula, the 2 GiB floor as a refusal, no ceiling); first proposed by integrating lane w25:p1, 2026-10-10 as a quarter clamped to 2 to 16 GiB |
| D2 | `ResourceLimits` gains `pids_max: Option<u32>`, default `4096`, written to `pids.max`. | F6: a fork bomb is a slot's failure, not the host's. 4096 clears any `cargo` build with `-j 2`. | integrating lane w25:p1, 2026-10-10 |
| D3 | Disk has no per-slot quota. Project quotas need a filesystem mount option and root; the slot's output is reclaimed by spec 0015 at `Reclaiming` (F7). Instead the pool keeps a floor: a slot is not provisioned when the worktree root's filesystem has less than `min_free_bytes` (default 20 GiB) free, and `allocate` returns `PoolError::DiskLow`. | A quota would be enforced by a mount the operator has not made. A floor is enforceable from user space with `statvfs`. | integrating lane w25:p1, 2026-10-10 |
| D4 | The limits are written before the first `exec` in the slot, in the same call as `cpu.max`. A missing controller file logs at `warn` and leaves that one limit unset, as `cpu.max` does today (`freezer.rs` lines 272-273). | One write path, one failure mode. | integrating lane w25:p1, 2026-10-10 |
| D5 | At `Reclaiming` the pool reads `memory.events` and `pids.events` of the slot cgroup and records `oom_kill` and `pids_max_hits` on the `AuditReceipt`. A non-zero `oom_kill` makes the worker outcome `Killed(Oom)`, distinct from `Exited(code)`. A non-zero `pids.events max` changes no outcome: the kernel kills nothing at `pids.max`, it fails `fork` with `EAGAIN`, so the worker's own exit code stands and the counter says why. | A verifier reading `exit 137` cannot tell an OOM from an operator kill; the receipt must say which (spec 0002). `pids.max` is a refusal, not a kill (cgroup v2 documentation; research lane, mesh 9d7c6d4b). | integrating lane w25:p1, 2026-10-10 |
| D6 | Limits are per slot, not per worker process: a slot's whole process tree shares one `memory.max` and one `pids.max`. | The cgroup is the slot; spec 0015 D3. | integrating lane w25:p1, 2026-10-10 |
| D7 | Operators override every default with `PoolConfig` fields and the environment twins `UAS_SLOT_MEMORY_MAX`, `UAS_SLOT_PIDS_MAX` and `UAS_MIN_FREE_BYTES` (bytes, decimal). Precedence follows spec 0042. Every pool start logs at `info` the `memory_max_bytes` in force and its source: `formula`, `env` or `config` (a D8 `spec.limits` value reaches the pool as `config`). | One way to tune, named in one place. The start log shows which source won. | integrating lane w25:p1, 2026-10-10; the start log ruled: Christian, lane w25:p1, 2026-10-11 about 00:10Z |
| D8 | A slot provisioned through the spec 0038 `WorkerSlot` CRD carries its limits in `spec.limits` (`memory_max_bytes`, `memory_high_bytes`, `pids_max`); `uas-operator` passes them into `PoolConfig`, and they beat the environment twins of D7. A `WorkerSlot` with no `limits` gets the D1 and D2 defaults. | Without them an operator cannot size slots declaratively (plan author, mesh a10df88d). The CRD field is the flag of spec 0042 D1 for a slot. | integrating lane w25:p1, 2026-10-10; the schema row awaits spec 0038's amendment |

### Requirements (EARS)

- **[REQ-0041-01]** (Event-driven): WHEN `uas-pool` configures a slot cgroup, it shall write `memory.max`, `memory.high`, `memory.swap.max` and `pids.max` before the first `exec` in that slot.
- **[REQ-0041-02]** (Ubiquitous): The `uas-pool` default `memory_max_bytes` shall be `MemTotal` divided by twice `host_max_slots`, with no upper clamp, the default `memory_high_bytes` shall be 80 percent of `memory_max_bytes`, and the default `memory.swap.max` shall be 0.
- **[REQ-0041-03]** (Ubiquitous): The `uas-pool` default `pids_max` shall be 4096.
- **[REQ-0041-04]** (Unwanted): IF the filesystem holding the worktree root has less than `min_free_bytes` free, THEN `uas-pool` shall refuse `allocate` with `PoolError::DiskLow` and provision nothing.
- **[REQ-0041-05]** (Unwanted): IF a limit's controller file is absent, THEN `uas-pool` shall log the missing file at `warn`, leave that limit unset, and continue.
- **[REQ-0041-06]** (Event-driven): WHEN a slot enters `Reclaiming`, `uas-pool` shall read `memory.events` and `pids.events` of the slot cgroup and record `oom_kill` and `pids_max_hits` on the `AuditReceipt`.
- **[REQ-0041-07]** (Unwanted): IF `oom_kill` is non-zero, THEN the worker outcome shall be `Killed(Oom)`, distinct from `Exited(code)`; a non-zero `pids_max_hits` shall change no outcome.
- **[REQ-0041-08]** (Ubiquitous): The `uas-pool` crate shall apply memory and pids limits to the slot cgroup as a whole, never to an individual worker process.
- **[REQ-0041-09]** (Ubiquitous): The `uas-domain` crate shall define the `Killed(Oom)` outcome and the two receipt counters with no I/O imports.
- **[REQ-0041-10]** (Event-driven): WHEN a `WorkerSlot` carries `spec.limits`, `uas-operator` shall pass those values into `PoolConfig`, and `uas-pool` shall apply them in place of the defaults and the environment twins.
- **[REQ-0041-11]** (Unwanted): IF no override sets `memory_max_bytes` and `MemTotal` divided by twice `host_max_slots` is below 2 GiB, THEN `uas-pool` shall refuse `allocate` with `PoolError::MemoryLow` and provision nothing.
- **[REQ-0041-12]** (Event-driven): WHEN a pool starts, `uas-pool` shall log at `info` the `memory_max_bytes` in force and its source, one of `formula`, `env` or `config`.

## Plan

- [ ] `uas-pool`: `pids_max` on `ResourceLimits`; the default `memory_max_bytes` from `MemTotal` and `host_max_slots`, `PoolError::MemoryLow`, and the `info` start log of the value and its source.
- [ ] `uas-pool`: write `pids.max` in `configure_limits`; read `memory.events` and `pids.events` at reclaim.
- [ ] `uas-pool`: `statvfs` floor in `allocate`; `PoolError::DiskLow`.
- [ ] `uas-domain`: `WorkerOutcome::Killed(Oom)` and the two receipt counters.
- [ ] Spec 0038: add `spec.limits` to the `WorkerSlot` schema (D8).
- [ ] Spec 0015: add the two receipt fields to the `AuditReceipt` row; spec 0020: cite this spec where it says memory is out of scope.

## Test

| Requirement | Negative Control |
|---|---|
| REQ-0041-01 | A fake freezer records writes; the test asserts `memory.max`, `memory.high` and `pids.max` are written before `exec`. Goes red if any write is removed. |
| REQ-0041-02 | A unit test feeds a fake `MemTotal` of 6 GiB, 54 GiB and 256 GiB with `host_max_slots` 2 and expects a refusal, 13.5 GiB and 64 GiB, with `memory.high` 10.8 GiB and 51.2 GiB for the last two. Goes red if the 16 GiB ceiling is kept: 256 GiB then gives 16 GiB. |
| REQ-0041-04 | A fake `statvfs` reports 1 GiB free; `allocate` returns `DiskLow`. Goes red if the check is removed. |
| REQ-0041-07 | Under `make ebpf-test`, a worker runs `python3 -c "x=bytearray(2**31)"` in a slot with `memory.max` 256 MiB; the receipt shows `oom_kill 1` and `Killed(Oom)`. Goes red if the outcome is `Exited(137)`. A second worker forks past `pids.max` 64; the receipt shows `pids_max_hits` above 0 and the outcome `Exited(code)`. Goes red if the outcome is a `Killed` variant. |
| REQ-0041-11 | A fake `MemTotal` of 6 GiB with `host_max_slots` 2 gives 1.5 GiB; `allocate` returns `MemoryLow` and creates no slot cgroup. Goes red if the floor clamps up to 2 GiB or the check is removed: `allocate` then succeeds. |
| REQ-0041-12 | A test starts a pool with no override, with `UAS_SLOT_MEMORY_MAX` set, and with a `PoolConfig` value; the captured `info` line names `formula`, `env` and `config` in turn. Goes red if the line or its source field is removed. |

```bash
cargo nextest run -p uas-pool limits -j 2
cargo nextest run -p uas-domain outcome -j 2
```

The live OOM and pids cases run under `make ebpf-test` on a workstation and print a skip
where `/sys/fs/cgroup/uas` cannot be created.

## Notes

- `memory.high` makes the kernel reclaim and throttle a slot before it kills it. A build that touches `memory.high` runs slower; it does not fail. Only `memory.max` kills.
- Swap: ceres has 20 GiB of it (F9). D1 sets `memory.swap.max` to 0 so a slot cannot push the host into swap and so the REQ-0041-07 test can go red. A measured build that needs swap is a change to D1, not an exception.
- The research lane's critic pass (mesh 9d7c6d4b) corrected the first draft, which had a `Killed(Pids)` outcome: `pids.max` refuses forks and kills nothing, so D5 now records a counter.
- Disk accounting per slot (a `du` at reclaim on the receipt) is cheap and useful but is a receipt change; it rides with the spec 0015 amendment in the Plan.
- Phase 8 (spec 0028) replaces the worktree on disk for transient tasks. The limits of this spec still apply to the slot cgroup; only D3 changes shape.
