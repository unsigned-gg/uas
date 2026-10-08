# cas-pool

Worker slot manager: warm worktree pool (gitoxide), cgroup v2 suspend and resume.

| | |
|---|---|
| Status | Placeholder. No code yet. |
| Roadmap phase | Phase 3 |
| Research | `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md §3.1` |
| Will depend on | cas-domain, cas-sandbox, cas-ebpf |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Add the Cargo dependency when the first
real code needs it, not before.
