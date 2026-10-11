<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Specs

This directory holds the LeanSpec registry. Every spec is one file.
Methodology: LeanSpec. The reference copy is in `../vendor/leanspec/`.

| Spec | Title | Status | Priority |
|---|---|---|---|
| [0001](0001-cas-architecture-and-crate-layout.md) | UAS architecture and crate layout | in-progress | high |
| [0002](0002-verification-gates-and-evidence.md) | Verification gates and evidence | in-progress | high |
| [0003](0003-branch-model-and-promotion.md) | Branch model and promotion | in-progress | high |
| [0004](0004-dependency-and-licence-policy.md) | Dependency and licence policy | in-progress | high |
| [0005](0005-ci-cache-and-runners.md) | CI, cache and runners | in-progress | high |
| [0006](0006-cas-engine-and-harness-dogfood.md) | UAS engine and harness dogfood | complete | high |
| [0007](0007-lease-rules.md) | Lease rules | complete | high |
| [0008](0008-soma-cortex-saga-reverie-integration.md) | Soma Cortex Saga Reverie integration | complete | high |
| [0009](0009-saga-compensation-runner-and-durability.md) | Saga compensation runner and durability | complete | high |
| [0010](0010-landlock-ruleset-and-abi-probe.md) | Landlock ruleset and ABI probe | complete | high |
| [0011](0011-network-namespace-isolation.md) | Network namespace isolation | complete | high |
| [0012](0012-sandbox-escape-suite.md) | Sandbox escape suite | complete | high |
| [0013](0013-llm-gateway-socket-proxy.md) | LLM gateway socket proxy | complete | high |
| [0014](0014-ebpf-priority-clamping-and-audit-probes.md) | eBPF priority clamping and audit probes | in-progress | high |
| [0015](0015-slot-pool-cgroup-freezer-and-gitoxide.md) | Slot pool, cgroup freezer, and gitoxide worktrees | planned | high |
| [0016](0016-zed-agent-communication-protocol.md) | Zed Agent Communication Protocol (ACP) server | planned | high |
| [0017](0017-fastcache-prefix-hashing-and-affinity-routing.md) | FastCache prefix hashing and affinity routing | planned | high |
| [0018](0018-mad-stack-operator-deck-and-settle-queue.md) | MAD stack operator deck and settle queue | planned | high |
| [0019](0019-pool-runtime-decision.md) | Pool runtime decision | complete | high |
| [0020](0020-build-job-cap.md) | Build job cap | planned | high |
| [0021](0021-cas-storage-scope-and-phase.md) | uas-storage scope and phase | planned | medium |
| [0022](0022-custody-landing.md) | Custody landing | planned | high |
| [0023](0023-test-time-compute-and-verifiable-reward.md) | Test-time compute and verifiable reward | planned | medium |
| [0024](0024-estate-saga-cpm-and-agent-sprint-wave-strategy.md) | Estate saga CPM and agent sprint wave strategy | complete | high |
| [0025](0025-wave-execution-on-the-custody-chain.md) | Wave execution on the custody chain | planned | high |
| [0026](0026-cortex-pool-member-adapter.md) | Cortex pool-member adapter | planned | high |
| [0028](0028-branchless-crdt-worktrees.md) | Branchless CRDT worktrees | planned | high |
| [0029](0029-agent-identity-and-on-behalf-of-delegation.md) | Agent identity and on-behalf-of delegation | planned | high |
| [0030](0030-a2a-multi-agent-interop.md) | A2A multi-agent interop | planned | high |
| [0031](0031-mcp-security-baseline.md) | MCP security baseline | planned | high |
| [0032](0032-reverie-access-from-a-jailed-slot.md) | Reverie access from a jailed slot | planned | high |
| [0033](0033-cargo-feature-policy-and-ci-matrix.md) | Cargo feature policy and CI matrix | planned | high |
| [0034](0034-tool-pinning-beyond-cargo.md) | Tool pinning beyond cargo | planned | high |
| [0035](0035-cas-acp-transport-scope.md) | uas-acp transport scope | planned | high |
| [0036](0036-slot-arbitration-across-processes.md) | Slot arbitration across processes | planned | high |
| [0037](0037-repository-migration-to-uas.md) | Repository migration to uas | planned | high |
| [0038](0038-uas-operator-and-crds.md) | uas-operator and CRDs | planned | high |
| [0039](0039-k8s-dependency-and-ci-plan.md) | k8s dependency and CI plan | planned | high |
| [0040](0040-pool-restart-and-orphan-reclamation.md) | Pool restart and orphan reclamation | in-progress | high |
| [0041](0041-slot-memory-pids-and-disk-limits.md) | Slot memory, pids and disk limits | in-progress | high |
| [0042](0042-cas-binary-conventions.md) | uas binary conventions | complete | high |
| [0043](0043-seccomp-syscall-filter-for-slot-workers.md) | seccomp syscall filter for slot workers | complete | high |
| [0044](0044-reverie-client-degradation.md) | Reverie client degradation | planned | high |
| [0045](0045-security-disclosure-and-advisory-policy.md) | Security disclosure and advisory policy | planned | high |
| [0046](0046-saga-team-and-topology-files.md) | Saga team and topology files | planned | high |
| [0047](0047-slot-build-cache-retention.md) | Slot build cache retention | planned | high |
| [0048](0048-licence-and-publication.md) | Licence and publication | in-progress | high |
| [0049](0049-cpm-migration-to-saga.md) | CPM migration to saga | planned | high |
| [0050](0050-build-handoff-pipeline.md) | Build handoff pipeline | complete | high |
| [0051](0051-repository-guards.md) | Repository guards | planned | high |
| [0052](0052-spec-check-rule-set.md) | Spec-check rule set | planned | medium |
| [0053](0053-changelog-and-commit-text-policy.md) | CHANGELOG and commit text policy | planned | medium |


## How to add a spec

1. Take the next free number. Use four digits. Example: `0006`.
2. Copy the template from `../vendor/leanspec/spec-template.md`.
3. Name the file `NNNN-kebab-slug.md`. Make the H1 read `# NNNN-kebab-slug`.
4. Keep the sections in this order: Overview, Design, Plan, Test, Notes.
5. Put one runnable command in a fenced block under Test.
6. Keep the file under 300 lines. Split a long spec into sub-spec files.
   See `../vendor/leanspec/sub-spec-files.md`.
7. Add one row to the table above, with a relative link.
8. Run `make spec-check`. It must pass.
