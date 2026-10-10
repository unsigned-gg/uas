---
status: planned
created: '2026-10-10'
tags: [architecture, agents, interoperability]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0016-zed-agent-communication-protocol]
---

# 0030-a2a-multi-agent-interop

> **Status**: planned - **Priority**: high - **Created**: 2026-10-10

## Overview

Multi-agent workflows combine autonomous reasoning loops with deterministic control flows, derived from invariant 4.14 in the estate synthesis (reported). This specification defines the Agent-to-Agent (A2A) protocol interoperability and declarative multi-agent orchestration for the Cerebral Agent Substrate, ensuring capabilities can be delegated without bloating single-agent contexts.

## Design

The architecture defines orchestrator nodes to compose hybrid agents, separating I/O-free state transitions in `cas-domain` from networking components in `cas-acp` and `cas-engine`. The A2A protocol gateway establishes cross-framework connectivity with mutual cryptographic authentication. 

### Decisions

| Decision | Rationale | Source |
|---|---|---|
| Zero-I/O Domain | `cas-domain` only defines pure state types; all network and artifact storage moves to `cas-acp` and `cas-engine`. | CAS spec lane w10:pN, 2026-10-10 |
| ADK-inspired Topology | Agent composition uses deterministic Sequential, Parallel, and Loop types similar to Google ADK. | CAS spec lane w10:pN, 2026-10-10 |
| Explicit Peer Addressing | Peer discovery requires pre-shared routing keys to prevent broad casting and context leakage. | CAS spec lane w10:pN, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0030-01]** (Ubiquitous): The `cas-domain` shall define pure state machine types for `LlmAgent`, `Sequential`, `Parallel`, and `Loop` orchestrator nodes.
- **[REQ-0030-02]** (Event-driven): WHEN an agent yields a large binary payload, the `cas-acp` shall store it by content reference in an out-of-band artifact service.
- **[REQ-0030-03]** (State-driven): WHILE agents communicate across frameworks, the `cas-acp` shall mediate the connection via an Agent-to-Agent protocol gateway with mutual cryptographic authentication.
- **[REQ-0030-04]** (Unwanted): IF a delegated peer agent disappears mid-task, THEN the `cas-engine` shall trigger a deterministic compensation sequence.
- **[REQ-0030-05]** (Optional): WHERE an agent delegates capabilities, the `cas-domain` shall represent them as an `AgentTool` to prevent context bloat.
- **[REQ-0030-06]** (Ubiquitous): The `cas-acp` shall enforce addressing and discovery of peer agents using pre-shared routing keys over the Cortex Mesh.
- **[REQ-0030-07]** (Event-driven): WHEN two peer agents connect, the `cas-acp` shall negotiate A2A protocol versions and downgrade to the highest mutually supported version.
- **[REQ-0030-08]** (Unwanted): IF an agent attempts to delegate a task that exceeds a nesting depth of 5, THEN the `cas-acp` shall reject the request to prevent infinite loops.

## Plan

- [ ] Define declarative agent composition types in `cas-domain` with zero I/O dependency.
- [ ] Implement the `AgentTool` capability delegation struct in `cas-domain`.
- [ ] Implement the A2A mutual cryptographic authentication gateway, peer addressing, and version negotiation in `cas-acp`.
- [ ] Integrate out-of-band artifact storage references in `cas-engine` message envelopes.

## Test

```bash
cargo test -p cas-domain -j 2 --test a2a_orchestrator_nodes
```

A passing test suite verifies that `LlmAgent`, `Sequential`, `Parallel`, and `Loop` types execute state transitions deterministically without external I/O. As a negative control, if the maximum delegation depth validation rule is removed, the AST composition test for nesting depth > 5 must go red.

## Notes

The orchestrator nodes draw inspiration from the Google ADK topology (asserted). Any new dependencies for cryptographic authentication will be pinned and recorded in spec 0004.
