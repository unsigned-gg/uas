---
status: planned
created: 2026-10-10
tags:
  - k8s
priority: high
depends_on:
  - 0004-dependency-and-licence-policy
  - 0005-ci-cache-and-runners
  - 0033-cargo-feature-policy-and-ci-matrix
  - 0034-tool-pinning-beyond-cargo
---

# 0039-k8s-dependency-and-ci-plan

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

The Kube-rs Integration Plan introduces a new Kubernetes dependency set to the `cas-substrate` (UAS) open compute stack. This spec defines the candidate versions, licenses, and CI test plan for `kube`, `k8s-openapi`, `schemars`, and two unpublished internal crates (`unsigned-k8s-core`, `unsigned-k8s-api`). It also defines the provisioning of `k3d` and `kubectl` for end-to-end tests, abiding by the dependency and tool pinning rules set in 0004, 0005, 0033, and 0034.

### Facts
- **F1** [verified 2026-10-10]: The candidate versions `kube` v4.2.0 (requires `1.89.0`) and `k8s-openapi` v0.28.0 (no strict MSRV) are compatible with workspace `rust-version = 1.89`. Command run in throwaway crate: `cargo metadata --format-version 1 | grep -o '"name":"kube","version":"4.2.0"[^}]*"rust_version":"1.89.0"'` returned a match.
- **F2** [verified 2026-10-10]: The `webpki-root-certs` crate transitively included by `kube` has a license `CDLA-Permissive-2.0` that fails the default policy. Command: `cargo deny check licenses` inside throwaway crate (with repo's `deny.toml`) outputted: `rejected: license is not explicitly allowed`.
- **F3** [asserted 2026-10-10]: The `v1_32` feature for `k8s-openapi` matches the target Kubernetes cluster version used in Cygnus.
- **F4** [reported 2026-10-10]: Crates namespaces `cerebral-work`, `cerebral-k8s`, and `uas` are held by `todie`. `vesperan` and `zerodays` are pending republish at ~20:56Z on 2026-10-11.

## Design

### Decisions

| ID | Decision | Source |
|---|---|---|
| D1 | Pin `kube` v4.2.0, `k8s-openapi` v0.28.0 (v1_32), `schemars` v1.2.2. | lane w10:pN, 2026-10-10 |
| D2 | Spec 0004 gains an allow entry for `CDLA-Permissive-2.0` in `deny.toml` due to `webpki-root-certs`. | lane w10:pN, 2026-10-10 |
| D3 | Pin `unsigned-k8s-core` and `unsigned-k8s-api` by path (`~/projects/unsigned/paas`) until publication, then by version recorded in spec 0004. | lane w10:pN, 2026-10-10 |
| D4 | The `k8s-e2e` job runs on the existing self-hosted runner group `ent-default` with the `ci-rust` image, which must gain `k3d` and `kubectl`. (Note spec 0037 F7: runner group organization may need re-homing post-transfer). | lane w10:pN, 2026-10-10 |
| D5 | Pin `k3d` and `kubectl` as external tools per 0034. | lane w10:pN, 2026-10-10 |
| D6 | Christian publishes the `unsigned-k8s-*` crates. | lane w10:pN, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0039-01]** (Ubiquitous): The workspace shall depend on `kube` (features: `runtime`, `derive`), `k8s-openapi` (feature: `v1_32`), and `schemars`.
- **[REQ-0039-02]** (Ubiquitous): The `deny.toml` shall explicitly allow the `CDLA-Permissive-2.0` license to permit the `webpki-root-certs` dependency, amending spec 0004.
- **[REQ-0039-03]** (Ubiquitous): The `unsigned-k8s-core` and `unsigned-k8s-api` crates shall be pinned by path into `paas` until publication, then by version recorded in spec 0004.
- **[REQ-0039-04]** (Ubiquitous): The CI pipeline shall include a `k8s-e2e` job on the `ent-default` runner group using the `ci-rust` image, which shall be provisioned with `k3d` and `kubectl`.
- **[REQ-0039-05]** (Ubiquitous): The Kubernetes components shall sit behind a `k8s` cargo feature flag, in compliance with spec 0033.
- **[REQ-0039-06]** (Ubiquitous): The `k3d` and `kubectl` tools shall be explicitly pinned as external tools according to spec 0034.
- **[REQ-0039-07]** (Ubiquitous): The `Christian` identity shall be the sole publisher of the `unsigned-k8s-core` and `unsigned-k8s-api` crates to their reserved `crates.io` namespaces.

## Plan

- [ ] Add `kube` v4.2.0, `k8s-openapi` v0.28.0, and `schemars` v1.2.2 to Cargo.toml.
- [ ] Amend Spec 0004 and update `deny.toml` to allow `CDLA-Permissive-2.0`.
- [ ] Add `k3d` and `kubectl` dependencies to the `ci-rust` runner image provisioning scripts.
- [ ] Implement the `k8s-e2e` CI job on `ent-default`.

## Test

| Requirement | Negative Control |
|---|---|
| REQ-0039-02 | `cargo deny check licenses` on the workspace with `webpki-root-certs` goes red if `CDLA-Permissive-2.0` is removed from `deny.toml`. |
| REQ-0039-04 | The `k8s-e2e` CI job fails if `k3d` executable is not found in the `ci-rust` image. |
| REQ-0039-06 | `make pin-check` on a fixture containing unpinned `k3d`. Goes red if the unrecorded crate fails the gate as specified by 0034. |

```bash
cargo build --features k8s
cargo deny check licenses
make pin-check
```

## Notes

- Note on `D4`: The `ent-default` runner group's organization could not be read per spec 0037 F7; this job may require re-homing after the transfer to `unsigned-gg`.
