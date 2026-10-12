---
status: planned
created: 2026-10-12
tags: [roadmap, substrate, architecture, milestones, runtime, ears]
priority: high
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0064-initial-roadmap

> **Status**: planned · **Priority**: high · **Created**: 2026-10-12

## Overview

UAS (Unsigned Agent Substrate) is the Rust-native, pooled runtime under every
agent harness across the estate (Claude Code, AGY, OMP, Codex, OpenCode).
It executes agent sessions as durable workflows, confines worker processes with
Linux kernel primitives, routes model requests by prompt prefix affinity, and
surfaces operational visibility to the operator deck.

Following the initial v0.1.0 scaffold and domain landing, this specification
formalizes the multi-horizon evolutionary roadmap for UAS. It establishes the
milestones, crate responsibilities, and architectural boundaries governing the
system from local sandbox execution to cluster-scale orchestration.

The roadmap organizes substrate development across four sequential horizons:
1. Kernel Isolation, Worker Slot Confinement & Durable Workflows (Phases 1–3).
2. Model Routing, Operator Deck & Local Substrate Testing (Phases 4–5).
3. Distributed Storage, Multi-Agent Interop & Cryptographic Attestation (Phases 6–7).
4. Virtual Worktrees, Custody Chain & Cloud Deployment (Phases 8–9).

## Design

### Horizon 1: Kernel Isolation, Worker Slot Confinement & Durable Workflows

- **Pure Domain & Durable Execution:** Maintain strict zero-I/O purity in `uas-domain`
  ([Spec 0001](0001-cas-architecture-and-crate-layout.md)). Autumn-Harvest workflow
  orchestration in `uas-engine` with deterministic event log replay, activity retry
  loops, and resilient Reverie client degradation ([Spec 0044](0044-reverie-client-degradation.md)).
- **Kernel Sandbox & Resource Containment:** Landlock LSM rulesets in `uas-sandbox`
  for strict worktree filesystem jailing ([Spec 0010](0010-landlock-ruleset-and-abi-probe.md)),
  network namespace isolation ([Spec 0011](0011-network-namespace-isolation.md)),
  and `PR_SET_NO_NEW_PRIVS` enforcement.
- **eBPF Telemetry & Priority Clamping:** Aya eBPF tracepoint programs in `uas-ebpf`
  enforcing process priority clamping (`nice 19`) on shared hosts ([Spec 0014](0014-ebpf-priority-clamping-and-audit-probes.md)),
  cgroup v2 freezer integration, and non-blocking zombie process tree reaping ([Spec 0060](0060-agent-harness-process-supervision-and-signals-coordination.md)).
- **Slot Pool & Build Cache:** Multi-process advisory slot arbitration in `uas-pool`
  ([Spec 0036](0036-slot-arbitration-across-processes.md)), gitoxide warm worktree lifecycle,
  and deterministic SHA-256 build cache retention with quota eviction ([Spec 0047](0047-slot-build-cache-retention.md)).
- **IDE Protocol Server:** Implement the Agent Communication Protocol (ACP) in `uas-acp`
  over stdio JSON-RPC 2.0 ([Spec 0016](0016-zed-agent-communication-protocol.md),
  [Spec 0035](0035-cas-acp-transport-scope.md)).

### Horizon 2: Model Routing, Operator Deck & Local Substrate Testing

- **Prefix-Hash Routing & Gateway Proxy:** `uas-router` implements FastCache
  SHA-256 prompt prefix hashing for cache-affinity load balancing ([Spec 0017](0017-fastcache-prefix-hashing-and-affinity-routing.md)),
  dual-stream `<think>` token splitting with fixed 7-byte window, and Unix domain
  socket proxying to the LLM gateway ([Spec 0013](0013-llm-gateway-socket-proxy.md)).
- **Operator Control Deck:** `uas-deck` Axum/Maud/htmx interface with in-process
  syntactic diff tables via `similar` ([Spec 0018](0018-mad-stack-operator-deck-and-settle-queue.md))
  and live Cortex settle queue integration (`/v1/settle/pending`).
- **Repeatable Local Testing:** Hermetic unprivileged substrate test suite on host
  Ceres (`make ceres-test`) verifying cgroup v2 and Landlock isolation ([Spec 0059](0059-ceres-localhost-repeatable-substrate-testing.md)).

### Horizon 3: Distributed Storage, Multi-Agent Interop & Cryptographic Attestation

- **Distributed Storage Layer:** `uas-storage` provides network client adapters
  for `rustfs`, `minikv`, and `aistore` over S3/REST protocols without linking Go internals
  ([Spec 0021](0021-cas-storage-scope-and-phase.md)).
- **Native A2A Wire Protocol:** Inter-agent delegation protocol over Cortex Mesh
  and NATS JetStream, personality-gated role capabilities, and out-of-band artifact references
  ([Spec 0030](0030-a2a-multi-agent-interop.md)).
- **Workload Attestation:** Cryptographically signed turn receipts, SPIFFE/SPIRE
  workload identities, and on-behalf-of delegation chains ([Spec 0029](0029-agent-identity-and-on-behalf-of-delegation.md)).

### Horizon 4: Virtual Worktrees, Custody Chain & Cloud Deployment

- **DeltaDB Virtual Worktrees & Custody:** Ephemeral virtual worktrees avoiding physical
  disk duplication, Blackwall jailed execution, and automated PR reconciliation targeting
  `development` ([Spec 0022](0022-custody-landing.md), [Spec 0025](0025-wave-execution-on-the-custody-chain.md)).
- **Kubernetes Operator:** Cloud-native lifecycle orchestration via `uas-operator`
  reconciling `AgentSession`, `WorkerPool`, and `AffinityRoute` CRDs ([Spec 0038](0038-uas-operator-and-crds.md)).
- **Ecosystem Publishing:** Coordinated topological release of open-source crates
  to crates.io under Apache-2.0 / MIT licensing ([Spec 0048](0048-licence-and-publication.md)).

### Requirements (EARS)

- **Ubiquitous:** The workspace crates shall strictly adhere to downward layering (`deck > acp > engine > pool > sandbox, ebpf > router > domain`).
- **Ubiquitous:** The `uas-domain` crate shall contain pure domain types with zero direct I/O dependencies.
- **State-driven:** WHILE a worker process executes inside a pool slot, the substrate shall clamp build process trees to `nice 19` and enforce cgroup v2 resource limits.
- **Event-driven:** WHEN a worker crash or host reboot occurs, the `uas-engine` workflow executor shall recover state by replaying the event log without re-spending LLM tokens.
- **Unwanted behavior:** IF a worker attempts unauthorized filesystem egress or privilege escalation, THEN the Landlock LSM filter shall reject the operation and trigger saga compensation.
- **Optional feature:** WHERE eBPF telemetry is active, `uas-ebpf` tracepoints shall stream process audit records to an in-memory ring buffer without userspace polling.

## Plan

- [x] Phase 1: Pure domain types in `uas-domain` and workflow engine foundations.
- [x] Phase 1: Reverie client degradation and durable write-behind outbox ([Spec 0044](0044-reverie-client-degradation.md)).
- [x] Phase 2: Landlock LSM sandboxing and namespace isolation ([Spec 0010](0010-landlock-ruleset-and-abi-probe.md)).
- [x] Phase 2: eBPF priority clamping and subreaper supervision ([Spec 0014](0014-ebpf-priority-clamping-and-audit-probes.md), [Spec 0060](0060-agent-harness-process-supervision-and-signals-coordination.md)).
- [x] Phase 3: Host slot arbitration and slot build cache retention ([Spec 0036](0036-slot-arbitration-across-processes.md), [Spec 0047](0047-slot-build-cache-retention.md)).
- [x] Phase 3: Agent Communication Protocol (ACP) stdio server ([Spec 0016](0016-zed-agent-communication-protocol.md), [Spec 0035](0035-cas-acp-transport-scope.md)).
- [ ] Phase 4: Prefix hashing, FastCache affinity routing, and gateway proxy ([Spec 0017](0017-fastcache-prefix-hashing-and-affinity-routing.md)).
- [ ] Phase 5: Operator deck settlement reader and modal diff engine ([Spec 0018](0018-mad-stack-operator-deck-and-settle-queue.md)).
- [ ] Phase 6: Distributed storage client adapters for rustfs and aistore ([Spec 0021](0021-cas-storage-scope-and-phase.md)).
- [ ] Phase 7: A2A multi-agent wire interop and cryptographic delegation ([Spec 0030](0030-a2a-multi-agent-interop.md)).
- [ ] Phase 8: DeltaDB virtual worktrees and Blackwall custody chain ([Spec 0025](0025-wave-execution-on-the-custody-chain.md)).
- [ ] Phase 9: Kubernetes operator CRDs and crates.io release publication ([Spec 0038](0038-uas-operator-and-crds.md), [Spec 0048](0048-licence-and-publication.md)).

## Test

Run LeanSpec specification conformance validation and workspace test suite:

```bash
make spec-check && cargo test --workspace --locked -j 2
```

## Notes

- References [Spec 0001](0001-cas-architecture-and-crate-layout.md) for crate layering rules and core invariants.
- Synchronizes with `ROADMAP.md` at the repository root and `engineering/roadmap-improvements.md`.
- Conforms to LeanSpec and ASD-STE100 technical documentation standards under CC-BY-4.0.
