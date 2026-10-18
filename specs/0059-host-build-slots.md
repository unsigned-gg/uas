---
status: planned
created: 2026-10-18
tags: [pool, slots, arbitration, builds, cli, ceres]
priority: high
depends_on: [0002-verification-gates-and-evidence, 0020-build-job-cap, 0036-slot-arbitration-across-processes]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0059-host-build-slots

> **Status**: planned, **Priority**: high, **Created**: 2026-10-18

## Overview

Every agent seat on ceres runs its own `cargo` builds. Each one honours `-j 2` (AGENTS.md
rule 2), but nothing bounds how many builds run at once across seats. Spec 0036 built a
host-wide slot arbiter in `uas-pool`. No command lets a seat take a slot for a build.

Facts:

- **F1 [verified 2026-10-18]:** `uas_pool::slot::arbitration::HostSlotArbiter` takes a host slot
  with `try_acquire` or `acquire(policy, timeout)` and holds it while the lease lives. Source:
  `crates/uas-pool/src/slot/arbitration.rs`. Spec 0036 is `in-progress`.
- **F2 [verified 2026-10-18]:** spec 0036 Plan item "Set the default slot directory for
  production `SlotPool` callers" is open. No production caller takes a host slot outside the
  operator.
- **F3 [verified 2026-10-18, about 07:55Z]:** host load was 10.12 on 16 CPUs (spec 0058 F4). The
  same day the lane's own gates waited for load to fall under 12 several times.

This spec adds `uas build -- <command>`. It holds one host slot from the spec 0036 arbiter for
as long as the command runs.

## Design

### Decisions

| # | Decision | Rationale | Source |
|---|---|---|---|
| D1 | `uas build` takes one host slot through `HostSlotArbiter`, runs the command as a child with inherited stdio, waits for it, then drops the lease. It writes no cgroup file and sets no limit. | A slot count bounds concurrent builds without confinement. Confinement of seats is out of scope for this lane (operator ruling 2026-10-18). | lane, 2026-10-18 |
| D2 | The slot directory is `/tmp/uas-slots` (spec 0036 D2). `UAS_SLOT_DIR` overrides it, and `--slot-dir` overrides both. `--host-slots` sets `host_max_slots` (default 2, spec 0036 D1). | One canonical directory, so every caller shares one ceiling. | spec 0036 D1, D2 |
| D3 | `uas build` waits under `WaitBounded`, with `--wait <seconds>` (default 600). `--try` does not wait. When no slot frees in time, it exits 75 (`EX_TEMPFAIL`) and does not start the command. | A build queue on a shared host is normal. A distinct exit status lets a caller retry. | lane, 2026-10-18 |
| D4 | `uas build` sets `CARGO_BUILD_JOBS=2` in the child environment only when the caller has not set it. | Spec 0020 and AGENTS.md rule 2. The caller's explicit value wins. | spec 0020 |
| D5 | `uas build` exits with the child's exit status. A child ended by a signal gives `128 + signal`. | Shell convention, so scripts and agents read it as they read cargo's status. | lane, 2026-10-18 |
| D6 | Signals are not forwarded. The child runs in the caller's process group, so a terminal interrupt reaches it directly. If `uas build` itself is killed, the kernel frees the slot (spec 0036 D3) while the child may keep running. To stop both, signal the process group. | Forwarding needs a signal-handling crate. That cost is not justified before adoption is decided. | lane, 2026-10-18 |
| D7 | Cache retention (spec 0047) is not part of this spec. | Spec 0047 D2 and D11 await the operator. | spec 0047 Notes |

### Requirements (EARS)

- **[REQ-0059-01]** (Event-driven): WHEN `uas build -- <command>` starts, it shall take one host slot through the spec 0036 arbiter before it starts the command.
- **[REQ-0059-02]** (State-driven): WHILE the command runs, the `uas build` command shall hold the slot, and it shall release the slot when the command exits.
- **[REQ-0059-03]** (Ubiquitous): The `uas build` command shall exit with the command's exit status, or with `128 + signal` when a signal ended the command.
- **[REQ-0059-04]** (Unwanted): IF no host slot frees within the wait limit, THEN `uas build` shall exit with status 75 and shall not start the command.
- **[REQ-0059-05]** (Optional): WHERE `--try` is given, `uas build` shall not wait for a slot.
- **[REQ-0059-06]** (Ubiquitous): The `uas build` command shall set `CARGO_BUILD_JOBS=2` for the command only when the caller's environment lacks it.
- **[REQ-0059-07]** (Ubiquitous): The `uas build` command shall write one line to stderr that names the slot index it holds and how long it waited.
- **[REQ-0059-08]** (Ubiquitous): The `uas build` command shall write no cgroup file and shall set no resource limit.

## Plan

- [ ] Add the `uas build` subcommand to `uas-cli` on top of `HostSlotArbiter`, with `--slot-dir`, `--host-slots`, `--wait` and `--try`.
- [ ] Add tests for REQ-01 to REQ-08 with a temporary slot directory and helper commands such as `sh -c 'exit 3'`.
- [ ] Add a multi-process test: with `--host-slots 1`, a second `uas build --try` exits 75 while the first holds the slot.
- [ ] Tick spec 0036's default-directory Plan item only if `SlotPool` callers also adopt the default. Otherwise leave it, and point to this spec from 0036 Notes.

## Test

```bash
cargo test -p uas-cli -j 2 --test build_slots
```

A pass requires each of these negative controls to exist and to go red when its own check is
removed:

| Case | Requirement |
|---|---|
| the command does not start until a slot is held | REQ-0059-01 |
| a second build with one host slot waits, then gets the slot after the first exits | REQ-0059-02 |
| `exit 3` gives 3; a child killed by SIGTERM gives 143 | REQ-0059-03 |
| a full host with `--wait 1` exits 75, and the command never runs | REQ-0059-04 |
| `--try` on a full host exits 75 at once | REQ-0059-05 |
| an unset `CARGO_BUILD_JOBS` becomes 2; a caller value of 5 stays 5 | REQ-0059-06 |
| stderr names the slot index | REQ-0059-07 |
| a source scan finds no cgroup path and no limit write in the build module | REQ-0059-08 |

## Notes

- Adoption is a separate decision: seat rules or wrappers that route `cargo` through `uas build`.
  This spec only provides the command.
- Spec 0036's `capacity.json` clamp is still open. Until it lands, callers that disagree on
  `--host-slots` each enforce their own value.
