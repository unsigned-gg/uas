# cas-deck

Operator dashboard: Axum routes and Maud templates. It shows the session table and the settle queue, read only.

| | |
|---|---|
| Status | Slice 2 (spec 0018 D7, D9, D10 and the Component 3 render side, A1). Read only: the Cortex body is parsed from a fixture, and no HTTP fetch, poller, action or binary exists yet. |
| Roadmap phase | Phase 5 |
| Research | `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md section 3.6` |
| Spec | `specs/0018-mad-stack-operator-deck-and-settle-queue.md` |
| Depends on | cas-domain, cas-engine |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Nothing depends on this crate.
