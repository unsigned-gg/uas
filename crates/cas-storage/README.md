# cas-storage

Storage clients: rustfs (WORM S3), minikv (Raft metadata) and aistore (GPU streaming).

| | |
|---|---|
| Status | Placeholder. No code yet. |
| Roadmap phase | Phase 6 (Distributed Storage & Operator Client; spec 0021) |
| Research | `cerebral-work/research@4068134:SYNTHESIS-ESTATE-AND-SUBSTRATE.md section 5` |
| Will depend on | cas-domain |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Add the Cargo dependency when the first
real code needs it, not before.

## Scope and protocols

Spec `specs/0021-cas-storage-scope-and-phase.md` resolves the open questions:
- **Roadmap phase:** Phase 6 (Distributed Storage & Operator Client).
- **rustfs:** Object Lock (WORM) S3 client using `aws-sdk-s3` with BLAKE3 checksums.
- **aistore:** HTTP REST and S3 gateway client for line-rate GPU weight and dataset streaming. No Go runtime or CGO is used.
- **minikv:** Lightweight HTTP/REST client for Raft metadata consensus.
