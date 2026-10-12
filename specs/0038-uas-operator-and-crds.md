---
status: planned
created: 2026-10-10
tags: [k8s, operator, crd, pool, sandbox, uas]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0002-verification-gates-and-evidence, 0004-dependency-and-licence-policy, 0007-lease-rules, 0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation, 0012-sandbox-escape-suite, 0013-llm-gateway-socket-proxy, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0022-custody-landing, 0032-reverie-access-from-a-jailed-slot, 0033-cargo-feature-policy-and-ci-matrix, 0036-slot-arbitration-across-processes, 0037-repository-migration-to-uas, 0042-cas-binary-conventions]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0038-uas-operator-and-crds

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

The Kube-rs Integration Plan v5 proposes integrating the core compute stack (`soma`, `saga`, `cortex`, `uas` [formerly `uas`], `reverie`) with Kubernetes using `kube-rs` [reported: `kube-rs-plan.md` v5; artifact approval by the operator at ~21:12Z in pane w1X:p22]. Under the split-monorepo architecture, the open compute runtime lives in `unsigned-gg/uas`, running `uas-operator` and consuming the public custom resources `uas::WorkerSlot` and `uas::AgentSandbox` [reported: `kube-rs-plan.md` v5].

This specification defines the crate architecture, reconciliation mapping, lifecycle semantics, and safety boundaries for `uas-operator` within this repository. Work on this specification proceeds under the operator's explicit exemption for the migration roadmap during the estate-wide development hold [reported: mesh 2bc37b9a]. Under Spec 0037 (repository migration to `unsigned-gg/uas`), `crates/uas-operator` becomes `crates/uas-operator` in one cutover, aligning with the `uas.unsigned.gg` finalizer and API domain namespace.

Facts, measured 2026-10-10 on ceres:

- **F1 [reported]:** `kube-rs-plan.md` v5 outlines the `uas-operator` binary in `unsigned-gg/uas`, backed by shared plumbing in `unsigned-k8s-core` and open schemas in `unsigned-k8s-api` [reported: artifact approved by the operator at ~21:12Z in author pane w1X:p22].
- **F2 [reported]:** Spec 0015 D1 and Spec 0036 D1-D3 define worker slot allocation, cgroup freezer management, and host-wide slot arbitration via advisory lock files under the Spec 0036 D2 slot directory (`/tmp/uas-slots` default, `UAS_SLOT_DIR` override).
- **F3 [reported]:** Specs 0010, 0011, 0013, and 0032 define Landlock rulesets, private network namespace isolation, and IPC proxy socket paths (`.ipc/llm.sock`, `.ipc/reverie.sock`).
- **F4 [verified, 2026-10-10]:** `uas-domain` contains zero I/O imports [verified: `make smoke JOBS=2` grep check prints `smoke: ok`]. Layering defined in Spec 0001 dictates `deck > acp > engine > pool > sandbox, ebpf > router > domain`.
- **F5 [reported]:** Platform operator deployment and workload syncing across cluster boundaries are operator-gated actions requiring explicit human sign-off [reported: the estate SOP `land-on-green-run-it`, the estate SOP `prod-is-dev`].
- **F6 [reported]:** Unpublished estate crates enter no tracked manifest until published to crates.io, avoiding Cargo workspace resolution failures on CI runners lacking local paas paths [reported: Spec 0039 D4; lane w25:p1, 2026-10-10].

## Design

`uas-operator` lives in a dedicated binary crate `crates/uas-operator`, which becomes `crates/uas-operator` under Spec 0037 in the same cutover. Layering holds strictly in AGENTS.md order: the operator sits below `uas-deck` and `uas-acp`, beside `uas-engine` (reconcilers invoke `uas-engine` lease and activity paths), and above `uas-pool` and `uas-sandbox`. `uas-domain` remains strictly zero I/O [verified: `make smoke JOBS=2`] and free of `kube` dependencies [verified: `cargo tree -p uas-domain | grep -c kube == 0`]. `uas-deck` remains the operator web surface and settle queue, keeping user interfaces decoupled from Kubernetes reconciliation loops.

Per Spec 0033 and Spec 0039 D4a, all Kubernetes controller dependencies, in-tree CRD types, and mocked reconciliation test suites sit behind cargo feature flag `k8s` (enabled in the CI matrix), while future estate crate integration is deferred to feature `k8s-estate` until publication.

The operator reconciles two primary Custom Resource Definitions (CRDs):
1. `WorkerSlot`: Reconciles to `uas-pool` worker slot allocation (Spec 0015 D1), host slot advisory locking (Spec 0036 D2-D3), and lease tracking (Spec 0007).
2. `AgentSandbox`: Reconciles to sandbox execution specifications translating Landlock path profiles (enforced in-process by the sandboxed runtime per Spec 0010), network namespace loopback isolation (Spec 0011), and IPC socket mounts (Specs 0013, 0032).

### Decisions

| # | Decision | Rationale | Source |
|---|---|---|---|
| D1 | `uas-operator` lives in dedicated binary crate `crates/uas-operator` (renamed `crates/uas-operator` under Spec 0037). It sits below `uas-deck` and `uas-acp`, beside `uas-engine` (invoking lease paths), and above `uas-pool` and `uas-sandbox`. `uas-domain` remains zero I/O and free of `kube` dependencies. | Crate layering purity and separation of CLI from continuous reconciliation; Spec 0001; Spec 0037 | proposed: research lane w1X:p1, 2026-10-10; awaits the operator confirmation (w1X:p22 notes location not decided yet; vespertin operator at `vespertin/operator`) |
| D2 | `WorkerSlot` reconciles to `uas-pool` slot leases and Spec 0036 D2 host locks (`slot-<i>.lock` under the Spec 0036 slot directory: `/tmp/uas-slots` default, `UAS_SLOT_DIR` override). Finalizer `uas.unsigned.gg/slot-cleanup` ensures cgroups, worktrees, and host lock descriptors are reclaimed before CR deletion. | Grounded lifecycle mapping to kernel-arbitrated host slots; Spec 0036 D2; research lane w1X:p1, 2026-10-10 |
| D3 | `AgentSandbox` reconciles to sandbox execution specifications configuring Landlock path rules, network namespace isolation, and IPC socket mounts (`.ipc`). Landlock self-restriction is applied by the sandboxed worker process at startup per Spec 0010; the controller does not invoke Landlock directly. Finalizer `uas.unsigned.gg/sandbox-cleanup` tears down IPC mounts and network namespaces. | Privilege separation; Landlock self-restriction constraint; Spec 0010; research lane w1X:p1, 2026-10-10 |
| D4 | Host lock authority on split-brain: If cluster scheduling demands exceed available host slot locks, the operator refuses oversubscription, sets status condition `CapacityExhausted`, and retries with backoff. Physical `-j 2` constraints are never violated. | Host throttling invariant; Spec 0002; Spec 0036; research lane w1X:p1, 2026-10-10 |
| D5 | Inviolable operator boundaries: `uas-operator` never lands code to Git branches, never manages raw credentials directly, and never bypasses the Spec 0022 custody chain. | Blast radius containment; Spec 0022; research lane w1X:p1, 2026-10-10 |
| D6 | Transitional CRD placement: CRDs are defined in internal module `crates/uas-operator/src/crd.rs` behind feature flag `k8s`. Dependency on `unsigned-k8s-api` is deferred until publication on crates.io and pin approval in Spec 0004. | Decoupled migration cadence; F6; Spec 0039 D4; research lane w1X:p1, 2026-10-10 |
| D7 | Operator deployment to the Cygnus cluster is an operator-gated action. An agent lane never deploys or syncs to Cygnus autonomously; deploying is the operator's act. | Estate safety SOP; the estate SOP `land-on-green-run-it`; the estate SOP `prod-is-dev`; research lane w1X:p1, 2026-10-10 |
| D8 | Operator execution topology: The physical deployment topology of `uas-operator` is marked undecided between a cluster-wide controller (using a node-agent DaemonSet with hostPath to reach the Spec 0036 mode 0700 slot directory) and a node-local DaemonSet. | Node privilege and filesystem access boundaries; Spec 0036 D2 | undecided: research lane w1X:p1, 2026-10-10; awaits the operator confirmation |
| D9 | Feature split and CI gating: Following Spec 0039 D4/D4a, unpublished estate crates enter no tracked manifest until published (avoiding cargo resolve failures on CI runners lacking local paas paths). The operator and in-tree CRD types sit behind the `k8s` feature, which is enabled in the CI test matrix so mocked-API tests run on every PR. The future `k8s-estate` feature remains off in CI until estate crates publish to crates.io. | CI portability and feature split; Spec 0004; Spec 0039 D4, D4a; research lane w1X:p1, 2026-10-10 |
| D10 | The `WorkerSlot` CRD schema carries `spec.limits` defining `memory_max_bytes`, `memory_high_bytes`, and `pids_max`. These limits flow into `PoolConfig` at runtime and take precedence over the `uas-pool` environment twins (`UAS_SLOT_MEMORY_MAX`, etc.). | Declarative per-slot sizing; Spec 0041 D8; research lane w1X:p1, 2026-10-10 |

| D11 | `uas-operator` follows Spec 0042 for flags, logging, stdout, exit codes, and `--version`, with telemetry named by 0038 itself using the `unsigned-k8s-core` observability abstraction. | Standardized estate telemetry and CLI conventions; Spec 0042 D6; research lane w1X:p1, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0038-01]** (Ubiquitous): The `uas-operator` shall implement a Kubernetes controller reconciling `WorkerSlot` and `AgentSandbox` custom resources behind cargo feature flag `k8s`.
- **[REQ-0038-02]** (Ubiquitous): The `uas-operator` shall be structured as a dedicated binary crate sitting below `uas-deck` and `uas-acp`, beside `uas-engine`, and above `uas-pool` and `uas-sandbox`, maintaining `uas-domain` zero I/O and free of `kube` crate dependencies.
- **[REQ-0038-03]** (Event-driven): WHEN a `WorkerSlot` custom resource is created or updated, the operator shall allocate a worker slot via `uas-pool`, acquiring a host lock file under the Spec 0036 slot directory per Spec 0036 D2.
- **[REQ-0038-04]** (State-driven): WHILE a `WorkerSlot` resource is actively reconciled, the operator shall update its status with assigned slot ID, host slot index, and lease conditions.
- **[REQ-0038-05]** (Event-driven): WHEN a `WorkerSlot` resource is deleted, the operator shall execute finalizer `uas.unsigned.gg/slot-cleanup`, releasing the Spec 0036 host lock and cleaning up cgroup freezer hierarchies.
- **[REQ-0038-06]** (Event-driven): WHEN an `AgentSandbox` custom resource is created or updated, the operator shall generate the sandbox execution specification containing Landlock filesystem path allowances, network namespace isolation, and IPC proxy socket mounts per Specs 0010, 0011, 0013, and 0032.
- **[REQ-0038-07]** (Unwanted): IF a node's Spec 0036 host slot capacity is exhausted, THEN the operator shall set condition `CapacityExhausted` on the `WorkerSlot` and back off without exceeding host limits.
- **[REQ-0038-08]** (Ubiquitous): The operator shall not execute Git branch merges or pull request landings.
- **[REQ-0038-09]** (Ubiquitous): The operator shall not read, store, or manage raw secret credentials directly.
- **[REQ-0038-10]** (Unwanted): IF a reconciliation action would bypass Spec 0022 custody landing receipts, THEN the operator shall halt reconciliation with a terminal error condition.
- **[REQ-0038-11]** (Ubiquitous): The operator shall define CRD types in an in-tree module behind feature flag `k8s` until `unsigned-k8s-api` is pinned in Spec 0004 and published.
- **[REQ-0038-12]** (Ubiquitous): The operator shall not autonomously deploy, mutate, or sync workloads in the Cygnus cluster without explicit human sign-off.
- **[REQ-0038-13]** (Ubiquitous): The `WorkerSlot` CRD shall define `spec.limits` (`memory_max_bytes`, `memory_high_bytes`, `pids_max`), which the operator shall map into `PoolConfig` to override default limits per Spec 0041.

- **[REQ-0038-14]** (Ubiquitous): The `uas-operator` shall follow Spec 0042 for flags, logging, stdout, exit codes, and `--version`, and shall export telemetry using the `unsigned-k8s-core` observability abstraction.

## Plan

- [ ] Scaffold `crates/uas-operator` binary crate with `kube` runtime dependencies behind `k8s` feature flag (renamed `crates/uas-operator` per Spec 0037).
- [ ] Implement transitional CRD structs in `crates/uas-operator/src/crd.rs` for `WorkerSlot` and `AgentSandbox`.
- [ ] Add `spec.limits` (`memory_max_bytes`, `memory_high_bytes`, `pids_max`) to `WorkerSlot` CRD schema and wire them into `PoolConfig`.
- [ ] Implement `WorkerSlot` reconciler integrating with `uas-engine` lease paths, `uas-pool` slot allocation, Spec 0036 D2 host lock files, and cleanup finalizers.
- [ ] Implement `AgentSandbox` reconciler generating sandbox execution specifications (Landlock path rules, netns, IPC proxy socket mounts).
- [ ] Add unit and mocked controller tests using `kube::client::Client` mock servers verifying status condition transitions and capacity backoff with `--features k8s`.
- [ ] Pin `unsigned-k8s-api` in `specs/0004-dependency-and-licence-policy.md` when the crate is published.

## Test

```bash
cargo test -p uas-operator --features k8s -j 2 --test crd_reconciliation
cargo tree -p uas-domain | grep -c kube
```

A passing suite verifies that `WorkerSlot` and `AgentSandbox` resources reconcile correctly against mocked Kubernetes API servers behind `--features k8s` (REQ-0038-01, REQ-0038-03, REQ-0038-04, REQ-0038-06). It verifies finalizer cleanup execution (REQ-0038-05), capacity backoff handling (REQ-0038-07), custody receipt verification (REQ-0038-10), and domain isolation (REQ-0038-02).

Runtime Landlock filesystem enforcement and path escape prevention are proven in `uas-sandbox` under Spec 0012; the operator test suite verifies that the `AgentSandbox` CR specification is translated into the exact `uas-sandbox` Landlock ruleset request structure (allowed paths, access flags, and network namespace configuration).

A pass requires each of these negative control cases to exist and to go red when its own check is deleted:

| Case | Where | Requirement |
|---|---|---|
| saturated host slots cause `WorkerSlot` to enter `CapacityExhausted` | `uas-operator` mocked K8s test | REQ-0038-03, REQ-0038-07 |
| deleting `WorkerSlot` triggers finalizer and drops Spec 0036 host lock | `uas-operator` mocked K8s test | REQ-0038-05 |
| `AgentSandbox` CR spec is translated into `uas-sandbox` ruleset request | `uas-operator` mocked K8s test | REQ-0038-06 |
| reconciliation halts with terminal error if custody receipt bypassed | `uas-operator` mocked K8s test | REQ-0038-10 |
| `uas-domain` has zero kube dependencies (`cargo tree -p uas-domain \| grep -c kube == 0`) | `uas-domain` dependency check | REQ-0038-02 |
| `WorkerSlot` spec.limits take precedence over `uas-pool` environment variables | `uas-operator` mocked K8s test | REQ-0038-13 |

The primary negative control: if the host slot acquisition call in the `WorkerSlot` reconciler is removed, the test asserting that `WorkerSlot` status reflects `CapacityExhausted` when the Spec 0036 slot directory is saturated must go red.

## Notes

- Integration Plan provenance: Derived from `kube-rs-plan.md` v5 (authored by `estate/w1X:p22`, artifact approval by the operator at ~21:12Z in pane w1X:p22) [reported].
- Hold exemption: Drafted under the operator's explicit exemption for the migration roadmap during the estate-wide development hold [reported: mesh 2bc37b9a].
- Relation to Spec 0036: Reconciles directly to the advisory lock files defined in Spec 0036 D2, anchoring cluster scheduler demand to physical node capacity.
- Relation to Spec 0037: Synchronizes crate naming (`uas-operator` -> `uas-operator`) and domain finalizers (`uas.unsigned.gg`) with repository cutover.
- Relation to Spec 0039: Aligns feature flag split (in-tree CRDs and controller under `k8s`; estate crates deferred under `k8s-estate` per Spec 0039 D4/D4a) and CI matrix execution.
- Layering preservation: Preserves `uas-domain` zero I/O [verified: `make smoke JOBS=2`] and absence of `kube` dependencies [verified: `cargo tree -p uas-domain | grep -c kube == 0`].
