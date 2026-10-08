# cas-router

vLLM prefix-hash routing, FastCache affinity and load balancing for model calls.

| | |
|---|---|
| Status | Placeholder. No code yet. |
| Roadmap phase | Phase 4 |
| Research | `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md §3.4` |
| Will depend on | cas-domain |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Add the Cargo dependency when the first
real code needs it, not before.
