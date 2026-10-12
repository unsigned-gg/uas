---
status: planned
created: 2026-10-11
tags: [supervision, signals, pidfd, subreaper, lifecycle, timeout, pool, engine]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0040-pool-restart-and-orphan-reclamation, 0041-slot-memory-pids-and-disk-limits, 0058-agent-harness-providers-and-mock-runtime]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0060-agent-harness-process-supervision-and-signals-coordination

> **Status**: planned · **Priority**: high · **Created**: 2026-10-11

## Overview

Agent harnesses execute arbitrary child processes during tool execution. When an
agent harness crashes, times out, or receives cancellation, orphaned processes
can accumulate as zombies. On shared physical hosts such as ceres, uncollected
processes starve host resources and break concurrency guarantees.

This specification defines the process supervision and signal coordination
protocol for agent harnesses across `uas-pool` and `uas-engine`.

Key capabilities:
- Process subreaper registration to collect orphaned grand-children.
- Process group isolation preventing signal pollution.
- PID recycling immunity using Linux `pidfd` handles.
- Two-phase escalating teardown with configurable graceful drain periods.

## Design

### Requirements (EARS)

- **Ubiquitous:** The slot supervisor shall register as a Linux subreaper before spawning worker processes.
- **Ubiquitous:** The worker execution shall run inside its own isolated process group.
- **Event-driven:** WHEN a session receives cancellation, the supervisor shall send `SIGTERM` to the worker process group.
- **State-driven:** WHILE the graceful shutdown timer runs, the supervisor shall drain completed child processes via non-blocking `waitpid`.
- **Unwanted behavior:** IF worker processes remain active after the timeout expires, THEN the supervisor shall send `SIGKILL` to the process group.
- **Optional feature:** WHERE supported by the kernel, the supervisor shall use `pidfd_open` for race-free signal dispatch.

### Decisions

| ID | Decision | Rationale | Owner |
|---|---|---|---|
| D1 | Register `PR_SET_CHILD_SUBREAPER` in slot supervisor. | Ensures orphaned grandchild processes reparent to the slot supervisor rather than PID 1. | uas-systems |
| D2 | Enforce two-phase shutdown: `SIGTERM` then `SIGKILL`. | Allows harnesses to persist run journals and release locks cleanly before hard kill. | uas-systems |
| D3 | Isolate process groups with `setpgid`. | Prevents signal leakage into the parent and coordinates signals to all child forks. | uas-systems |
| D4 | Prefer `pidfd` signaling over raw PIDs. | Prevents PID recycling vulnerabilities on high-churn multi-agent systems. | uas-systems |
| D5 | Non-blocking reaper loop on slot reclaim. | Prevents zombie accumulation in the system process table. | uas-systems |

## Plan

- [ ] Implement `SubreaperGuard` in `uas-pool::freezer` using `rustix::process::set_subreaper`.
- [ ] Add `pidfd_send_signal` path to `uas_pool::freezer::SignalFreezer`.
- [ ] Wire two-phase escalating teardown in `uas_pool::slot::reclaim_slot`.
- [ ] Add integration test verifying zombie reaping and child tree teardown.
- [ ] Connect graceful cancellation to `uas-engine::wave_exec`.

## Test

```bash
cargo test -p uas-pool --test reclaim -j 2
```

The test validates process tree termination and child reaping. On Linux kernels
with cgroup v2, `cgroup.kill` and subreaper collection ensure zero zombie leaks.
Negative control: removing process group signaling leaves child forks running.

## Notes

- Physical host ceres runs WSL2 Linux kernel 6.6 with full subreaper and `pidfd` support.
- Follows design patterns researched for robust systems agent runtimes.
- Replaces ad-hoc `SIGKILL` loops with deterministic lifecycle coordination.
