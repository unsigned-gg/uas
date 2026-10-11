---
status: in-progress
created: 2026-10-10
tags: [pool, restart, orphan, worktree, cgroup, flock, phase-3]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0007-lease-rules, 0009-saga-compensation-runner-and-durability, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0020-build-job-cap, 0036-slot-arbitration-across-processes]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0040-pool-restart-and-orphan-reclamation

> **Status**: in-progress, **Priority**: high, **Created**: 2026-10-10

## Overview

`uas-pool` keeps every slot, lease and worktree handle in process memory. When the process
dies, the kernel frees the spec 0036 host slot locks, so capacity recovers. Nothing frees what
the process left on disk and in the cgroup tree: the slot worktree, its `.git/worktrees/<slot>`
control directory, its `refs/heads/<branch>` ref, and its `/sys/fs/cgroup/uas/slot-<id>`
cgroup with any worker still running in it. Spec 0015 names a `Reclaiming` state and spec
0036 names lock release on death, but no spec says what a starting pool does with the remains
of a dead one [reported: integrating lane w25:p1 gap analysis, 2026-10-10].

This spec defines restart reconciliation: a starting pool finds the slot remains that no live
pool owns, kills and removes them, and only then admits work. It also states what a restart
means for leases.

### Facts

Measured 2026-10-10 on ceres (WSL2 kernel 6.6.87.2) at development `bd31291`.

- **F1 [verified]:** `SlotPool` persists nothing. Command: `grep -nE 'serde|fs::' crates/uas-pool/src/slot.rs` prints nothing. The lease ledger is in memory [reported: spec 0007 line 96].
- **F2 [verified]:** `WarmWorktree::provision` creates three things: the worktree directory, `<common_git_dir>/worktrees/<slot_id>` and the branch `refs/heads/<branch_name>` (`crates/uas-pool/src/worktree.rs` lines 69-82). `teardown_inner` removes all three (lines 275-290) and `Drop` calls it (line 331). `Drop` does not run on `SIGKILL`, so a killed pool leaves all three behind.
- **F3 [reported]:** Spec 0036 D3: the slot lock is held by an open descriptor and the kernel releases it when the holder dies. Capacity recovers on its own; disk state does not.
- **F4 [verified]:** A cgroup directory outlives the process that made it, and the delegated subtree on ceres has `cgroup.kill`. Command: `C=/sys/fs/cgroup$(cut -d: -f3 /proc/self/cgroup); ls $C | grep -E '^(cgroup\.kill|memory\.events|pids\.max)$'` prints all three names.
- **F5 [reported]:** Spec 0015 D3 puts the slot cgroup at `/sys/fs/cgroup/uas/slot-<id>`; spec 0015 line 74 has `Reclaiming` restore the index and remove untracked files through `gix`.
- **F6 [verified]:** `SlotPool::sweep` (`crates/uas-pool/src/slot.rs` line 410) only visits slots the running process holds in memory. It cannot see another process's remains.
- **F7 [reported]:** Spec 0006 line 31: IF a worker crashes or restarts, THEN the engine recovers by replaying its event log; spec 0009 runs the compensators. The engine, not the pool, owns the decision to retry a task whose slot vanished.

## Design

Reconciliation belongs to `uas-pool`, which owns the files and cgroups (spec 0001 layering). It
reuses the spec 0036 primitive: ownership is an open advisory lock, never a pid or a timestamp.

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Every provisioned slot carries a marker file `<common_git_dir>/worktrees/<slot_id>/uas-pool.json`. The provisioning pool creates it as `uas-pool.json.tmp` with `O_CREAT \| O_EXCL \| O_CLOEXEC`, takes `File::try_lock` (exclusive) on it, writes `{slot_id, worktree_dir, cgroup, branch, pool_pid, provisioned_at}`, renames it to `uas-pool.json`, and only then creates the cgroup and the worktree. The lock is on the inode and survives the rename. The descriptor stays open for the life of the slot. The metadata is advisory; the lock is the ownership proof. | Same primitive as spec 0036 D3 and D7. A pid in a file lies after a reboot; a lock never does. | integrating lane w25:p1, 2026-10-10 |
| D2 | On start, before the first `allocate`, the pool scans `<common_git_dir>/worktrees/*/uas-pool.json` and `/sys/fs/cgroup/uas/slot-*`. An entry whose marker lock is free is an orphan. An entry whose marker lock is held belongs to a live pool and is left alone. A `uas-pool.json.tmp` whose lock is free is garbage and is removed; one whose lock is held is a slot being provisioned and is left alone. A cgroup with no marker is an orphan, because D1 publishes the marker before the cgroup exists. | A marker with a held lock is the only evidence of a live owner across processes. | integrating lane w25:p1, 2026-10-10 |
| D3 | An orphan is reclaimed in this order: write `1` to `cgroup.kill` of the slot cgroup, which kills its whole subtree; wait for `cgroup.events` to show `populated 0` (bounded by `reclaim_timeout`, default 5 s); `rmdir` every descendant cgroup deepest first, then the slot cgroup, because `rmdir` returns `ENOTEMPTY` while a child cgroup exists; remove the worktree directory; delete the branch ref; remove the worktree control directory last. Each step is idempotent and a missing item is not an error. | Kill before delete: a worker still writing into a directory being removed is the failure mode. A worker that made nested cgroups leaves them behind (research lane, mesh 9d7c6d4b). The control directory goes last because the marker lives in it: a failed earlier step leaves the marker, and D4 retries the entry at the next start. So the file steps differ from `teardown_inner`, which removes the control directory before the branch ref [reported: `uas@2399dda:crates/uas-pool/src/reconcile.rs`, the comment on `reclaim_filesystem`; `worktree.rs` lines 298-317]. | integrating lane w25:p1, 2026-10-10; order amended per PR #140, ruled: integrating lane w25:p1, 2026-10-11 |
| D4 | A reclaim step that fails logs at `warn` with the slot id and the step, and the pool continues with the next orphan. The pool starts with its full capacity. The failed entry is retried at the next start and counted in `ReconcileReport::failed`. | A leftover directory is a disk problem, not a safety problem. Refusing to start would make a crash sticky. | integrating lane w25:p1, 2026-10-10 |
| D5 | Leases do not survive a pool restart. The ledger is in memory by spec 0007, and this spec keeps it so. A holder of a `LeaseToken` from a dead pool gets `DomainError::LeaseNotFound` from the new one. `uas-engine` treats that error on a replayed activity as a lost slot and runs the spec 0009 compensators, then re-admits the task. | Persisting leases would need a durable ledger that no spec owns and that spec 0001 keeps out of `uas-domain`. The engine already replays. | integrating lane w25:p1, 2026-10-10; awaits the operator if a durable ledger is wanted |
| D6 | The scan never follows symlinks, refuses a marker whose `worktree_dir` is outside the pool's worktree root, and removes nothing it did not find under the two roots of D2. | A planted marker must not turn the pool into an arbitrary `rm -rf`. | integrating lane w25:p1, 2026-10-10 |
| D7 | `uas-domain` gains `ReconcileReport { reclaimed, kept, failed }` as a pure type. All I/O stays in `uas-pool`. | Spec 0001: `uas-domain` has zero I/O. | integrating lane w25:p1, 2026-10-10 |
| D8 | A worktree control directory with neither `uas-pool.json` nor `uas-pool.json.tmp` is left alone when it is not empty: it may be a user's own worktree. It counts in none of `reclaimed`, `kept` or `failed`. An empty one is debris of a failed marker creation; it is removed and counted in `reclaimed`. The marker's `cgroup` field holds the relative name `slot-<slot_id>` of spec 0015 D3; reconcile joins it under the cgroup root and refuses a value that is not one path component. | D2 scans markers, so a directory without one is no evidence that a pool made it. `provision` takes no cgroup argument, so it writes the spec 0015 D3 name [reported: plan 0040 finding 6]. | PR #140 deviation, ruled: integrating lane w25:p1, 2026-10-11; test `reconcile_leaves_foreign_control_dir_alone` |

### Requirements (EARS)

- **[REQ-0040-01]** (Event-driven): WHEN `uas-pool` provisions a slot, it shall create the marker file of D1, hold an exclusive advisory lock on it with an `O_CLOEXEC` descriptor for the life of the slot, and never pass that descriptor to a worker.
- **[REQ-0040-02]** (Event-driven): WHEN `uas-pool` starts, it shall run reconciliation over the two roots of D2 before it admits any `allocate` call.
- **[REQ-0040-03]** (Unwanted): IF a marker's lock can be taken with `File::try_lock`, THEN `uas-pool` shall treat that slot as an orphan and reclaim it in the order of D3, removing descendant cgroups deepest first before the slot cgroup, and removing the worktree control directory last, after the branch ref.
- **[REQ-0040-04]** (Unwanted): IF a marker's lock is held by another process, THEN `uas-pool` shall leave that slot's worktree, control directory, branch and cgroup untouched and count it in `ReconcileReport::kept`.
- **[REQ-0040-05]** (Unwanted): IF a slot cgroup under `/sys/fs/cgroup/uas/` has no marker, THEN `uas-pool` shall kill and remove it.
- **[REQ-0040-06]** (State-driven): WHILE an orphan cgroup is populated after `cgroup.kill`, `uas-pool` shall wait at most `reclaim_timeout` (default 5 s) before it records the entry as failed and moves on.
- **[REQ-0040-07]** (Unwanted): IF a reclaim step fails, THEN `uas-pool` shall log the slot id and the step at `warn`, count the entry in `ReconcileReport::failed`, and continue; the pool shall start with its configured capacity.
- **[REQ-0040-08]** (Unwanted): IF a marker names a `worktree_dir` outside the pool's worktree root, or any path in the scan is a symlink, THEN `uas-pool` shall skip it, log it at `warn`, and remove nothing for it.
- **[REQ-0040-09]** (Ubiquitous): The `uas-pool` crate shall not persist leases; after a restart every lease issued by the previous process shall be unknown to the new one.
- **[REQ-0040-10]** (Unwanted): IF a replayed `uas-engine` activity receives `DomainError::LeaseNotFound` for a lease it issued before the restart, THEN `uas-engine` shall run the registered compensators of spec 0009 and re-admit the task through a fresh lease.
- **[REQ-0040-11]** (Ubiquitous): The `uas-domain` crate shall define `ReconcileReport` with no I/O imports.
- **[REQ-0040-12]** (Event-driven): WHEN `uas-pool` provisions a slot, it shall hold the marker lock and have renamed the marker to `uas-pool.json` before it creates the slot cgroup or the worktree directory.

## Plan

- [ ] `uas-domain`: add `ReconcileReport` and the `LostSlot` reason for compensation.
- [ ] `uas-pool`: write and lock the marker in `WarmWorktree::provision`; release it in `teardown_inner`.
- [ ] `uas-pool`: add `SlotPool::reconcile(roots) -> ReconcileReport` and call it from the constructor before any `allocate`.
- [ ] `uas-pool`: add `PoolError::ReconcileRefused` for D6 cases.
- [ ] `uas-engine`: map `LeaseNotFound` on a replayed activity to compensation plus re-admission (D5).
- [ ] Amend spec 0015's state diagram: `[*] --> Reconciling --> Idle` on start.

## Test

| Requirement | Negative Control |
|---|---|
| REQ-0040-03 | A fixture provisions a slot in a child process, the child is killed with `SIGKILL`, a new pool starts on the same roots. Goes red if the worktree directory or the branch still exists after the constructor returns. |
| REQ-0040-04 | A child holds the marker lock and stays alive; a new pool starts. Goes red if the child's worktree is removed, or if `kept` is 0. |
| REQ-0040-12, 04 | A child creates and locks its marker, then sleeps before it creates its cgroup; the parent reconciles during the sleep. Goes red if the child's marker is removed or its slot is counted as reclaimed. |
| REQ-0040-08 | A fixture plants a marker whose `worktree_dir` points at a sibling temp dir with a sentinel file. Goes red if the sentinel is gone. |
| REQ-0040-10 | An engine test replays an activity whose lease the fake ledger no longer knows. Goes red if no compensator runs. |

```bash
cargo nextest run -p uas-pool reconcile -j 2
cargo nextest run -p uas-engine lost_slot -j 2
```

The cgroup steps run under `make pool-live-test` on a workstation (a uid-owned cgroup v2 user
slice, no sudo). Under `make test` they print a skip where that slice is not writable (spec 0005
skip rule); `make pool-live-test` fails on that skip.

## Notes

- `uas-pool` and `SlotPool::sweep` stay the in-process path; this spec adds the cross-process path once at start, not a periodic janitor. Spec 0036 D3 already rejects janitors for locks; the same holds for files.
- A reboot clears `/tmp/uas-slots` (spec 0036) and the cgroup tree but not the worktrees on `/home`; D2 covers that case because the markers survive.
- Out of scope: a durable lease ledger. If the operator wants leases to survive a restart, that is a new spec that keeps `uas-domain` I/O free.
- Parked with evidence: spec 0015 line 62 has `Reclaiming --> [*]: pool_shutdown()` but no start transition; the Plan amends it.
- REQ-0040-05, the scan for slot cgroups with no marker, waits on plan 0040 P7. `SlotPool::create_slot` makes the slot cgroup and the pool writes no marker for it yet, so the scan would kill a live pool's cgroup. No production caller sets the cgroup root of reconcile until P7 lands [reported: `engineering/plans/0040-pool-restart-and-orphan-reclamation.md` finding 2 and P7]. Ruled: integrating lane w25:p1, 2026-10-11.
- Status in-progress, ruled: integrating lane w25:p1, 2026-10-11. Landed on `development`: PR #138 (wave 1: the domain types, the slot marker, the `Reconciling` state) and PR #140 (wave 2: reconcile, cgroup reclaim, the engine's lost-slot path).
