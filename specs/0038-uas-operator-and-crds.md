---
status: planned
created: 2026-10-10
tags: [k8s, operator, crd, pool, sandbox, uas]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0004-dependency-and-licence-policy, 0007-lease-rules, 0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation, 0013-llm-gateway-socket-proxy, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0022-custody-landing, 0033-cargo-feature-policy-and-ci-matrix, 0036-slot-arbitration-across-processes, 0037-repository-migration-to-uas, 0039-k8s-dependency-and-ci-plan]
---

# 0038-uas-operator-and-crds

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

The Kube-rs Integration Plan v5 proposes integrating the core compute stack (`soma`, `saga`, `cortex`, `uas` [formerly `cas`], `reverie`) with Kubernetes using `kube-rs` [reported: `kube-rs-plan.md` v5; artifact approval by the operator at ~21:12Z in pane w1X:p22]. Under the split-monorepo architecture, the open compute runtime lives in `unsigned-gg/uas`, running `uas-operator` and consuming the public custom resources `uas::WorkerSlot` and `uas::AgentSandbox` [reported: `kube-rs-plan.md` v5].

This specification defines the crate architecture, reconciliation mapping, lifecycle semantics, and safety boundaries for `uas-operator` within this repository. Work on this specification proceeds under the operator's explicit exemption for the migration roadmap during the estate-wide development hold [reported: mesh 2bc37b9a]. Under Spec 0037 (repository migration to `unsigned-gg/uas`), `crates/cas-operator` becomes `crates/uas-operator` in one cutover, aligning with the `uas.unsigned.gg` finalizer and API domain namespace.

Facts, measured 2026-10-10 on ceres:

- **F1 [reported]:** `kube-rs-plan.md` v5 outlines the `uas-operator` binary in `unsigned-gg/uas`, backed by shared plumbing in `unsigned-k8s-core` and open schemas in `unsigned-k8s-api` [reported: artifact approved by the operator at ~21:12Z in author pane w1X:p22].
- **F2 [reported]:** Spec 0015 D1 and Spec 0036 D1-D3 define worker slot allocation, cgroup freezer management, and host-wide slot arbitration via advisory lock files under the Spec 0036 D2 slot directory (`/tmp/cas-slots` default, `CAS_SLOT_DIR` override).
- **F3 [reported]:** Specs 0010, 0011, 0013, and 0032 define Landlock rulesets, private network namespace isolation, and IPC proxy socket paths (`.ipc/llm.sock`, `.ipc/reverie.sock`).
- **F4 [verified, 2026-10-10]:** `cas-domain` contains zero I/O imports [verified: `make smoke JOBS=2` grep check prints `smoke: ok`]. Layering defined in Spec 0001 dictates `deck > acp > engine > pool > sandbox, ebpf > router > domain`.
- **F5 [reported]:** Platform operator deployment and workload syncing across cluster boundaries are operator-gated actions requiring explicit human sign-off [reported: `<home>/.agents/sops/land-on-green-run-it.md`, `<home>/.agents/sops/prod-is-dev.md`].
- **F6 [reported]:** Unpublished estate crates `unsigned-k8s-core` and `unsigned-k8s-api` will be path dependencies into `<home>/projects/unsigned/paas` first, not published crates [reported: lane w25:p1, 2026-10-10].

## Design

`uas-operator` lives in a dedicated binary crate `crates/cas-operator`, which becomes `crates/uas-operator` under Spec 0037 in the same cutover. Layering holds strictly in AGENTS.md order: the operator sits below `cas-deck` and `cas-acp`, beside `cas-engine` (reconcilers invoke `cas-engine` lease and activity paths), and above `cas-pool` and `cas-sandbox`. `cas-domain` remains strictly zero I/O [verified: `make smoke JOBS=2`] and free of `kube` dependencies [verified: `cargo tree -p cas-domain | grep -c kube == 0`]. `cas-deck` remains the operator web surface and settle queue, keeping user interfaces decoupled from Kubernetes reconciliation loops.

Per Spec 0033 and Spec 0039, all Kubernetes controller dependencies and runtime paths sit behind cargo feature flag `k8s`.

The operator reconciles two primary Custom Resource Definitions (CRDs):
1. `WorkerSlot`: Reconciles to `cas-pool` worker slot allocation (Spec 0015 D1), host slot advisory locking (Spec 0036 D2-D3), and lease tracking (Spec 0007).
2. `AgentSandbox`: Reconciles to sandbox execution specifications translating Landlock path profiles (enforced in-process by the sandboxed runtime per Spec 0010), network namespace loopback isolation (Spec 0011), and IPC socket mounts (Specs 0013, 0032).

### Decisions

| # | Decision | Rationale | Source |
|---|---|---|---|
| D1 | `uas-operator` lives in dedicated binary crate `crates/cas-operator` (renamed `crates/uas-operator` under Spec 0037). It sits below `cas-deck` and `cas-acp`, beside `cas-engine` (invoking lease paths), and above `cas-pool` and `cas-sandbox`. `cas-domain` remains zero I/O and free of `kube` dependencies. | Crate layering purity and separation of CLI from continuous reconciliation; Spec 0001; Spec 0037 | proposed: CAS spec lane w1X:p1, 2026-10-10; awaits the operator confirmation (w1X:p22 notes location not decided yet; vespertin operator at <home>/projects/unsigned/vespertin/operator) |
| D2 | `WorkerSlot` reconciles to `cas-pool` slot leases and Spec 0036 D2 host locks (`slot-<i>.lock` under the Spec 0036 slot directory: `/tmp/cas-slots` default, `CAS_SLOT_DIR` override). Finalizer `uas.unsigned.gg/slot-cleanup` ensures cgroups, worktrees, and host lock descriptors are reclaimed before CR deletion. | Grounded lifecycle mapping to kernel-arbitrated host slots; Spec 0036 D2; CAS spec lane w1X:p1, 2026-10-10 |
| D3 | `AgentSandbox` reconciles to sandbox execution specifications configuring Landlock path rules, network namespace isolation, and IPC socket mounts (`.ipc`). Landlock self-restriction is applied by the sandboxed worker process at startup per Spec 0010; the controller does not invoke Landlock directly. Finalizer `uas.unsigned.gg/sandbox-cleanup` tears down IPC mounts and network namespaces. | Privilege separation; Landlock self-restriction constraint; Spec 0010; CAS spec lane w1X:p1, 2026-10-10 |
| D4 | Host lock authority on split-brain: If cluster scheduling demands exceed available host slot locks, the operator refuses oversubscription, sets status condition `CapacityExhausted`, and retries with backoff. Physical `-j 2` constraints are never violated. | Host throttling invariant; Spec 0002; Spec 0036; CAS spec lane w1X:p1, 2026-10-10 |
| D5 | Inviolable operator boundaries: `uas-operator` never lands code to Git branches, never manages raw credentials directly, and never bypasses the Spec 0022 custody chain. | Blast radius containment; Spec 0022; CAS spec lane w1X:p1, 2026-10-10 |
| D6 | Transitional CRD placement: CRDs are defined in internal module `crates/cas-operator/src/crd.rs` until `unsigned-k8s-api` is adopted. Dependency on `unsigned-k8s-api` requires a Spec 0004 pin approval before adoption. | Decoupled migration cadence; F6 path dependency staging; CAS spec lane w1X:p1, 2026-10-10 |
| D7 | Operator deployment to the Cygnus cluster is an operator-gated action. An agent lane never deploys or syncs to Cygnus autonomously; deploying is the operator's act. | Estate safety SOP; <home>/.agents/sops/land-on-green-run-it.md; <home>/.agents/sops/prod-is-dev.md; CAS spec lane w1X:p1, 2026-10-10 |
| D8 | Operator execution topology: The physical deployment topology of `uas-operator` is marked undecided between a cluster-wide controller (using a node-agent DaemonSet with hostPath to reach the Spec 0036 mode 0700 slot directory) and a node-local DaemonSet. | Node privilege and filesystem access boundaries; Spec 0036 D2; CAS spec lane w1X:p1, 2026-10-10 |
| D9 | Path dependencies local-only; k8s feature disabled in CI: Path dependencies into `<home>/projects/unsigned/paas` for `unsigned-k8s-*` crates exist only on local developer environments. Because CI runners lack this path and `deny.toml` restricts sources to crates.io and one git source, the `k8s` cargo feature remains disabled in default CI jobs until crates are published (or vendored/git-pinned), with in-tree CRD definitions in `crd.rs` providing standalone compilation. | CI portability and deny.toml source rules; Spec 0004; Spec 0039; CAS spec lane w1X:p1, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0038-01]** (Ubiquitous): The `cas-operator` shall implement a Kubernetes controller reconciling `WorkerSlot` and `AgentSandbox` custom resources behind cargo feature flag `k8s`.
- **[REQ-0038-02]** (Ubiquitous): The `cas-operator` shall be structured as a dedicated binary crate sitting below `cas-deck` and `cas-acp`, beside `cas-engine`, and above `cas-pool` and `cas-sandbox`, maintaining `cas-domain` zero I/O and free of `kube` crate dependencies.
- **[REQ-0038-03]** (Event-driven): WHEN a `WorkerSlot` custom resource is created or updated, the operator shall allocate a worker slot via `cas-pool`, acquiring a host lock file under the Spec 0036 slot directory per Spec 0036 D2.
- **[REQ-0038-04]** (State-driven): WHILE a `WorkerSlot` resource is actively reconciled, the operator shall update its status with assigned slot ID, host slot index, and lease conditions.
- **[REQ-0038-05]** (Event-driven): WHEN a `WorkerSlot` resource is deleted, the operator shall execute finalizer `uas.unsigned.gg/slot-cleanup`, releasing the Spec 0036 host lock and cleaning up cgroup freezer hierarchies.
- **[REQ-0038-06]** (Event-driven): WHEN an `AgentSandbox` custom resource is created or updated, the operator shall generate the sandbox execution specification containing Landlock filesystem path allowances, network namespace isolation, and IPC proxy socket mounts per Specs 0010, 0011, 0013, and 0032.
- **[REQ-0038-07]** (Unwanted): IF a node's Spec 0036 host slot capacity is exhausted, THEN the operator shall set condition `CapacityExhausted` on the `WorkerSlot` and back off without exceeding host limits.
- **[REQ-0038-08]** (Ubiquitous): The operator shall not execute Git branch merges or pull request landings.
- **[REQ-0038-09]** (Ubiquitous): The operator shall not read, store, or manage raw secret credentials directly.
- **[REQ-0038-10]** (Unwanted): IF a reconciliation action would bypass Spec 0022 custody landing receipts, THEN the operator shall halt reconciliation with a terminal error condition.
- **[REQ-0038-11]** (Ubiquitous): The operator shall define CRD types in an in-tree module until `unsigned-k8s-api` is pinned in Spec 0004 and published.
- **[REQ-0038-12]** (Ubiquitous): The operator shall not autonomously deploy, mutate, or sync workloads in the Cygnus cluster without explicit human sign-off.

## Plan

- [ ] Scaffold `crates/cas-operator` binary crate with `kube` runtime dependencies behind `k8s` feature flag (renamed `crates/uas-operator` per Spec 0037).
- [ ] Implement transitional CRD structs in `crates/cas-operator/src/crd.rs` for `WorkerSlot` and `AgentSandbox`.
- [ ] Implement `WorkerSlot` reconciler integrating with `cas-engine` lease paths, `cas-pool` slot allocation, Spec 0036 D2 host lock files, and cleanup finalizers.
- [ ] Implement `AgentSandbox` reconciler generating sandbox execution specifications (Landlock path rules, netns, IPC proxy socket mounts).
- [ ] Add unit and mocked controller tests using `kube::client::Client` mock servers verifying status condition transitions and capacity backoff with `--features k8s`.
- [ ] Pin `unsigned-k8s-api` in `specs/0004-dependency-and-licence-policy.md` when the crate is published.

## Test

```bash
cargo test -p cas-operator --features k8s -j 2 --test crd_reconciliation
cargo tree -p cas-domain | grep -c kube
! cargo tree -p cas-operator --features k8s | grep -E 'gix|git2'
```

A passing suite verifies that `WorkerSlot` and `AgentSandbox` resources reconcile correctly against mocked Kubernetes API servers behind `--features k8s` (REQ-0038-01, REQ-0038-03, REQ-0038-04, REQ-0038-06). It verifies finalizer cleanup execution (REQ-0038-05), capacity backoff handling (REQ-0038-07), and domain isolation (REQ-0038-02).

Runtime Landlock filesystem enforcement and path escape prevention are proven in `cas-sandbox` under Spec 0012; the operator test suite verifies that the `AgentSandbox` CR specification is translated into the exact `cas-sandbox` Landlock ruleset request structure (allowed paths, access flags, and network namespace configuration).

A pass requires each of these negative control cases to exist and to go red when its own check is deleted:

| Case | Where | Requirement |
|---|---|---|
| saturated host slots cause `WorkerSlot` to enter `CapacityExhausted` | `cas-operator` mocked K8s test | REQ-0038-03, REQ-0038-07 |
| deleting `WorkerSlot` triggers finalizer and drops Spec 0036 host lock | `cas-operator` mocked K8s test | REQ-0038-05 |
| `AgentSandbox` CR spec is translated into `cas-sandbox` ruleset request | `cas-operator` mocked K8s test | REQ-0038-06 |
| operator crate links no Git merge or PR landing libraries (`! cargo tree -p cas-operator \| grep -E 'gix\|git2'`) | `cas-operator` architecture test | REQ-0038-08 |
| operator crate contains no raw credential access imports | `cas-operator` source scan | REQ-0038-09 |
| reconciliation halts with terminal error if custody receipt bypassed | `cas-operator` mocked K8s test | REQ-0038-10 |
| `cas-domain` has zero kube dependencies (`cargo tree -p cas-domain \| grep -c kube == 0`) | `cas-domain` dependency check | REQ-0038-02 |
| operator rejects unauthorized Cygnus cluster mutation | `k8s-e2e` job of Spec 0039 [unchecked: pending k3d runner] | REQ-0038-12 |

The primary negative control: if the host slot acquisition call in the `WorkerSlot` reconciler is removed, the test asserting that `WorkerSlot` status reflects `CapacityExhausted` when the Spec 0036 slot directory is saturated must go red.

## Notes

- Integration Plan provenance: Derived from `kube-rs-plan.md` v5 (authored by `estate/w1X:p22`, artifact approval by the operator at ~21:12Z in pane w1X:p22) [reported].
- Hold exemption: Drafted under the operator's explicit exemption for the migration roadmap during the estate-wide development hold [reported: mesh 2bc37b9a].
- Relation to Spec 0036: Reconciles directly to the advisory lock files defined in Spec 0036 D2, anchoring cluster scheduler demand to physical node capacity.
- Relation to Spec 0037: Synchronizes crate naming (`cas-operator` -> `uas-operator`) and domain finalizers (`uas.unsigned.gg`) with repository cutover.
- Relation to Spec 0039: Aligns feature flag gating (`--features k8s`), dependency staging, and `k8s-e2e` verification.
- Layering preservation: Preserves `cas-domain` zero I/O [verified: `make smoke JOBS=2`] and absence of `kube` dependencies [verified: `cargo tree -p cas-domain | grep -c kube == 0`].
