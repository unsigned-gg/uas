---
status: planned
created: 2026-10-10
tags: [storage, rustfs, minikv, aistore, phase-6, s3]
priority: medium
depends_on: [0001-cas-architecture-and-crate-layout, 0004-dependency-and-licence-policy]
---

# 0021-cas-storage-scope-and-phase

## Overview

Spec 0001 leaves two open gaps for `uas-storage`:
1. Gap 3: `uas-storage` has no phase in the research roadmap.
2. Gap 4: `aistore` is written in Go, so `uas-storage` can only be an HTTP client of it.

Spec 0001 Plan item 5 requires a spec that gives `uas-storage` a phase or merges
it into another crate. This spec resolves the scope, protocol boundaries,
layering, and roadmap phase for `uas-storage`.

Facts, measured 2026-10-10 on ceres:

- **F1 [verified]:** `specs/0001` lists `uas-storage` with phase "none (gap)" and
  records Gap 3 and Gap 4. Command:
  `grep -n "uas-storage" specs/0001-cas-architecture-and-crate-layout.md`.
- **F2 [verified]:** The research roadmap section 6 defines Phase 6 as
  "Distributed Storage & Operator: Deploy `ais-k8s` operator to reconcile AIStore
  clusters and bind local NVMe disks for GPU model acceleration." Source:
  `cerebral-work/research@4b3ed94:agentic-systems/designs/rust-native-agent-substrate.md`.
- **F3 [verified]:** Spec 0004 lines 149 and 158 pin `tokio` 1.53.1 and
  `aws-sdk-s3` 1.150.0 for `uas-storage` in phase 6 with features `sigv4a`,
  `http-1x`, `default-https-client`, `rt-tokio`, and `behavior-version-latest`.
  Command: `grep -n "uas-storage" specs/0004-dependency-and-licence-policy.md`.
- **F4 [verified]:** Spec 0004 line 168 rejects unpinned third-party minikv crates.
  Any minikv communication must use standard HTTP/REST or gRPC client libraries.
  Command: `grep -n "minikv" specs/0004-dependency-and-licence-policy.md`.
- **F5 [verified]:** Research synthesis sections 4.8, 4.9, 4.10, and table 5 define
  the three storage systems:
  1. `rustfs`: S3-compatible object store with Object Lock (WORM) and BLAKE3 verification.
  2. `minikv`: Raft metadata consensus store with linearizable ReadIndex.
  3. `aistore`: Distributed NVMe cache streaming model weights to GPU memory via AIS REST and S3 APIs.
- **F6 [verified]:** `crates/uas-storage` exists as a valid workspace placeholder
  with zero Cargo dependencies. Command: `cargo check -p uas-storage -j 2`.
- **F7 [verified]:** Layering rule (spec 0001): `uas-storage` sits in layer 2
  beside `uas-router`, above `uas-domain`, and below `uas-engine` and `uas-cli`.

## Design

### Requirements (EARS)

- **Ubiquitous:** `uas-storage` shall sit in layer 2 beside `uas-router`,
  depending only on `uas-domain` and workspace-pinned dependencies approved in spec 0004.
- **Ubiquitous:** `uas-storage` shall provide client interfaces for three backends:
  `RustfsClient` (S3 WORM), `MinikvClient` (Raft metadata consensus), and `AistoreClient` (GPU streaming).
- **Event-driven:** WHEN `uas-engine` persists an execution receipt or audit log to `rustfs`,
  `uas-storage` shall commit the payload using S3 Object Lock WORM retention with a BLAKE3 checksum.
- **Event-driven:** WHEN a worker slot requests model weights or LoRA adapters from `aistore`,
  `uas-storage` shall return pre-signed direct target streaming URLs to bypass gateway bottlenecks.
- **State-driven:** WHILE the project is in Phases 1 through 5, `crates/uas-storage` shall
  remain a placeholder with zero external dependencies.
- **Unwanted behavior:** IF a storage backend endpoint is unreachable or returns an HTTP 5xx error,
  THEN `uas-storage` shall return a typed `StorageError` without panicking or leaking credentials.

### Options

| Option | Architecture | For | Against |
|---|---|---|---|
| A. Dedicated crate in Phase 6 (recommended) | Keep `uas-storage` as layer 2 client crate assigned to Phase 6. | Preserves layer isolation. Prevents pulling S3 and HTTP client dependency trees into `uas-engine` (Phase 1). Aligns with Phase 6 `ais-k8s` milestone. | Adds one more crate to maintain in the workspace. |
| B. Merge into `uas-engine` and `uas-router` | Delete `uas-storage`. Put receipt storage into `uas-engine` and model streaming into `uas-router`. | One fewer crate in the workspace. | Violates layer separation. Bloats `uas-engine` compile times and pulls network I/O into workflow core. |
| C. Out-of-band external sidecars only | Delete `uas-storage`. Delegate all storage operations to Kubernetes CSI drivers and sidecars. | No Rust storage code in UAS. | UAS engine cannot programmatically verify WORM audit locks or generate dynamic pre-signed AIStore streaming URLs. |

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | **Roadmap phase:** `uas-storage` is assigned to **Phase 6** ("Distributed Storage & Operator Client"). Spec 0001 gap 3 and Plan item 5 close with this assignment. | UAS lane, 2026-10-10 |
| D2 | **aistore client protocol:** `AistoreClient` in `uas-storage` connects to AIStore over its HTTP REST API and S3 gateway. No Go runtime or CGO is used. Spec 0001 gap 4 closes with this record. | UAS lane, 2026-10-10 |
| D3 | **rustfs client protocol:** `RustfsClient` uses `aws-sdk-s3` (Apache-2.0, pinned in spec 0004) with BLAKE3 checksum validation and Object Lock headers. | UAS lane, 2026-10-10 |
| D4 | **minikv client protocol:** `MinikvClient` uses standard HTTP/REST via the workspace HTTP client (`reqwest` with `rustls`). No unvetted third-party Raft client crate is added. | UAS lane, 2026-10-10 |
| D5 | **Layering invariance:** `uas-storage` depends only on `uas-domain`. `uas-engine`, `uas-acp`, and `uas-cli` may depend on `uas-storage` starting in Phase 6. `uas-storage` never depends on `uas-engine`, `uas-pool`, or `uas-sandbox`. | UAS lane, 2026-10-10 |

### What does not change

- `uas-domain` remains pure and zero I/O.
- No new dependencies are added to `crates/uas-storage/Cargo.toml` until Phase 6 begins.
- `uas-engine` uses local SQLite/PostgreSQL WAL for Phase 1 durability without requiring distributed storage.

## Plan

- [x] Draft spec 0021 defining `uas-storage` scope, protocols, and Phase 6 assignment.
- [x] Update `specs/0001-cas-architecture-and-crate-layout.md` to record Phase 6 for `uas-storage` and close gaps 3 and 4.
- [x] Update `crates/uas-storage/README.md` to record Phase 6 assignment.
- [x] Add `specs/0021-cas-storage-scope-and-phase.md` to `specs/README.md`.
- [ ] Open PR into `development` and merge on green CI.

## Test

```bash
make spec-check
cargo check -p uas-storage -j 2
! grep -E 'uas-(engine|pool|sandbox|ebpf|acp|router|deck)' crates/uas-storage/Cargo.toml
```

The third command confirms that `uas-storage` does not depend on higher-layer crates.

## Notes

- AIStore Kubernetes operator (`ais-k8s`) deploys and reconciles clusters on Cygnus NVMe nodes; `uas-storage` is the client library that substrate worker sessions use to interact with those clusters.
- Reed-Solomon erasure coding and bitrot healing are handled inside `rustfs` server daemons, not in `uas-storage` client code.
