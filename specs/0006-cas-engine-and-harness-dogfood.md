---
status: complete
created: 2026-10-08
tags: [engine, harness, dogfood, saga, substrate]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout]
---

# 0006-cas-engine-and-harness-dogfood

## Overview

This spec defines the dogfooding architecture for the Cerebral Agent Substrate (CAS).
It establishes the operational loop where Saga workflows and Substrate harnesses
host the builder agents that build CAS itself.

Why now: `cas-domain` is complete with pure types for slots, leases, envelopes,
and attestations. The next milestone in `specs/0001-cas-architecture-and-crate-layout.md`
is `cas-engine` (Phase 1 durable workflows and saga compensators).
Rather than building the engine in isolation, builder agents must execute within
Substrate slots governed by Saga workflows, proving the runtime invariants on host `ceres`.

## Design

### Requirements (EARS)

- **Ubiquitous:** The dogfooding harness shall execute builder agents within Substrate slots governed by Saga workflows.
- **Ubiquitous:** The engine shall record activity transitions in an append-only event store for replay recovery.
- **Event-driven:** WHEN a builder task completes, the engine shall emit a verified attestation containing command digests and exit codes.
- **State-driven:** WHILE executing activities on host `ceres`, the harness shall enforce Blackwall custody and host throttling (`-j 2`).
- **Unwanted behavior:** IF a worker crashes or restarts, THEN the engine shall recover execution by replaying events without re-spending model tokens.

### 1. Macro and Micro Orchestration Boundary

Development uses a two-tier orchestration model:

- **Macro tier (Saga):** Declares task graphs, step dependencies, operator gates,
  and release promotion. Defined in `.saga/flows/*.toml`.
- **Micro tier (Substrate):** Manages slot allocation, lease budgets, event-sourced
  workflow replay, and execution attestations. Handled by `cas-domain` and `cas-engine`.

### 2. The Dogfooding Loop

Each builder task follows a six-stage lifecycle:

```text
Saga Flow -> Custody Check -> Slot Acquisition -> Implementation -> Gate Verification -> Heimdal Approval
```

1. **Custody verification:** Verify that Blackwall and DeltaDB virtual worktrees
   are active. Prevent physical worktree duplication on host `ceres`.
2. **Slot lease:** Allocate a `cas-domain::Slot` with a timed `Budget` and `LeaseToken`.
3. **Task execution:** Builder agent implements the requested feature within
   the leased slot boundary.
4. **Mechanical gate verification:** Run `make check`, `make hygiene`, and `make smoke`.
   Enforce the mandatory `-j 2` CPU throttling rule.
5. **Attestation generation:** Emit a `cas-domain::Attestation` with `EvidenceTier::Verified`
   containing the command exit codes and output digests.
6. **Operator gate:** Surface receipts to Heimdal for sign-off before merge.

### 3. Engine Architecture (`cas-engine`)

`cas-engine` bridges `cas-domain` with runtime execution:

- `WorkflowSession`: Trait representing a durable execution unit.
- `Activity`: Discrete work item producing an `Attestation`.
- `EventStore`: Append-only event log enabling crash recovery and replay
  without token re-spending.
- `Compensator`: Rollback handler executed when verification gates fail.

### 4. CLI Harness Integration (`cas-cli`)

`cas-cli` exposes harness commands consumed by Saga steps:

- `cas harness run --slot <ID> --task <CMD>`: Executes a task within a leased slot,
  records turn envelopes, and emits structured JSON receipts.
- `cas harness verify --receipt <FILE>`: Verifies receipt integrity and evidence tiers.

## Plan

- [x] Create spec 0006 and record in `specs/README.md`.
- [x] Define `.saga/flows/cas-builder.toml` workflow template.
- [x] Validate flow template via `saga flows` and `saga run --dry-run`.
- [x] Implement `WorkflowSession` and `Activity` traits in `crates/cas-engine`.
- [x] Implement in-memory event store in `crates/cas-engine`.
- [x] Expose `cas harness` subcommand in `crates/cas-cli`.
- [x] Pass all workspace verification gates (`make check`, `make smoke`).

## Test

Run LeanSpec check, workspace verification gates, and verify the Saga flow:

```bash
make spec-check && make check && make smoke && saga flows | grep -q "cas-builder"
```

Pass criteria: exit code 0 across all checks, zero clippy warnings, and `cas-builder`
flow listed in Saga catalog.

## Notes

References:
- `specs/0001-cas-architecture-and-crate-layout.md`
- `specs/0002-verification-gates-and-evidence.md`
- `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md`

Decisions of record:
- Workstation throttling remains capped at `-j 2`.
- Virtual workspaces use Blackwall and DeltaDB; no physical Git worktree allocation.
