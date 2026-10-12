---
status: complete
created: 2026-10-11
tags: [ceres, testing, substrate, cgroup, mock, localhost, validation]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0002-verification-gates-and-evidence, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0041-slot-memory-pids-and-disk-limits, 0058-agent-harness-providers-and-mock-runtime]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0059-ceres-localhost-repeatable-substrate-testing

> **Status**: complete · **Priority**: high · **Created**: 2026-10-11

## Overview

Physical workstation `ceres` executes multi-agent sessions concurrently. Substrate
development requires a repeatable, hermetic localhost testing harness that validates the
entire runtime without external CI runner queues, network dependencies, or sudo access.

This spec defines the `ceres-test` verification target and protocol:
- Zero-privilege execution (unprivileged user scope delegation).
- Zero-I/O domain invariant enforcement.
- Strict physical host CPU/memory throttling (`-j 2` mandatory).
- Complete monorepo crate coverage (`uas-domain`, `uas-sandbox`, `uas-pool`,
  `uas-engine`, `uas-acp`, `uas-router`, `uas-deck`, `uas-cli`).
- Live cgroup v2 subtree delegation (freezer and memory limit enforcement).
- CLI smoke and version protocol verification.

## Design

### Requirements (EARS)

- **Ubiquitous:** The substrate test runner shall execute all integration suites without requiring root privileges or sudo elevation.
- **State-driven:** WHILE running repeatable substrate tests on physical host `ceres`, the runner shall enforce concurrency cap `-j 2` across all cargo invocations to prevent resource starvation.
- **Event-driven:** WHEN cgroup v2 subtree delegation is available, the runner shall execute live memory OOM and freezer lifecycle tests inside an unprivileged systemd user scope.
- **Unwanted behavior:** IF any file in `crates/uas-domain/src` references network, filesystem, process, or async runtime primitives, THEN the boundary verification check shall fail immediately.
- **Optional feature:** WHERE an external CI runner queue is unavailable, developers and autonomous agents shall use `make ceres-test` to achieve verified evidence tier validation locally.

### Decisions

| ID | Decision | Rationale | Owner |
|---|---|---|---|
| D1 | Run live cgroup v2 tests via `systemd-run --user --scope -q -p Delegate=yes`. | Permits nested cgroup control without sudo or elevated capabilities. | uas-architect |
| D2 | Enforce `-j 2` concurrency cap in `scripts/ceres-test.sh`. | Protects shared physical host `ceres` against CPU starvation and memory exhaustion. | uas-architect |
| D3 | Run `make smoke` with full stdout grep consumption. | Prevents broken pipe (`SIGPIPE`) failures during CLI version checks. | uas-engineer |
| D4 | Include all workspace crates in crate integration step. | Guarantees complete workspace health before branch push. | uas-engineer |

## Plan

- [x] Create repeatable localhost test runner script in `scripts/ceres-test.sh`.
- [x] Wire `make ceres-test` target into root `Makefile`.
- [x] Verify toolchain check, spec check, and zero-I/O domain checks.
- [x] Integrate live cgroup v2 test runner `pool-live-test`.
- [x] Add CLI smoke verification step.
- [x] Register spec in `specs/README.md` and validate with `cargo xtask spec-check`.

## Test

```bash
make ceres-test
```

- [x] All 8 workspace crate test suites pass cleanly.
- [x] Live cgroup v2 memory OOM kill and freezer tests verified.
- [x] Domain zero-I/O boundary verified.
- [x] CLI version protocol and demo run verified.
- [x] Repository hygiene and pin checks pass.

## Notes

The `ceres-test` target provides a complete localhost alternative to the GitHub Actions
`rust` CI workflow. It runs in under 45 seconds on host `ceres` and proves end-to-end
correctness before pull request creation.
