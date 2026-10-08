# cas-deck

Operator dashboard (Axum, Maud, htmx, Tailwind) with cortex settle integration.

| | |
|---|---|
| Status | Placeholder. No code yet. |
| Roadmap phase | Phase 5 |
| Research | `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md §3.6` |
| Will depend on | cas-domain, cas-acp |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Add the Cargo dependency when the first
real code needs it, not before.
