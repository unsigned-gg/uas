# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Specs 0014 through 0018: LeanSpecs defining eBPF priority clamping and audit telemetry (`cas-ebpf`), slot pool with cgroup freezer and gitoxide worktrees (`cas-pool`), Zed Agent Communication Protocol server (`cas-acp`), FastCache prefix hashing and affinity routing (`cas-router`), and MAD stack operator deck with settle queue (`cas-deck`). Kernel-side eBPF bytecode subcrate and committed object staged at `crates/cas-ebpf/ebpf/`.
- Spec 0013 and the gateway socket proxy (CER-2718): `cas-sandbox::ipc` owns `<workspace>/.ipc/llm.sock`; `cas-router::uds_proxy` serves it on tokio, allows only the gateway paths, replaces the worker's `Authorization` with the key from a file the worker cannot read, and forwards over plain HTTP to the tailnet or in-cluster gateway. The escape suite measures that a jailed worker can connect to a pathname socket outside its workspace, so the allowlist is the boundary.
- Spec 0012 and the `cas-sandbox` escape suite (CER-2684, CER-2721): one jailed worker runs the attack matrix (writes and links out of the workspace, execute from `/tmp`, `no_new_privs` and Landlock across exec, setuid `sudo`, TCP, UDP and raw-socket egress) and the gate asserts every row.
- Spec 0011 and `cas-sandbox::netns` (CER-2717): `isolate()` installs a `pre_exec` hook that gives the worker a private network namespace with only `lo` up, through `unshare(CLONE_NEWNET)` or, unprivileged, a user namespace with an identity uid/gid map; `rustix` 1.1.5 enters the workspace.
- `cas-sandbox` launcher (CER-2716): `jail()` installs a `pre_exec` hook that enters the Landlock ruleset with `PR_SET_NO_NEW_PRIVS` before `exec`, `launch()` spawns a worker inside it, and a child that cannot enter the jail never runs (`SandboxError::JailRefused`).
- `cas-engine`: `MeshIntent` and optional `intent`/`hop_count` fields on `CortexMeshEvent` mirror the cortex mesh envelope contract (CER-2737). `CortexMeshEvent::increment_hop` enforces the bounded-recursion rule: at most 10 agent-to-agent hops, then an error demanding human escalation. The JSON wire form matches `cortex-domain` `MessageIntent` (snake_case, absent distinct from defaulted).
- Spec 0010 and the first `cas-sandbox` code (CER-2715): `probe()` reports the kernel Landlock ABI, `LandlockRulesetBuilder` canonicalises paths and builds the default-deny worker ruleset, and the `landlock` 0.4.7 dependency enters the workspace under the spec 0004 ruling of 2026-10-09.
- Record the CER-2671 custody rollout artifacts: the `spec-0001-deltadb-custody` task (the run that landed in PR #21) and the `estate-custody-rollout` saga flow (terrain assert, custody verify, jailed run, settle, reconcile gate, PR landing on green).
- Saga compensation runner, replay durability, and vendored contracts (CER-2677, CER-2678, CER-2679): `cas-engine` adds backward saga compensators (`WorktreeCleanupCompensator`, `BranchPruneCompensator`, `LeaseReturnCompensator`, `CompositeCompensator`), event log replay, and flow execution; `cas-cli` adds headless flow runner `cas harness run --flow <path>`; vendored domain schemas, protobufs, and flow templates from Soma, Cortex, and Saga under `vendor/`.
- Spec 0008 and service edge integration modules connecting CAS Substrate to estate pillars (CER-2676): `cas-domain` adds `PolicyTier` and `SomaClassification`; `cas-engine` adds `SomaAdmissionGate`, `ReverieClient`, `CortexMeshEvent`, `SettleCard` and `SettleQueue`, and `SagaFlowCoordinator` with backward rollback; `cas-cli` adds `--policy-tier` admission checking.
- Blackwall custody setup (CER-2671): `blackwall.toml` (claude provider, `make check` and `make hygiene` as the verify gate, PRs into `development`, automerge off), the `tasks/` library with the first task `harness-evidence`, the `custody-run` skill, `make custody-check`, and `.blackwall/` ignored.
- `cas-engine` workflow sessions, activities, and event log with `cas harness` CLI runner and `.saga/flows/cas-builder.toml` dogfood flow.
- Spec 0007, the lease rules: one live lease per slot, only the lease token renews a lease, a renewal extends by at least one second, and a draining slot keeps its worktree. `cas-domain` adds `check_slot_free`, `Lease::holds_slot_at`, `LeaseToken::matches`, and the errors `SlotLeased` and `TokenMismatch`.

### Changed
- Adopt DeltaDB and Blackwall custody process with PR-floor constraints in spec 0001 (CER-2671).
- `Lease::renew` and `LeaseLedger::renew` take the lease token as a new parameter. A renewal by zero seconds now fails. The in-memory ledger refuses a second live lease on a slot.
- `release-plz`: name GitHub releases `v{{ version }}` to match tag names instead of `cas-cli-v{{ version }}`.
- `ci`: drop the dead `release` trigger and attach job from `sbom.yml`, generating the SBOM artifact on push to `main` only.
- `cas-sandbox`, `cas-pool` and `cas-ebpf` deny `unsafe` instead of forbidding it. A block opts in with `#[allow(unsafe_code)]` and a `// SAFETY:` comment, which clippy enforces. Spec 0001 and spec 0004 record this ruling, the BPF program licence (`Dual MIT/GPL`, program object only) and the sudo `make ebpf-test` decision.

### Fixed
- `cas-cli`: `cas harness run` makes `--evidence` optional, defaults dry runs to heuristic evidence, and rejects claiming verified or reported evidence when simulating execution (CER-2671).
- `cas-domain`: deserializing a `Slot`, `Lease`, `Budget` or `Envelope` now runs the same checks as the constructors. Before, JSON could build a `Leased` slot with no worktree, a lease that ends before it begins, a zero budget, or an envelope with a bad id.

## [0.1.0] - 2026-10-08

### Added
- The repository, generated from `cerebral-work/rust-monorepo-template` (template `main` at `1366ed9`) and renamed with `make new-repo NAME=cas`: the crates `cas-domain` and `cas-cli`, the `xtask`, the gate targets, CI, moon, lefthook, the specs and the agent files.
- Eight placeholder crates from the research design: `cas-engine`, `cas-pool`, `cas-sandbox`, `cas-ebpf`, `cas-router`, `cas-acp`, `cas-storage` and `cas-deck`. Each compiles and has a README that names its research section, phase and open questions.
- Spec 0001 `cas-architecture-and-crate-layout` replaces the template architecture spec. It pins the nine research crates plus `cas-cli`, the downward dependency rule, the invariants, the failure matrix, the integration edges and the open gaps. Spec 0004 lists the evaluated dependencies and the two copyleft exclusions. README, AGENTS.md, the docs index and `engineering/` describe CAS.
- `cas-domain`: the slot state machine, leases with a `LeaseLedger` port, evidence tiers and envelopes, plus a `cas demo` that walks a slot through its lifecycle.

### Changed
- The promotion PR from `development` to `main` now merges on green CI, by operator ruling on 2026-10-08. AGENTS.md, CONTRIBUTING.md, the release skill and spec 0003 say so. Spec 0003 also names the real required check `rust / cargo`.

### Fixed
- The `release` workflow installs a pinned `gh` 2.101.0 before release-plz. Before this, every push to `main` failed with `gh: command not found`, and no tag or release was made. The release-plz step now runs only on `main`.
- The `release` job gets `pull-requests: read`. release-plz lists the pull requests of the released commit, and without this scope the call failed with 403 on `main`.
