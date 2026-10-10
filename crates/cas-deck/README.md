# cas-deck

Operator dashboard: Axum routes and Maud templates. It shows the session table and the settle queue, read only.

| | |
|---|---|
| Status | Slice 1 (spec 0018 D1 partial, D5, Invariant 1, read side of the settle failure path). No actions, no events, no binary yet. |
| Roadmap phase | Phase 5 |
| Research | `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md section 3.6` |
| Spec | `specs/0018-mad-stack-operator-deck-and-settle-queue.md` |
| Depends on | cas-domain, cas-engine |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Nothing depends on this crate.
