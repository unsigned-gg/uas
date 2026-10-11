---
status: complete
created: 2026-10-09
tags: [engine, saga, compensator, cli, replay, durability]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0006-cas-engine-and-harness-dogfood, 0008-soma-cortex-saga-reverie-integration]
---

# 0009-saga-compensation-runner-and-durability

## Overview

This spec defines the execution runner, cleanup compensation handlers, and deterministic
crash recovery verification for Phase 1 of the Unsigned Agent Substrate (UAS).

Why now: `uas-domain` provides slot, lease, attestation, and policy primitives. `uas-engine`
provides event-sourced session models and basic coordinators. To complete Phase 1 and dogfood
autonomous builder agents on host `ceres`, the substrate requires:

1. **Saga Compensation Handlers (CER-2677):** Modular handlers for worktree removal,
   branch pruning, and lease return, executed in reverse dependency order upon step failure,
   producing structured `CompensationReceipt` records.
2. **Headless Flow Runner (CER-2678):** Parsing `.saga/flows/*.toml` definitions in `uas-cli`,
   dispatching step activities across dependency DAGs, and emitting structured JSON execution receipts.
3. **Deterministic Replay & Crash Recovery (CER-2679):** Proving that interrupted sessions
   reconstruct their full state strictly from the recorded event log without re-executing
   activities or re-spending model tokens.

## Design

### Requirements (EARS)

- **Ubiquitous:** The engine coordinator shall execute registered compensators in reverse dependency order upon workflow failure.
- **Ubiquitous:** Each executed compensator shall produce a structured `CompensationReceipt` recording success and status reason.
- **Event-driven:** WHEN a step activity fails, the session shall transition to `Compensating` and trigger cleanup.
- **State-driven:** WHILE recovering from a restart, the engine shall reconstruct session state from event logs without re-executing activities.
- **Unwanted behavior:** IF a compensation handler fails, THEN the engine shall record the error and continue remaining compensations.

### 1. Compensation Architecture

When an activity returns a non-zero exit code or violates admission policies, the session
transitions to `SessionState::Compensating`. The coordinator executes registered compensators
in strict reverse order of completed steps:

- `WorktreeCleanupCompensator`: Cleans up temporary working directories and virtual worktree state.
- `BranchPruneCompensator`: Prunes unmerged transient git branches.
- `LeaseReturnCompensator`: Revokes or returns active slot leases to the ledger.
- `CompositeCompensator`: Aggregates multiple compensators and executes them sequentially. It runs every child in add order, never returns Err, and records a child Err as a receipt with `success: false`. A rollback returns Err only when at least one receipt has `success: false`, and that Err carries every receipt (lane decision w25:p1, 2026-10-10, CER-2783; Christian may override).
- `WorkflowSession::fail_activity` keeps every receipt, ends the log with `SessionFinished` (`Failed`), and returns `CompensationIncomplete` only when the compensator itself returned Err (lane decision w25:p1, 2026-10-10, CER-2785; Christian may override). Replay restores `Failed` but not the receipts, which are not in the event log.

Each executed compensator yields a `CompensationReceipt`:

```rust
pub struct CompensationReceipt {
    pub activity_id: String,
    pub handler_name: String,
    pub success: bool,
    pub details: String,
    pub timestamp: u64,
}
```

### 2. Flow Parsing and Execution

Flows defined in `.saga/flows/*.toml` follow the schema:

```toml
[flow]
name = "gates"
description = "Execute workspace verification gates"

[[step]]
id = "fmt-check"
title = "Verify rustfmt formatting"
kind = "test"
agent = "auto"
verify_cmd = ["make", "fmt-check"]
```

`uas harness run --flow <PATH>`:
- Parses the flow TOML file.
- Builds a `SagaFlowCoordinator` with dependency tracking.
- Dispatches runnable steps in topological order.
- Executes `verify_cmd` with `-j 2` CPU throttling.
- Collects step attestations into the final `HarnessReport`.

### 3. Crash Recovery and Deterministic Replay

A crashed or interrupted workflow must be recoverable from any point in its event stream:

- Process termination simulated after any step.
- `WorkflowSession::replay_from_events` reconstructs slot, lease, session state, and activity logs.
- Previously completed activities are skipped on recovery, guaranteeing zero re-spend and zero redundant side effects.

## Plan

- [x] Create spec 0009 and register in `specs/README.md`.
- [x] Add `toml` dependency to workspace and document in spec 0004.
- [x] Implement compensation handlers and `CompensationReceipt` in `uas-engine`.
- [x] Add `SagaFlow` TOML deserializer and runner in `uas-engine` and `uas-cli`.
- [x] Implement `crates/uas-engine/tests/saga.rs` integration tests.
- [x] Implement `crates/uas-engine/tests/replay.rs` deterministic replay tests.
- [x] Verify gates with `make spec-check && make check && make smoke`.

## Test

```bash
make spec-check
cargo test -p uas-engine -j 2 --test saga
cargo test -p uas-engine -j 2 --test replay
cargo run -p uas-cli -j 2 -- harness run --help
```

Pass criteria: exit code 0 across all test suites under mandatory `-j 2` host throttling.

## Notes

- `CompositeCompensator` ran its children in add order until PR #140 changed it to reverse add order, per line 33 (reverse dependency order). Ordering across activities was already reverse; the fix is the order inside one composite. Test: `lost_slot_runs_compensators_in_reverse_order` in `crates/uas-engine/tests/lost_slot.rs` [verified 2026-10-11 at `2399dda`: `cargo nextest run -p uas-engine lost_slot_runs_compensators_in_reverse_order -j 2`, 1 passed]. The same PR reordered the receipt assertions of `saga_compensation_runner_reverse_execution_and_receipts` in `tests/saga.rs`. Ruled: integrating lane w25:p1, 2026-10-11.

References:
- `specs/0001-cas-architecture-and-crate-layout.md`
- `specs/0006-cas-engine-and-harness-dogfood.md`
- `specs/0008-soma-cortex-saga-reverie-integration.md`
- `specs/0004-dependency-and-licence-policy.md`
