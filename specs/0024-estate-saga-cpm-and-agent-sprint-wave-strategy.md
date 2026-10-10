---
status: complete
created: 2026-10-10
tags: [engine, saga, cpm, wave, orchestration]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0006-cas-engine-and-harness-dogfood, 0009-saga-compensation-runner-and-durability]
---

# 0024-estate-saga-cpm-and-agent-sprint-wave-strategy

## Requirements

- **Ubiquitous:** The saga engine coordinator shall compute Critical Path Method (CPM) schedules over workflow step graphs before wave execution.
- **Ubiquitous:** The CPM schedule shall calculate Earliest Start (ES), Earliest Finish (EF), Latest Start (LS), Latest Finish (LF), and total float slack for every step.
- **Ubiquitous:** The wave coordinator shall partition unblocked steps into concurrent execution waves bounded by a maximum concurrency limit.
- **Ubiquitous:** The wave scheduler shall prioritize zero-float critical path steps over non-critical steps within each execution wave.
- **State-driven:** WHILE steps remain in a wave, the coordinator shall classify each step into its agent role and model tier.
- **State-driven:** WHILE an operator gate step executes, the coordinator shall retain execution within the main session.
- **Event-driven:** WHEN any step in a wave fails, the coordinator shall halt downstream waves and trigger reverse-order saga compensations.
- **Unwanted behavior:** IF a step graph contains a directed cycle, THEN the engine shall reject the flow with an invalid flow error.

## Design

### Critical Path Method (CPM)

Saga flows define directed acyclic graphs of activities. The CPM engine performs forward and backward passes to determine schedule bounds and identify the critical path:

1. **Forward pass:**
   - $ES(u) = \max_{v \in \text{preds}(u)} EF(v)$ ($ES = 0$ for entry nodes).
   - $EF(u) = ES(u) + \text{duration}(u)$.
   - Project duration $T = \max_{u} EF(u)$.

2. **Backward pass:**
   - $LF(u) = \min_{w \in \text{succs}(u)} LS(w)$ ($LF = T$ for terminal nodes).
   - $LS(u) = LF(u) - \text{duration}(u)$.
   - $\text{slack}(u) = LS(u) - ES(u) = LF(u) - EF(u)$.

3. **Critical Path:**
   - Any step with $\text{slack}(u) = 0$ is critical.
   - Delay in any critical step delays the entire workflow completion.

### Agent Sprint Wave Strategy

In accordance with estate orchestration rules:

- **Concurrency limit:** At most 3 concurrent subagents per wave (`max_concurrency = 3`).
- **Critical path priority:** Critical path steps are scheduled first in each wave. Remaining capacity is filled with non-critical tasks ordered by ascending slack.
- **Agent role and model tier classification:**
  - `Probe`: Fast lightweight model, read-only mechanical tasks, lint, and verification.
  - `Builder`: Capable pro model, implementation tasks in virtual DeltaDB or shared worktree with delta tracking.
  - `Critic`: Capable pro model, adversarial review and verification post-implementation.
  - `MainSession`: Main session coordinator, gated actions, landing, and secrets. Subagents never execute gated actions.
- **Wave execution and rollback:**
  - Waves execute sequentially; steps within a wave execute concurrently up to the concurrency limit.
  - Failure in any step halts the current wave and triggers compensators in reverse order for all completed steps across all previous and current waves.
  - Compensation continues past a failed compensator. The rollback returns Err only when at least one receipt has `success: false`, and the Err carries every receipt (spec 0009, CER-2783).

## Tasks

- [x] Create spec 0024 and register in `specs/README.md`.
- [x] Implement CPM scheduler and DAG analysis in `uas-engine::cpm`.
- [x] Implement Agent Sprint Wave strategy and role assignment in `uas-engine::wave`.
- [x] Expose CPM and wave planning on `SagaFlowCoordinator`.
- [x] Integrate wave execution support in `uas-cli`.
- [x] Add comprehensive tests for CPM, wave scheduling, and rollback.

## Verification

```bash
cargo test -p uas-engine -j 2 --test cpm_wave
```

## Notes

- Concurrency default is 3, matching the estate host throttling rule for Ceres.
- Default duration estimates: Verify = 5, Impl = 20, Gate = 1, Test = 10.
