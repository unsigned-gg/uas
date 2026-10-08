# cas-acp

Agent Communication Protocol server (JSON-RPC 2.0 over stdio and TCP) for Zed and IDE integration.

| | |
|---|---|
| Status | Placeholder. No code yet. |
| Roadmap phase | Phase 3 (with cas-pool) |
| Research | `cerebral-work/research@4068134:agentic-systems/designs/substrate-diagrams-and-components.md section 3` |
| Will depend on | cas-domain, cas-engine |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Add the Cargo dependency when the first
real code needs it, not before.
