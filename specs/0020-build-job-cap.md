---
status: planned
created: 2026-10-10
tags: [pool, cgroup, cpu-max, job-cap, nice, phase-3]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0002-verification-gates-and-evidence, 0014-ebpf-priority-clamping-and-audit-probes, 0015-slot-pool-cgroup-freezer-and-gitoxide]
---

# 0020-build-job-cap

## Overview

Spec 0001 open gap 2: the research says eBPF enforces two parallel jobs on shared
hosts, but the mechanism it describes sets `nice 19`, which is a priority, not a
cap on the number of jobs. Spec 0001 Plan item 4 asks for the spec that designs a
job cap for builds, or drops the claim. This spec does both: it drops the eBPF
claim and designs the cap that exists in the kernel, a cgroup v2 CPU quota per
slot, on top of the `-j 2` convention the repos already follow.

Facts, measured on ceres 2026-10-10 unless stated:

- **F1 [verified]:** `nice 19` is a scheduling priority (spec 0014 D4). It changes
  who runs first when CPUs are contended. It does not bound how many processes or
  threads a worker starts. Measured in spec 0014 T8: a 4-task process at nice 19
  still runs 4 tasks.
- **F2 [verified]:** the `-j 2` convention is advisory: `Makefile` sets `JOBS ?= 2`
  and passes it to every cargo call (AGENTS.md rule 2). Nothing stops a worker
  from running `cargo build -j 16` or `make -j`.
- **F3 [verified]:** cgroup v2 is the unified hierarchy on ceres. The user slice
  `/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/app.slice` has
  `cpu memory pids` in `cgroup.subtree_control`. An unprivileged process (uid 1000)
  created a child cgroup there, wrote `50000 100000` to its `cpu.max`, read the same
  value back, and removed it. Command: `mkdir $C/probe && echo "50000 100000" >
  $C/probe/cpu.max && cat $C/probe/cpu.max && rmdir $C/probe`. The agy pane
  (w25:p3) reported the same result the same day.
- **F4 [verified]:** `cpu.max` on `user-1000.slice` and on `app.slice` is
  `max 100000`: no quota above the slot today. `nproc` is 16.
- **F5 [verified]:** `cas-pool` already writes the quota. `crates/cas-pool/src/freezer.rs`
  `CgroupFreezer::configure_limits` (development aee639c, PR #36) writes `cpu.max`
  as `<quota_us> <period_us>` from `ResourceLimits { cpu_quota_us, cpu_period_us,
  memory_high_bytes, memory_max_bytes }`, with `period` defaulting to 100000 and
  quota `max` when unset. `crates/cas-pool/tests/freezer.rs`
  `cgroup_freezer_creates_child_directory_and_writes_limits` asserts the written
  line.
- **F6 [verified]:** the same function skips each file silently when it does not
  exist (`if cpu_max_path.exists()`). A slot whose cgroup lacks the `cpu`
  controller runs uncapped and nothing records it.
- **F7 [verified]:** `pids.max` is delegated too (F3). It caps the number of tasks
  in the cgroup, not CPU time. A build with two jobs and a thread pool of 16 is
  legitimate, so `pids.max` is the wrong knob for a job cap.

## Design

### Requirements (EARS)

- **Ubiquitous:** The substrate shall state the host throttling invariant as two
  parts: the `-j 2` convention for every cargo and make call, and a CPU quota on
  every worker slot cgroup. Neither part is enforced by eBPF.
- **Event-driven:** WHEN `cas-pool` launches a worker slot, it shall write
  `cpu.max` on the slot cgroup before the first `exec` in that slot.
- **Event-driven:** WHEN the slot cgroup has no `cpu` controller, `cas-pool` shall
  record `cpu_cap: uncapped` in the slot's receipt and log it at warn level; it
  shall not fail the launch.
- **State-driven:** WHILE a slot is capped, every process in its process tree
  shall share the quota, since cgroup membership is inherited across fork and exec.
- **Unwanted behavior:** IF a worker raises its own job count above the
  convention, THEN the quota shall bound the CPU time it can consume; no further
  mechanism shall be added without a measurement.
- **Optional feature:** WHERE the host runs a single worker and no human session,
  the operator may set the quota to `max`.

### The cap

| Part | Mechanism | Enforced by | Scope |
|---|---|---|---|
| Convention | `JOBS ?= 2`, `cargo ... -j $(JOBS)`, `make -j 2` in every brief | Makefiles, AGENTS.md rule 2, builder briefs | per command; advisory |
| Quota | `cpu.max = <2 * period> <period>`, default `200000 100000` (two CPUs' worth of time per 100 ms period) on `/sys/fs/cgroup/.../cas/slot-<id>` | the kernel's CFS bandwidth controller | the whole slot process tree; enforced |
| Priority | `nice 19` via `PRIO_PGRP` (spec 0014) | the scheduler | who yields under contention; not a cap |

Two CPUs of quota is the arithmetic twin of `-j 2`: two jobs that each use one CPU
fit exactly; a worker that starts sixteen gets the same total time and slows
itself, not the host. The quota is a ceiling on CPU time; it does not stop a
process from blocking on I/O or from allocating memory (`memory.high` and
`memory.max`, already in `ResourceLimits`, cover that and are out of scope here).

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | The claim "eBPF enforces `-j 2`" is dropped. Spec 0001 gap 2 closes with this spec. | CAS lane w25:p1, 2026-10-10 |
| D2 | The job cap is the cgroup v2 CPU quota per slot, written by `CgroupFreezer::configure_limits`, plus the `-j 2` convention. | CAS lane w25:p1, 2026-10-10 |
| D3 | Default quota per slot: `200000 100000` (two CPUs). Alternatives considered: one CPU (`100000 100000`) for a crowded host; `max` for a dedicated one. | the operator, interview in lane w25:p1, 2026-10-10: two CPUs |
| D4 | A missing `cpu` controller is recorded, not fatal: `cpu_cap: uncapped` in the receipt, a warn log. This replaces the silent skip in F6. | CAS lane w25:p1, 2026-10-10 |
| D5 | The quota applies to the slot, not to builds alone. The kernel cannot tell a build from an editor; the slot is the unit of isolation. | CAS lane w25:p1, 2026-10-10 |

### Receipt field

The slot receipt (spec 0015 `AuditReceipt` carrier) gains `cpu_cap`, one of
`"200000 100000"` (the written line), `"max"`, or `"uncapped"` (controller absent).
Saga reads it as evidence that the host throttling invariant held for the run.

## Plan

- [x] the operator rules D3 (two CPUs, 2026-10-10).
- [ ] `cas-pool`: default `ResourceLimits` to `cpu_quota_us: 200_000, cpu_period_us: 100_000` at slot launch (today
  the caller passes limits; the default is `None`, which writes nothing).
- [ ] `cas-pool`: replace the silent skip (F6) with the `uncapped` receipt field and
  a warn log; test goes red first on a cgroup without the `cpu` controller
  (a tempdir stands in for the cgroup, as `tests/freezer.rs` does today).
- [ ] Spec 0001: close gap 2 and Plan item 4 with a citation to this spec.
- [ ] Spec 0015: cite this spec from D3 (cgroup path) and the receipt.

## Test

```bash
make spec-check
cargo nextest run -p cas-pool --build-jobs 2 cgroup_freezer_creates_child_directory_and_writes_limits
```

Live, unprivileged, on ceres (proves F3; leaves nothing behind):

```bash
C=/sys/fs/cgroup/user.slice/user-$(id -u).slice/user@$(id -u).service/app.slice
mkdir $C/cas-cap-probe && echo "200000 100000" > $C/cas-cap-probe/cpu.max \
  && cat $C/cas-cap-probe/cpu.max && rmdir $C/cas-cap-probe
```

Expected: `200000 100000` printed, exit 0. A host that prints a permission error
has no delegated `cpu` controller, which is the `uncapped` path.

## Notes

- `cpu.max` throttling is visible in `cpu.stat` (`nr_throttled`, `throttled_usec`)
  of the slot cgroup; a later measurement spec can read it into the receipt.
- The quota is per slot. Two slots on one host may use four CPUs together; the
  pool's slot count (spec 0015) is the host-level bound.
