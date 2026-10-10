---
status: planned
created: '2026-10-10'
tags: [architecture, agents, interoperability]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0008-soma-cortex-saga-reverie-integration, 0009-saga-compensation-runner-and-durability, 0016-zed-agent-communication-protocol, 0026-cortex-pool-member-adapter]
---

# 0030-a2a-multi-agent-interop

> **Status**: planned - **Priority**: high - **Created**: 2026-10-10

## Overview

Multi-agent workflows combine autonomous reasoning loops with deterministic control flows, derived from invariant 4.14 in the estate synthesis (reported). This specification defines the Agent-to-Agent (A2A) protocol interoperability and declarative multi-agent orchestration for the Cerebral Agent Substrate, ensuring capabilities can be delegated without bloating single-agent contexts.

## Design

The architecture defines orchestrator nodes to compose hybrid agents, separating I/O-free state transitions in `cas-domain` from networking components in `cas-acp` and `cas-engine`. The A2A protocol gateway establishes cross-framework connectivity. Note: The identity and auth parts of this spec sit in the Director of Engineering's identity veto area.

### Decisions

| Decision | Rationale | Source |
|---|---|---|
| Zero-I/O Domain | `cas-domain` only defines pure state types; all network and artifact storage moves to `cas-acp` and `cas-engine`. | CAS spec lane w10:pN, 2026-10-10 |
| ADK-inspired Topology | Agent composition uses deterministic Sequential, Parallel, and Loop types similar to Google ADK. | CAS spec lane w10:pN, 2026-10-10 |
| Explicit Peer Addressing | Peer discovery requires pre-shared routing keys to prevent broad casting and context leakage. | CAS spec lane w10:pN, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0030-01]** (Ubiquitous): The `cas-domain` shall define pure state machine types for `LlmAgent`, `Sequential`, `Parallel`, and `Loop` orchestrator nodes.
- **[REQ-0030-02]** (Event-driven): WHEN an agent yields a large binary payload, the `cas-acp` shall store it by content reference in an out-of-band artifact service.
- **[REQ-0030-03]** (State-driven): WHILE agents communicate across frameworks, the `cas-acp` shall mediate the connection via an A2A protocol gateway.
- **[REQ-0030-04]** (Unwanted): IF a delegated peer agent disappears mid-task, THEN the `cas-engine` shall trigger a deterministic compensation sequence as defined by Spec 0009.
- **[REQ-0030-05]** (Optional): WHERE an agent delegates capabilities, the `cas-domain` shall represent them as an `AgentTool` to prevent context bloat.
- **[REQ-0030-06]** (Ubiquitous): The `cas-acp` shall enforce addressing and discovery of peer agents over the Cortex Mesh per Specs 0008 and 0026.
- **[REQ-0030-07]** (Event-driven): WHEN two peer agents connect, the `cas-acp` shall negotiate A2A protocol versions and downgrade to the highest mutually supported version.
- **[REQ-0030-08]** (Unwanted): IF an agent attempts to delegate a task that exceeds a nesting depth of 5, THEN the `cas-domain` types shall reject the AST construction to prevent infinite loops.

## Plan

- [ ] Define declarative agent composition types and depth limits in `cas-domain` with zero I/O dependency.
- [ ] Implement the `AgentTool` capability delegation struct in `cas-domain`.
- [ ] Implement the A2A mutual cryptographic authentication gateway, peer addressing, and version negotiation in `cas-acp`.
- [ ] Integrate out-of-band artifact storage references in `cas-engine` message envelopes.

## Test

```bash
cargo test -p cas-domain -j 2 --test a2a_orchestrator_nodes
```

This test suite covers REQ-0030-01, REQ-0030-05, and REQ-0030-08. Coverage for `cas-acp` and `cas-engine` networking and artifact handling (REQ-0030-02 through 04, 06, 07) is deferred to their respective implementation specs. A passing suite verifies that `LlmAgent`, `Sequential`, `Parallel`, and `Loop` types execute state transitions deterministically without external I/O. As a negative control, if the maximum delegation depth validation rule is removed, the AST composition test for nesting depth > 5 must go red.

## Notes

The orchestrator nodes draw inspiration from the Google ADK topology (asserted). The required A2A dependencies are already pinned in Spec 0004 (line 193 pins `a2a-lf` 0.3.1 for `cas-engine`).
