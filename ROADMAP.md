# uas — Architectural Roadmap

> Rust-Native Pooled Agent Substrate & Durable Workflow Runtime  
> Repo: `unsigned-gg/uas` · Formal Spec: [0064-initial-roadmap.md](specs/0064-initial-roadmap.md)

## Current State (Horizon 1 — Active)

- **Pure Domain & Zero-I/O Isolation:** Pure types, slot states, lease tokens, envelopes, and attestations in `uas-domain` ([Spec 0001](specs/0001-cas-architecture-and-crate-layout.md)). Zero direct I/O enforced (`std::fs`, `std::net`, `std::process`, `tokio` denied).
- **Durable Workflow Execution:** Autumn-Harvest event-driven workflow engine in `uas-engine` with event log replay, activity retry loops, and fail-open Reverie memory client degradation ([Spec 0044](specs/0044-reverie-client-degradation.md)).
- **Kernel Sandboxing & Confinement:** Landlock LSM filesystem containment in `uas-sandbox` ([Spec 0010](specs/0010-landlock-ruleset-and-abi-probe.md)), network namespace isolation ([Spec 0011](specs/0011-network-namespace-isolation.md)), and `PR_SET_NO_NEW_PRIVS`.
- **eBPF Priority Clamping & Supervision:** Aya eBPF tracepoint program in `uas-ebpf` clamping builds to `nice 19` ([Spec 0014](specs/0014-ebpf-priority-clamping-and-audit-probes.md)), cgroup v2 freezer, subreaper process supervision, and signal escalation ([Spec 0060](specs/0060-agent-harness-process-supervision-and-signals-coordination.md)).
- **Multi-Process Slot Arbitration:** Advisory file locking (`flock`) and sequential slot allocation in `uas-pool` ([Spec 0036](specs/0036-slot-arbitration-across-processes.md)), gitoxide warm worktrees, and SHA-256 build cache retention with quota eviction ([Spec 0047](specs/0047-slot-build-cache-retention.md)).
- **Agent Communication Protocol (ACP):** Stdio JSON-RPC 2.0 ACP server in `uas-acp` for Zed and IDE harnesses ([Spec 0016](specs/0016-zed-agent-communication-protocol.md), [Spec 0035](specs/0035-cas-acp-transport-scope.md)).
- **Agent Harness MCP Bridge:** Hermetic MCP configuration bridge in `uas-engine` with UDS proxying, tool command allowlisting, and credential sanitization ([Spec 0063](specs/0063-agent-harness-mcp-configuration-bridge.md)).

## Planned Horizons (LeanSpec 0064)

### Horizon 2: Model Routing, Operator Deck & Local Substrate Testing
- **Prefix Hashing & Affinity Routing:** FastCache SHA-256 prompt prefix hashing in `uas-router` for KV-cache hit optimization ([Spec 0017](specs/0017-fastcache-prefix-hashing-and-affinity-routing.md)).
- **Dual-Stream Think Token Splitting:** Zero-allocation 7-byte window `<think>` streaming parser and Unix socket gateway proxy ([Spec 0013](specs/0013-llm-gateway-socket-proxy.md)).
- **Operator Control Deck:** Axum/Maud/htmx interface in `uas-deck` with in-process syntactic diff viewer via `similar` ([Spec 0018](specs/0018-mad-stack-operator-deck-and-settle-queue.md)) and live settlement queue polling.
- **Repeatable Substrate Test Suite:** Hermetic local test suite on Ceres (`make ceres-test`) verifying unprivileged cgroup v2 and Landlock isolation ([Spec 0059](specs/0059-ceres-localhost-repeatable-substrate-testing.md)).

### Horizon 3: Distributed Storage, Multi-Agent Interop & Cryptographic Attestation
- **Distributed Storage Client:** Zero-Go network client adapters in `uas-storage` for `rustfs`, `minikv`, and `aistore` over S3/REST protocols ([Spec 0021](specs/0021-cas-storage-scope-and-phase.md)).
- **Native A2A Wire Protocol:** Inter-agent delegation wire format over Cortex Mesh and NATS JetStream with personality-based capability bounds ([Spec 0030](specs/0030-a2a-multi-agent-interop.md)).
- **Workload Attestation:** Cryptographic turn receipts, SPIFFE/SPIRE workload identities, and on-behalf-of delegation chains ([Spec 0029](specs/0029-agent-identity-and-on-behalf-of-delegation.md)).

### Horizon 4: Virtual Worktrees, Custody Chain & Cloud Deployment
- **DeltaDB Virtual Worktrees:** Ephemeral virtual worktrees avoiding physical disk duplication, Blackwall jailed execution, and automated PR reconciliation targeting `development` ([Spec 0022](specs/0022-custody-landing.md), [Spec 0025](specs/0025-wave-execution-on-the-custody-chain.md)).
- **Kubernetes Operator:** Cloud-native lifecycle orchestration via `uas-operator` managing `AgentSession`, `WorkerPool`, and `AffinityRoute` CRDs ([Spec 0038](specs/0038-uas-operator-and-crds.md)).
- **crates.io Publishing:** Topological release of open-source crates (`uas-domain`, `uas-sandbox`, `uas-pool`, `uas-acp`, `uas-cli`) under Apache-2.0 / MIT licensing ([Spec 0048](specs/0048-licence-and-publication.md)).

## Estate Integration
- **Soma:** Reflex gateway classifies task policy tiers; swarm-control provisions worker slots.
- **Cortex:** Agent mesh peer messaging, settle queue integration (`/v1/settle/pending`), and lane synchronization.
- **Saga:** Compensation workflows for failed executions, durable audit receipts, and verification tier promotion.
- **Reverie:** Persistent cross-session agent memory, fail-open search, and durable disk outbox synchronization.
