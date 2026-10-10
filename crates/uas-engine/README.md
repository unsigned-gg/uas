# uas-engine

Durable workflow engine: Autumn-Harvest workflow definitions, activities and saga compensators.

| | |
|---|---|
| Status | Placeholder. No code yet. |
| Roadmap phase | Phase 1 (with uas-domain) |
| Research | `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md section 3.2` |
| Will depend on | uas-domain, uas-pool |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Add the Cargo dependency when the first
real code needs it, not before.
