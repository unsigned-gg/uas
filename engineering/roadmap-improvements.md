# Roadmap improvements

The engineering-side roadmap in three horizons. Every item has a "what" and a "done when". Every item is proposed until the owner accepts it.

Source for the phases: the research roadmap in `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md` section 6. The research is a draft. The phase order is design intent, not a commitment.

Science and paid runs stay with their approver. They do not belong here.

## Now

1. **Phase 1, domain** (proposed). What: build `cas-domain` with slot, lease, envelope, attestation and error types. Done when: tests for illegal transitions, lease expiry and tier ordering pass, and `cargo tree -p cas-domain` shows no runtime or workspace crates.
2. **Phase 1, engine** (proposed). What: wire the Autumn-Harvest engine into `cas-engine` with SQLite and Postgres backends and activity retry loops. Done when: a workflow replays from its event log after a killed process with no repeated activity. Blocked on spec 0004 approving the dependency.
3. **Open gap, pool runtime** (proposed). What: a spec that chooses thread-per-core or Tokio for `cas-pool`. Done when: the spec is merged and names the evidence.
4. **Open gap, job cap** (proposed). What: a spec for a real build job cap, or a decision to drop the claim. Done when: the spec states a mechanism that limits job count, not only priority.

## Next

5. **Phase 2, sandbox and priority** (proposed). What: Landlock rulesets in `cas-sandbox` and aya tracepoints in `cas-ebpf`. Done when: a write outside the worktree fails with a permission error and a spawned build runs at nice 19. Needs the eBPF licence position in spec 0004 and a nightly toolchain with `bpf-linker`.
6. **Phase 3, pool and ACP** (proposed). What: `cas-pool` with cgroup v2 freeze and resume and gitoxide worktrees, plus the `cas-acp` endpoints. Done when: a slot suspends, resumes and rolls back a failed edit, and an IDE client connects to the server.
7. **Open gap, storage phase** (proposed). What: give `cas-storage` a phase, or merge it into another crate. Done when: the roadmap table and spec 0001 agree.

## Later

8. **Phase 4, routing** (proposed). What: `cas-router` with SHA-256 prefix hashing, affinity routing and dual-stream think-token splitting. Done when: repeated prefixes reach the same replica and the hit rate is measured.
9. **Phase 5, deck** (proposed). What: the operator dashboard in `cas-deck` that reads the cortex settle queue. Done when: an operator approves a pending card from the page.
10. **Phase 6, distributed storage** (proposed). What: deploy the aistore Kubernetes operator and bind local NVMe disks. Done when: the cluster reconciles. This is operator work, not a crate.
11. **Phase 7, identity and control plane** (proposed). What: the in-line control plane with signed messages, dual identity and A2A bridges. Done when: a spec scopes it.
