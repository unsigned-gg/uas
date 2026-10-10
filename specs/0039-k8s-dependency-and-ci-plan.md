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

Status: planned, Created: 2026-10-10

## Requirements

[REQ-0039-01] The workspace shall depend on `kube` (features: `runtime`, `derive`), `k8s-openapi` (feature: `v1_32`), and `schemars`.
[REQ-0039-02] The `deny.toml` shall explicitly allow the `CDLA-Permissive-2.0` license to permit the `webpki-root-certs` dependency.
[REQ-0039-03] The `unsigned-k8s-core` and `unsigned-k8s-api` crates shall be pinned via git revision until published.
[REQ-0039-04] The CI pipeline shall include a new `k8s-e2e` job in `ci.yml` using `ubuntu-latest` that provisions `k3d` and `kubectl`.
[REQ-0039-05] The Kubernetes components shall sit behind a `k8s` cargo feature flag, in compliance with spec 0033.
[REQ-0039-06] The `k3d` and `kubectl` tools shall be explicitly pinned as external tools according to spec 0034.
[REQ-0039-07] The `Christian` identity shall be the sole publisher of the `unsigned-k8s-core` and `unsigned-k8s-api` crates to their reserved `crates.io` namespaces.

## Design

### Decisions

| Topic | Decision | Against |
|---|---|---|
| Crates | Pin `kube` v4.2.0, `k8s-openapi` v0.28.0 (v1_32), `schemars` v1.2.2. | Unpinned dependencies. |
| Licenses | Add `CDLA-Permissive-2.0` to `deny.toml` allowed list. | Block migration on `webpki-root-certs`. |
| Unpublished | Pin `unsigned-k8s-core` and `unsigned-k8s-api` via git revision. | Assume they exist on crates.io. |
| CI Job | Create a new `k8s-e2e` job in `ci.yml` on `ubuntu-latest`. | Add `k3d` to the `ci-rust` runner image. |
| Tool Pins | Pin `k3d` and `kubectl` as external tools per 0034. | Rely on unpinned runner tools. |
| Publishing | Christian publishes the `unsigned-k8s-*` crates. | Automated or lane publication. |
*Attributed to: w10:pN*

## Tasks

- [ ] Check candidate versions of the new dependencies.
- [ ] Evaluate the transitive license tree with `cargo deny`.
- [ ] Confirm `webpki-root-certs` license requires `CDLA-Permissive-2.0` exception.
- [ ] Formulate the CI matrix extension.

## Verification

The candidate dependencies must build successfully under the `1.94.1` toolchain with `rust-version = "1.89"`. We use a throwaway crate to verify the build and test the license tree.

```bash
cargo tree | grep -E "kube |k8s-openapi |schemars "
cargo deny check licenses
```

Negative controls:
- The `k8s-e2e` job fails explicitly if the `k3d` executable is not found.
- Running `cargo deny check licenses` goes red when `webpki-root-certs` is evaluated without a `CDLA-Permissive-2.0` exception.

Facts:
- Candidate versions are `kube` v4.2.0, `k8s-openapi` v0.28.0, and `schemars` v1.2.2.
  - Date: 2026-10-10
- The transitive dependency `webpki-root-certs` introduces a license (`CDLA-Permissive-2.0`) currently rejected by `deny.toml`.
  - Date: 2026-10-10

## Notes

No additional research notes.
