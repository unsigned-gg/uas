---
status: in-progress
created: 2026-10-18
tags: [pool, cgroups, seats, limits, inventory, ceres, observability]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0041-slot-memory-pids-and-disk-limits]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0058-ceres-seat-inventory-and-limit-policy

> **Status**: in-progress, **Priority**: high, **Created**: 2026-10-18

## Overview

Agent seats on ceres are tmux panes. uas does not launch them, so uas confinement does not
reach them. Spec 0041 limits only the slots that uas creates itself. Nothing measures the seats,
and nothing proposes limits for them.

Facts, measured 2026-10-18 about 07:55Z on ceres:

- **F1 [verified]:** each tmux pane runs in its own `tmux-spawn-<uuid>.scope` under
  `user@1000.service`. There were 87 such scopes. Command:
  `ls -d /sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/tmux-spawn-*.scope | wc -l`.
- **F2 [verified]:** none of the 87 scopes had a limit. Every one showed `memory.max`,
  `memory.high` and `cpu.max` as `max`, and `pids.max` as the default 67473. Command:
  `cat .../tmux-spawn-*.scope/memory.max | sort | uniq -c`, and the same for the others.
- **F3 [verified]:** `user@1000.service` has `Delegate=yes` with controllers
  `cpu io memory pids`. Its `cgroup.subtree_control` is `cpu io memory pids`. Command:
  `systemctl show -p Delegate,DelegateControllers user@1000.service`.
- **F4 [verified]:** the host had 16 CPUs, load 10.12, 54 GiB of RAM with 23 GiB available,
  and 12 GiB of swap in use. 35 `claude` processes held 15.2 GiB RSS. Commands:
  `/proc/loadavg`, `free -g`, `pgrep -xc claude`, `ps -eo rss,comm`.
- **F5 [verified]:** an OpenRig seat name equals its tmux session name, for example
  `orch-lead@uas`. `tmux list-panes -a -F '#{pane_pid} #{session_name}'` gives each pane's pid,
  and `/proc/<pid>/cgroup` gives the pid's scope.
- **F6 [reported]:** the OpenRig role map admits a new seat only with at least 16 GiB available
  and a 1-minute load under 12 (estate ROLE-MAP, 2026-10-17).

This spec adds two things:
- a read-only inventory, `uas seats`, which reports each seat scope's usage and limits and the
  host headroom;
- a dry-run limit plan, `uas seats plan`, which prints the limits a policy would set and applies
  none.

## Design

### Decisions

| # | Decision | Rationale | Source |
|---|---|---|---|
| D1 | uas measures and proposes. It never writes a cgroup property onto a running seat scope. Enforcement belongs to seat launch, owned by the OpenRig lanes with the operator's go. A policy uses `MemoryHigh` (throttle) before `MemoryMax`. | Writing onto a running scope mutates another lane's live session. A `MemoryMax` that kills a seat mid-work loses its work. | operator ruling 2026-10-18 (uas lane decision D2) |
| D2 | Discovery reads the cgroup subtree `user.slice/user-<uid>.slice/user@<uid>.service` of the calling uid. It reports every direct child scope, and marks `tmux-spawn-*` scopes as panes. | Seats live there (F1). The calling uid owns the subtree, so no privilege is needed (F3). | lane, 2026-10-18 |
| D3 | Attribution works through tmux: for every tmux server socket of the calling uid, `tmux -S <socket> list-panes -a` gives each pane's pid and session name, and `/proc/<pid>/cgroup` gives its scope. Sockets are the socket entries of `$TMUX_TMPDIR` (else `/tmp`) `/tmux-<uid>/`. A scope keeps every session found, sorted and deduplicated, each with its socket name (the `default` socket is unlabelled). A dead socket is skipped with a recorded reason. Before it starts tmux, the inventory probes each socket with a connect that sends nothing, and a refused connection means no tmux process; every socket is probed, and at most 64 live sockets (`--max-tmux-sockets`) are queried with tmux, `default` first and the rest sorted, with the overflow counted. The cap bounds tmux processes, not probes. The probe connects without blocking (`SOCK_NONBLOCK`), and a socket whose accept backlog is full is skipped as `busy (backlog full)` with no tmux started. The inventory never reads `/proc/<pid>/environ`. | Session name equals seat name (F5). OpenRig tests start their own tmux servers, so one socket misses panes. A process environment can hold credentials. | lane, 2026-10-18 |
| D4 | The headroom gate reads `/proc/loadavg` and `MemAvailable` from `/proc/meminfo`. Defaults: 1-minute load under 12, and at least 16 GiB available. Flags override both. | The same gate the estate uses to admit seats (F6). | lane, 2026-10-18 |
| D5 | Pure types (`SeatSample`, `HeadroomGate`, `SeatLimitPolicy`, `SeatLimitPlan`) live in `uas-domain`. The `/sys` and `/proc` readers live in `uas-pool`, which already parses cgroup files (spec 0041). The tmux call and the commands live in `uas-cli`, which gains a `uas-pool` dependency. | Layering from spec 0001. `uas-domain` stays zero I/O. | lane, 2026-10-18 |
| D6 | `--json` emits one typed document with `schema = "uas.seats.v1"`. An unreadable value is absent, not zero. | Estate wire rule: typed structs, and absent is not the same as defaulted. | lane, 2026-10-18 |
| D7 | The plan policy is a TOML file given by flag. The repository ships no default limit values until the inventory has sampled a normal week on ceres. | Values guessed before measurement become policy by accident. | lane, 2026-10-18 |

### Policy file (wave 2)

`uas seats plan --policy <file.toml>` reads one TOML file. The repository ships no policy with
values (D7). Test fixtures hold values only to test the parser.

```toml
[default]            # optional; every field optional
memory_high = "<size>"
memory_max  = "<size>"
tasks_max   = <integer>
cpu_weight  = <integer 1 to 10000>

[[seat]]             # optional, repeatable; matched by exact tmux session name
session     = "<session name>"
memory_high = "<size>"   # each field overrides the default for this seat only

[host]               # optional
max_attributed_seats = <integer>
max_load_1m          = <number>      # headroom gate overrides
min_mem_available    = "<size>"
```

- A size uses systemd syntax: a decimal number with an optional `K`, `M`, `G` or `T` suffix, in
  powers of 1024 (`4G`, `512M`). The parser returns a typed error for anything else, for a zero
  size, and for an unknown key.
- The merged limits of the default, and of each seat over the default, must pass REQ-0058-12.
- A `[[seat]]` whose session matches no attributed scope is reported as unknown, not ignored.
- `uas seats plan --json` emits `schema = "uas.seats.plan.v1"`. The plan types live in
  `uas-domain`; the TOML read lives in `uas-cli`.

### Requirements (EARS)

- **[REQ-0058-01]** (Ubiquitous): The `uas seats` command shall list every direct child scope of the calling uid's user service cgroup, with `memory.current`, `memory.peak`, `memory.high`, `memory.max`, `pids.current`, `pids.max`, `cpu.max`, `cpu.weight` and the `some avg10` value of `memory.pressure` and `cpu.pressure`.
- **[REQ-0058-02]** (Ubiquitous): The inventory shall perform no write to a cgroup file, to `/proc`, to tmux or to any file.
- **[REQ-0058-03]** (Event-driven): WHEN a tmux pane's pid belongs to a scope, the inventory shall label that scope with the pane's tmux session name, and with every other pane's session name when a scope holds several.
- **[REQ-0058-04]** (Unwanted): IF a scope has no tmux pane, THEN the inventory shall report the scope as unattributed and shall not drop it.
- **[REQ-0058-05]** (Unwanted): IF a cgroup interface file is missing or unreadable, THEN the inventory shall report that field as absent and shall not report zero.
- **[REQ-0058-06]** (Ubiquitous): The inventory shall not open any `/proc/<pid>/environ` file.
- **[REQ-0058-07]** (Ubiquitous): The inventory shall report the 1-minute load, `MemAvailable`, the thresholds in force and whether the headroom gate passes.
- **[REQ-0058-08]** (Optional): WHERE `--json` is given, the inventory shall emit one JSON document with the field `schema` set to `uas.seats.v1`.
- **[REQ-0058-09]** (Unwanted): IF tmux is not installed or its server is not running, THEN the inventory shall list every scope as unattributed and shall state the reason.
- **[REQ-0058-10]** (Ubiquitous): The `uas-domain` crate shall hold the inventory and policy types with zero I/O.
- **[REQ-0058-11]** (Event-driven): WHEN `uas seats plan` runs with a policy file, it shall print, for each attributed seat, the `MemoryHigh`, `TasksMax` and `CPUWeight` values it would set, and it shall apply none of them.
- **[REQ-0058-12]** (Unwanted): IF a policy sets `MemoryMax` without a `MemoryHigh` that is at or below it, THEN `uas seats plan` shall reject the policy.

- **[REQ-0058-13]** (Event-driven): WHEN more than one tmux server socket of the calling uid exists, the inventory shall query each one and label each attribution with its socket name.
- **[REQ-0058-14]** (Unwanted): IF a cgroup interface file is not a regular file or exceeds 64 KiB, THEN the inventory shall report that field as absent.
- **[REQ-0058-15]** (Unwanted): IF a tmux socket refuses a connection, THEN the inventory shall skip it without starting a tmux process.
- **[REQ-0058-16]** (Ubiquitous): The `uas seats plan` command shall print each attributed seat's `memory.peak` and `memory.current` beside the proposed `MemoryHigh`, and shall mark each seat whose peak exceeds it.
- **[REQ-0058-17]** (Event-driven): WHEN the policy sets `max_attributed_seats`, `uas seats plan` shall report the current attributed count against it, and shall say whether the host is over it.
- **[REQ-0058-18]** (Ubiquitous): The `uas seats plan` output shall state that uas applied nothing, and that enforcement belongs to seat launch.

## Plan

Wave 1: inventory (REQ-01 to REQ-10, REQ-13 to REQ-15).

- [x] Add `SeatSample`, `ScopeKind`, `HeadroomGate` and the `uas.seats.v1` document types to `uas-domain`, with serde round-trip tests.
- [x] Add a read-only scope reader to `uas-pool` that takes the cgroup root as an argument, so tests use a fixture tree.
- [x] Add a tmux attribution adapter and the `uas seats [--json]` command to `uas-cli`.
- [x] Add fixture tests for REQ-01 to REQ-09 and REQ-13 to REQ-15 (the CLI tests cover a nested-cgroup pid, a two-pane scope, a non-pane scope, pressure values, several sockets and a dead socket), and a source-scan test for REQ-06 and REQ-10.

Wave 2: dry-run plan (REQ-11, REQ-12 and REQ-16 to REQ-18).

- [x] Add the size newtype, `SeatLimitPolicy` (with its typed parse errors and the REQ-0058-12 check), `SeatLimitPlan` and the `uas.seats.plan.v1` document to `uas-domain`, pure, with unit tests.
- [x] Add `uas seats plan --policy <file.toml> [--json]` to `uas-cli`, reusing the wave 1 inventory, with the no-apply statement, the peak-over-high mark, the unknown-session report and the seat ceiling.
- [x] Add fixture tests for REQ-11, REQ-12 and REQ-16 to REQ-18, and put the plan modules under the source-scan bans.
- [ ] Sample a normal week of inventory on ceres before any limit value is proposed (D7).

## Test

```bash
cargo test -p uas-domain -j 2 seat && cargo test -p uas-pool -j 2 --test seat_inventory && cargo test -p uas-cli -j 2 seats
```

A pass requires each of these negative controls to exist and to go red when its own check is
removed:

| Case | Requirement |
|---|---|
| a fixture scope with no tmux pane appears as unattributed | REQ-0058-04 |
| a missing `memory.peak` file yields an absent field, not 0 | REQ-0058-05 |
| a fixture pane pid maps to its scope and session name | REQ-0058-03 |
| a fake tmux that fails to start leaves every scope listed and unattributed, with a reason | REQ-0058-09 |
| the JSON document carries `schema = "uas.seats.v1"` | REQ-0058-08 |
| the gate fails at load 12.0 and at 15.9 GiB available | REQ-0058-07 |
| two tmux sockets are both queried, each attribution carries its socket name, and a dead socket is skipped with a reason | REQ-0058-13 |
| a stale socket file is skipped with zero tmux spawns, a listening socket is queried, and every socket is probed even past the cap, while the cap bounds tmux spawns, takes `default` first and counts the live overflow | REQ-0058-15 |
| a scope with two panes lists both sessions, sorted and deduplicated | REQ-0058-03 |
| an oversize file, a FIFO and a symlink as interface files give absent fields | REQ-0058-14 |
| a policy with `memory_max` and no `memory_high` at or below it is rejected, for the default and for a seat | REQ-0058-12 |
| a per-seat override beats the default, field by field, and an unlisted seat takes the default | REQ-0058-11 |
| a `[[seat]]` naming a session that no scope holds is reported as unknown | REQ-0058-11 |
| a seat whose `memory.peak` exceeds the proposed `MemoryHigh` is marked, and one at or under it is not | REQ-0058-16 |
| the plan reports the attributed count against `max_attributed_seats` and whether it is over | REQ-0058-17 |
| the human and JSON plan carry the statement that uas applied nothing | REQ-0058-18 |
| a source scan finds no `environ` path and no write call in the inventory modules | REQ-0058-02, REQ-0058-06 |
| `uas-domain` seat types use no `std::fs`, `std::net`, `std::process` or `tokio` | REQ-0058-10 |

Live check on ceres (read-only): `cargo run -q -p uas-cli -j 2 -- seats --json | jq -r .schema`
prints `uas.seats.v1`.

## Notes

- The inventory observes scopes that uas did not create. Spec 0041 governs the slots that uas
  creates. Spec 0040's cgroup-root scan may reuse the wave 1 reader.
- Enforcement at seat launch is a separate plan for the operator, and is not part of this spec.
