---
status: complete
created: 2026-10-08
tags: [soma, cortex, saga, reverie, integration, service-edges]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0006-cas-engine-and-harness-dogfood]
---

# 0008-soma-cortex-saga-reverie-integration

## Overview

This spec defines the service edge integration architecture for the Cerebral Agent Substrate (CAS).
Per `specs/0001-cas-architecture-and-crate-layout.md` section 3, CAS integrates with four external
estate pillars through typed service edges without vendoring them:

1. **Soma**: The reflex gateway classifying incoming tasks into policy tiers.
2. **Saga**: The durable workflow engine executing macro-task graphs and triggering compensators.
3. **Cortex**: The inter-agent communication mesh and operator settle queue (`/v1/settle/pending`).
4. **Reverie**: The persistent memory daemon (`127.0.0.1:7437`) storing context and attestations.

Why now: `cas-domain` and `cas-engine` provide the pure domain primitives and event-sourced session
replay. To host autonomous builder agents on host `ceres`, the harness must classify policy with Soma,
execute under Saga flows, broadcast attestations to Cortex, and query/record memory with Reverie.

## Design

### Requirements (EARS)

- **Ubiquitous:** The substrate shall communicate with Soma, Saga, Cortex, and Reverie exclusively across defined service edges.
- **Event-driven:** WHEN admitting a task, the substrate shall verify that slot parameters match the Soma policy tier.
- **Event-driven:** WHEN an activity produces verified attestations, the substrate shall persist records to Reverie and publish status to Cortex.
- **State-driven:** WHILE handling Tier2 actions, the substrate shall await operator approval from the Cortex settle queue.
- **Unwanted behavior:** IF an activity fails verification, THEN the substrate shall invoke registered Saga compensators for rollback.

### 1. Integration Topology

The lifecycle follows the estate boundary contract:

```text
Soma (Policy Gateway)
       │ (classify policy tier: Tier0 / Tier1 / Tier2)
       ▼
Saga Workflow (.saga/flows/cas-builder.toml)
       │ (dispatches harness steps into leased slot)
       ▼
CAS Substrate (cas-domain, cas-engine, cas-cli)
       │
       ├──► Reverie Client (HTTP 127.0.0.1:7437) — pre-fetch context, write attestations
       │
       ├──► Cortex Edge (mesh events & /v1/settle/pending) — operator approval cards
       │
       └──► Saga Compensators — backward recovery on verification failure
```

### 2. Service Edge Contracts

#### Soma Policy Tier Edge
- Defines `PolicyTier` in domain / engine:
  - `Tier0`: Read-only, inspection, or dry-run. No filesystem or network writes.
  - `Tier1`: Virtual sandboxed builder activity (Landlock + DeltaDB virtual worktree).
  - `Tier2`: Gated actions (promotions, release merges, external sync). Requires operator signoff.
- Tasks are admitted only when the slot lease budget and sandbox configuration match the classified tier.

#### Reverie Memory Edge
- HTTP client connecting to `http://127.0.0.1:7437`:
  - `query_context(project, query)`: Retrieves semantic context prior to activity execution (`GET /search?q=...` or `/context/smart`).
  - `record_attestation(session_id, attestation)`: Persists execution receipts and verified evidence into memory.

#### Cortex Mesh & Settle Edge
- Publishes signed attestation events to the Cortex mesh using `cortex-msg` or edge JSON payloads.
- Defines `SettleRequest` and pending settlement queue schema matching `/v1/settle/pending`:
  - When an activity reaches a `Tier2` gate (e.g. `operator-gate`), CAS registers an approval card.
  - The operator reviews diffs and receipts in `cas-deck` or Cortex before releasing the gate.

#### Saga Orchestration Edge
- `.saga/flows/cas-builder.toml` acts as the macro-orchestrator.
- Activities emit structured JSON execution receipts (`HarnessReport`).
- If an activity returns a non-zero exit code or fails a verification gate, `cas-engine` executes registered `Compensator`s in reverse order.

## Plan

- [x] Create spec 0008 and record in `specs/README.md`.
- [x] Implement `PolicyTier` and classification in `crates/cas-domain` and `crates/cas-engine`.
- [x] Implement Reverie client adapter in `crates/cas-engine` (or `cas-cli`).
- [x] Implement Cortex mesh notification and Settle approval schema in `crates/cas-engine` and `crates/cas-deck`.
- [x] Wire Saga execution flow in `cas harness run` to integrate policy classification, memory queries, and compensations.
- [x] Validate with unit tests and workspace verification gates (`make spec-check`, `make check`, `make smoke`).

## Test

```bash
make spec-check && cargo test -p cas-domain -p cas-engine -j 2
```

Pass criteria: `make spec-check` exits 0 with no broken links, and domain and engine unit tests pass with `-j 2`.

## Notes

References:
- `specs/0001-cas-architecture-and-crate-layout.md`
- `specs/0006-cas-engine-and-harness-dogfood.md`
- `cerebral-work/research@4068134:SYNTHESIS-ESTATE-AND-SUBSTRATE.md`
- `~/.agents/sops/blackwall-custody-process.md`
