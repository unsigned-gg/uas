---
status: planned
created: 2026-10-10
tags: [architecture, agents, interoperability]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0008-soma-cortex-saga-reverie-integration, 0009-saga-compensation-runner-and-durability, 0016-zed-agent-communication-protocol, 0021-cas-storage-scope-and-phase, 0026-cortex-pool-member-adapter]
---

# 0030-a2a-multi-agent-interop

> **Status**: planned - **Priority**: high - **Created**: 2026-10-10

## Overview

Multi-agent workflows combine autonomous reasoning loops with deterministic control flows, derived from invariant 4.14 in the estate synthesis (reported). This specification defines the Agent-to-Agent (A2A) protocol interoperability and declarative multi-agent orchestration for the Unsigned Agent Substrate, ensuring capabilities can be delegated without bloating single-agent contexts.

## Design

The architecture defines orchestrator nodes to compose hybrid agents, separating I/O-free state transitions in `uas-domain` from networking components in the `uas-acp` Zed Agent Communication Protocol server and `uas-engine`. The A2A protocol gateway establishes cross-framework connectivity. Note: The identity and auth parts of this spec sit in Allen Hackley's identity veto area.

### Decisions

| Decision | Rationale | Source |
|---|---|---|
| Zero-I/O Domain | `uas-domain` only defines pure state types; all network and artifact storage moves to `uas-acp` and `uas-engine`. | UAS spec lane w10:pN, 2026-10-10 |
| ADK-inspired Topology | Agent composition uses deterministic Sequential, Parallel, and Loop types similar to Google ADK. | UAS spec lane w10:pN, 2026-10-10 |
| Explicit Peer Addressing | Peer discovery requires pre-shared routing keys to prevent broad casting and context leakage. | UAS spec lane w10:pN, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0030-01]** (Ubiquitous): The `uas-domain` shall define pure state machine types for `LlmAgent`, `Sequential`, `Parallel`, and `Loop` orchestrator nodes.
- **[REQ-0030-02]** (Event-driven): WHEN an agent yields a large binary payload, the `uas-engine` shall store it by content reference in an out-of-band artifact service per Spec 0021.
- **[REQ-0030-03]** (State-driven): WHILE agents communicate across frameworks, the `uas-acp` shall mediate the connection via an A2A protocol gateway.
- **[REQ-0030-04]** (Unwanted): IF a delegated peer agent disappears mid-task, THEN the `uas-engine` shall trigger a deterministic compensation sequence as defined by Spec 0009.
- **[REQ-0030-05]** (Optional): WHERE an agent delegates capabilities, the `uas-domain` shall represent them as an `AgentTool` to prevent context bloat.
- **[REQ-0030-06]** (Ubiquitous): The `uas-acp` shall enforce addressing and discovery of peer agents over the Cortex Mesh per Specs 0008 and 0026.
- **[REQ-0030-07]** (Event-driven): WHEN two peer agents connect, the `uas-acp` shall negotiate A2A protocol versions and downgrade to the highest mutually supported version.
- **[REQ-0030-08]** (Unwanted): IF an agent attempts to delegate a task that exceeds a nesting depth of 5, THEN the `uas-domain` types shall reject the AST construction to prevent infinite loops.

## Plan

- [ ] Define declarative agent composition types and depth limits in `uas-domain` with zero I/O dependency.
- [ ] Implement the `AgentTool` capability delegation struct in `uas-domain`.
- [ ] Implement the A2A mutual cryptographic authentication gateway, peer addressing, and version negotiation in `uas-acp`.
- [ ] Integrate out-of-band artifact storage references in `uas-engine` message envelopes.

## Test

```bash
cargo test -p uas-domain -j 2 --test a2a_orchestrator_nodes
```

This test suite explicitly exercises `uas-domain` AST construction and validation, covering REQ-0030-01, REQ-0030-05, and REQ-0030-08. Coverage for `uas-acp` and `uas-engine` networking and artifact handling (REQ-0030-02 through 04, 06, 07) is deferred to their respective implementation specs. A passing suite verifies that `LlmAgent`, `Sequential`, `Parallel`, and `Loop` types execute state transitions deterministically without external I/O. As a negative control, if the maximum delegation depth validation rule in `uas-domain` is removed, the AST composition test for nesting depth > 5 must go red.

## Notes

The orchestrator nodes draw inspiration from the Google ADK topology (asserted). The required A2A dependencies are already pinned in Spec 0004 (line 193 pins `a2a-lf` 0.3.1 for `uas-engine`).
