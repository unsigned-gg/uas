---
status: planned
created: 2026-10-10
tags: [k8s, kube-rs, dependencies, pins, ci, k3d, publish]
priority: high
depends_on: [0004-dependency-and-licence-policy, 0005-ci-cache-and-runners, 0033-cargo-feature-policy-and-ci-matrix, 0034-tool-pinning-beyond-cargo, 0037-repository-migration-to-uas, 0038-uas-operator-and-crds]
---

# 0039-k8s-dependency-and-ci-plan

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

The Kube-rs Integration Plan v5 gives this repository a dependency set it has
never had: `kube`, `k8s-openapi`, `schemars`, and two estate crates that do not
exist on crates.io yet, `unsigned-k8s-core` and `unsigned-k8s-api` [reported: the
plan, at `<surfaces>/uas-ears/kube-rs-plan.md`]. AGENTS.md rule 12
says every external dependency is pinned and recorded in spec 0004 before first
use. This spec says what is pinned, at which versions and why, what the licence
check finds, how the two unpublished crates are reached before publication, how
the end-to-end job runs in CI, and who publishes what. It picks no code.

Facts, measured 2026-10-10 in a throwaway crate outside the worktree, with this
repository's `deny.toml` copied in:

- **F1 [verified]:** `kube` 4.2.0 is the newest release and declares
  `rust_version = 1.89.0`, which equals the workspace `rust-version = "1.89"`
  exactly, with no headroom. Command: `cargo metadata --format-version 1` on the
  throwaway crate; `grep rust-version Cargo.toml` in the workspace.
- **F2 [verified]:** `k8s-openapi` 0.28.0 is the newest release (published
  2026-06-15), declares no `rust_version`, and offers the Kubernetes version
  features `v1_32` through `v1_36`. No release offers `v1_37`. Commands:
  `cargo metadata` as above; `curl https://crates.io/api/v1/crates/k8s-openapi`.
- **F3 [verified]:** The Cygnus cluster runs Kubernetes **1.37.0**, client and
  server. Command: `KUBECONFIG=~/.kube/cygnus kubectl version`. An earlier draft
  of this spec chose `v1_32` without reading the cluster; that choice is withdrawn.
- **F4 [verified]:** `schemars` 1.2.2 declares `rust_version = 1.74` and licence
  `MIT`. `kube` and `k8s-openapi` are `Apache-2.0`. Command: `cargo metadata`.
- **F5 [verified]:** With this repository's `deny.toml`, the set fails the
  licence gate on one transitive crate: `webpki-root-certs`, licence
  `CDLA-Permissive-2.0`, "rejected: license is not explicitly allowed".
  Advisories, bans and sources pass. Command: `cargo deny check` and
  `cargo deny check licenses` in the throwaway crate.
- **F6 [reported]:** The two estate crates are path dependencies into
  `<home>/projects/unsigned/paas` first, where they are scaffolded, and are published
  later; `kube 4.2.0` and `k8s-openapi 0.28.0` were chosen there to match the
  `vespertin` operator and were not checked against this repository before F1
  to F5 [research lane w1X:p22, mesh reply 2026-10-10 about 21:5xZ].
- **F7 [reported]:** The crates.io names `uas`, `cerebral-work` and
  `cerebral-k8s` are held by `todie`. `vesperan` and `zerodays` hit the 24-hour
  lock after a deletion and are due for republish at about 20:56Z on 2026-10-11
  [research lane w1X:p22, same reply]. `uas` 0.1.0 is also verified in spec 0037
  F6. `unsigned-k8s-core` and `unsigned-k8s-api` are held by nobody.
- **F8 [reported]:** the operator ruled in his own session at about 21:38Z on
  2026-10-10 that the `uas` crate is the vehicle for publishing substrate work
  [this lane, w25:p1, heard it; spec 0037 D13 records it].

## Design

### Decisions

| | Decision | Source |
|---|---|---|
| D1 | Pin `kube` 4.2.0 with features `runtime` and `derive`, `k8s-openapi` 0.28.0 with feature `v1_36`, and `schemars` 1.2.2. `v1_36` is the newest feature any release offers and sits one minor behind the 1.37 cluster. That a one-minor skew is supported is this lane's reading of how `k8s-openapi` version features are used [asserted]; it is revisited when a release carries `v1_37`. | F1 to F4; integrating lane w25:p1, 2026-10-10, taking over from lane w10:pN, whose pane ended before round two |
| D2 | `kube`'s MSRV equals the workspace floor exactly. The floor is not raised for it, and any bump of `kube` that raises its MSRV above 1.89 is a decision for spec 0004, not an incidental update. | F1; integrating lane w25:p1, 2026-10-10 |
| D3 | Spec 0004 gains an allow entry for `CDLA-Permissive-2.0`, scoped by crate to `webpki-root-certs`, because F5 proves the set cannot pass without it. Spec 0004 line 205 already allows that licence for a wasm32 path; this is the Linux path, and the entry says so. | F5; integrating lane w25:p1, 2026-10-10 |
| D4 | The two estate crates do **not** enter any manifest until they are published. A cargo path dependency must exist at resolve time even when optional, so a CI runner or another checkout without `<home>/projects/unsigned/paas` would fail `--locked` builds and `cargo deny` with the feature off; "path dependency behind a feature" does not work. While they are unpublished, a workstation that needs them uses a `[patch]` section in a local, untracked `.cargo/config.toml`, and the operator's CRD types stay in-tree (spec 0038 D6). | F6; spec 0038 D6; integrating lane w25:p1, 2026-10-10; corrects an earlier draft of this row that assumed a feature could hide a path dependency |
| D4a | Two features, not one. `k8s` gates everything that builds from crates.io alone: `kube`, `k8s-openapi`, `schemars` and the in-tree CRD types, and it is ON in the CI matrix of spec 0033, so the operator's mocked-API tests run on every pull request. `k8s-estate` gates the two estate crates once published, and stays off in CI until then. Without this split no Kubernetes test would ever run in CI. | integrating lane w25:p1, 2026-10-10 |
| D5 | The `k8s-e2e` job runs on the existing self-hosted runner group `ent-default` with the `ci-rust` image, which gains `k3d` and `kubectl`. No GitHub-hosted runner: spec 0005 has none, and one would change cost, secrets reach and the self-hosted assumption. Spec 0037 F7 notes the group's organisation could not be read, so the job may be re-homed after the transfer. | spec 0005; integrating lane w25:p1, 2026-10-10 |
| D6 | Every Kubernetes-facing piece sits behind the cargo feature `k8s`, declared per spec 0033 in each crate that owns gated code, off by default. | spec 0033; integrating lane w25:p1, 2026-10-10 |
| D7 | `k3d` and `kubectl` are pinned as tools beyond cargo under spec 0034, in spec 0004's CI tools table. | spec 0034; integrating lane w25:p1, 2026-10-10 |
| D8 | **Publishing.** The `uas` crate is the publication vehicle for substrate work (F8). crates.io has no namespaces: `uas` is one name, and every `uas-*` sub-crate is a separate name, unreserved until published. Which workspace crates publish and under which names is decided when the first publish is prepared, in a spec that amends 0004's `publish = false`. Publishing, and reserving any further name, is the operator's act; no lane presses publish. | F7, F8; spec 0037 D13; integrating lane w25:p1, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0039-01]** (Ubiquitous): The workspace shall depend on `kube` 4.2.0 with features `runtime` and `derive`, `k8s-openapi` 0.28.0 with feature `v1_36`, and `schemars` 1.2.2, each recorded in spec 0004 before the first `Cargo.toml` edit that uses it.
- **[REQ-0039-02]** (Ubiquitous): `deny.toml` shall allow `CDLA-Permissive-2.0` for `webpki-root-certs` only, and spec 0004 shall record that entry with F5 as its reason.
- **[REQ-0039-03]** (State-driven): WHILE `unsigned-k8s-core` and `unsigned-k8s-api` are unpublished, no tracked manifest shall name them, and a workstation shall reach them only through an untracked local `[patch]` entry.
- **[REQ-0039-04]** (Event-driven): WHEN the two estate crates are published, the workspace shall pin them by version recorded in spec 0004, and the path dependencies shall be removed in the same change.
- **[REQ-0039-05]** (Ubiquitous): Every crate with Kubernetes-facing code shall declare the cargo feature `k8s` for the crates.io dependency set and the in-tree CRD types, and `k8s-estate` for the two estate crates, both off by default and both declared per spec 0033.
- **[REQ-0039-05a]** (Ubiquitous): The CI feature matrix of spec 0033 shall carry a `k8s` leg on every pull request, so the operator's tests against a mocked API server run in CI.
- **[REQ-0039-06]** (Event-driven): WHEN the `k8s` leg of the matrix runs, the `k8s-e2e` job shall run on the `ent-default` runner group with the `ci-rust` image and shall provision a `k3d` cluster.
- **[REQ-0039-07]** (Ubiquitous): `k3d` and `kubectl` shall be pinned in spec 0004's CI tools table per spec 0034 before the `ci-rust` image gains them.
- **[REQ-0039-08]** (Unwanted): IF `make pin-check` finds a Kubernetes crate in a manifest that spec 0004 does not record, THEN it shall fail and name the crate.
- **[REQ-0039-09]** (Unwanted): IF a release of `kube` raises its MSRV above the workspace `rust-version`, THEN the bump shall wait for a spec 0004 decision on the floor.
- **[REQ-0039-10]** (Unwanted): IF a lane's change would publish any crate or reserve any crates.io name, THEN the lane shall stop and leave that act to the operator.
- **[REQ-0039-11]** (Optional): WHERE a `k8s-openapi` release offers `v1_37`, the feature shall move to it and spec 0004 shall record the bump.

## Plan

In this order, because rule 12 records before it uses:

- [ ] Amend spec 0004: rows for `kube` 4.2.0, `k8s-openapi` 0.28.0, `schemars` 1.2.2 with licence and MSRV from F1 to F4; the `CDLA-Permissive-2.0` allow for `webpki-root-certs` with F5 as reason; `k3d` and `kubectl` in the CI tools table.
- [ ] Update `deny.toml` with the scoped allow.
- [ ] Add the three crates to the workspace behind the `k8s` feature, in the crates spec 0038 names.
- [ ] Document the untracked local `[patch]` form for the two estate crates in `docs/`, and add nothing for them to any tracked manifest.
- [ ] Add `k3d` and `kubectl` to the `ci-rust` image provisioning, pinned.
- [ ] Add the `k8s-e2e` job to `ci.yml` on `ent-default`, gated on the `k8s` feature being on, per spec 0033's matrix.
- [ ] When the estate crates publish: replace the path dependencies with version pins and record them in spec 0004.

## Test

```bash
cargo deny check licenses && \
  make pin-check JOBS=2 && \
  cargo build -p uas-operator -j 2 --features k8s && \
  make features JOBS=2
```

A pass requires each of these cases to exist and to go red when its own check is
deleted:

| Case | Where | Requirement |
|---|---|---|
| `cargo deny check licenses` fails when the `webpki-root-certs` allow is removed from `deny.toml` | workspace | REQ-0039-02 |
| `make pin-check` fails on a fixture manifest that adds `kube` with no spec 0004 row | xtask fixture | REQ-0039-08 |
| the `k8s-e2e` job fails when `k3d` is absent from the image | `ci.yml`, run with the tool removed | REQ-0039-06 |
| the default build has no `kube` in its graph: `! cargo tree -e normal \| grep -q kube` with the feature off | workspace | REQ-0039-05 |
| the `k8s` leg exists in the matrix and runs the operator's mocked-API tests | `make features` | REQ-0039-05a |
| the feature matrix of spec 0033 has a `k8s` leg once the feature is declared | `make features` | REQ-0039-05 |

The control that exists today: with this repository's `deny.toml` and no allow
entry, the licence gate fails on `webpki-root-certs` (F5). The spec is
implemented when the same command passes with the scoped allow and fails again
when the allow is removed.

## Notes

- **Takeover.** This spec was drafted by the uas spec pane w10:pN. That pane's
  workspace was re-laid before its second review round and its mesh role was
  gone, so the integrating lane applied the critic's round-two points itself and
  re-verified every fact above with its own commands. The pane's own measurements
  stand where they were repeated.
- **Why not `v1_37`.** It does not exist in any `k8s-openapi` release as of
  2026-10-10 (F2). `v1_36` is the newest; that a one-minor skew against the
  server is acceptable is asserted here, not sourced. REQ-0039-11 moves it when
  a release carries `v1_37`.
- **Why not a GitHub-hosted runner.** Spec 0005 has none, and the first one would
  be a cost and secrets decision, not a CI detail.
- **The MSRV coincidence.** `kube` 4.2.0 needs exactly the floor the workspace
  has. The next `kube` is likely to raise it, and that is a spec 0004 decision
  (D2, REQ-0039-09), not something `cargo update` is allowed to decide.
- Reading: spec 0004 for the pin tables and line 205 on `CDLA-Permissive-2.0`;
  spec 0033 for the feature matrix; spec 0034 for tools beyond cargo; spec 0037
  D13 and F6 for the `uas` crate; spec 0038 D6 for the in-tree CRD window.
