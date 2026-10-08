# cas-storage

Storage clients: rustfs (WORM S3), minikv (Raft metadata) and aistore (GPU streaming).

| | |
|---|---|
| Status | Placeholder. No code yet. |
| Roadmap phase | None in the research roadmap (open gap) |
| Research | `cerebral-work/research@4068134:SYNTHESIS-ESTATE-AND-SUBSTRATE.md section 5` |
| Will depend on | cas-domain |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Add the Cargo dependency when the first
real code needs it, not before.

## Open questions

- The research roadmap (DES section 6) gives this crate no phase.
- aistore is written in Go. This crate can only be a client of its HTTP API.
