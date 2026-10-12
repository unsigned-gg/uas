# uas-acp

Agent Communication Protocol server (JSON-RPC 2.0 over stdio) for Zed and IDE integration.

| | |
|---|---|
| Status | initialize, session/new and session/cancel over stdio, against a pool port (spec 0016 A1). No turn runs yet. |
| Roadmap phase | Phase 3 (with uas-pool) |
| Research | `cerebral-work/research@4068134:agentic-systems/designs/substrate-diagrams-and-components.md section 3` |
| Depends on | uas-domain, uas-engine, uas-pool, agent-client-protocol 2.2.0 |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Add the Cargo dependency when the first
real code needs it, not before.
