# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Spec 0008 and service edge integration modules connecting CAS Substrate to estate pillars (CER-2676): `cas-domain` adds `PolicyTier` and `SomaClassification`; `cas-engine` adds `SomaAdmissionGate`, `ReverieClient`, `CortexMeshEvent`, `SettleCard` and `SettleQueue`, and `SagaFlowCoordinator` with backward rollback; `cas-cli` adds `--policy-tier` admission checking.
- Blackwall custody setup (CER-2671): `blackwall.toml` (claude provider, `make check` and `make hygiene` as the verify gate, PRs into `development`, automerge off), the `tasks/` library with the first task `harness-evidence`, the `custody-run` skill, `make custody-check`, and `.blackwall/` ignored.
- `cas-engine` workflow sessions, activities, and event log with `cas harness` CLI runner and `.saga/flows/cas-builder.toml` dogfood flow.
- Spec 0007, the lease rules: one live lease per slot, only the lease token renews a lease, a renewal extends by at least one second, and a draining slot keeps its worktree. `cas-domain` adds `check_slot_free`, `Lease::holds_slot_at`, `LeaseToken::matches`, and the errors `SlotLeased` and `TokenMismatch`.

### Changed
- `Lease::renew` and `LeaseLedger::renew` take the lease token as a new parameter. A renewal by zero seconds now fails. The in-memory ledger refuses a second live lease on a slot.
- `release-plz`: name GitHub releases `v{{ version }}` to match tag names instead of `cas-cli-v{{ version }}`.
- `ci`: drop the dead `release` trigger and attach job from `sbom.yml`, generating the SBOM artifact on push to `main` only.

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
