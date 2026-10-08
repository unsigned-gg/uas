# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- The repository, generated from `cerebral-work/rust-monorepo-template` (template `main` at `1366ed9`) and renamed with `make new-repo NAME=cas`: the crates `cas-domain` and `cas-cli`, the `xtask`, the gate targets, CI, moon, lefthook, the specs and the agent files.
- Eight placeholder crates from the research design: `cas-engine`, `cas-pool`, `cas-sandbox`, `cas-ebpf`, `cas-router`, `cas-acp`, `cas-storage` and `cas-deck`. Each compiles and has a README that names its research section, phase and open questions.
- Spec 0001 `cas-architecture-and-crate-layout` replaces the template architecture spec. It pins the nine research crates plus `cas-cli`, the downward dependency rule, the invariants, the failure matrix, the integration edges and the open gaps. Spec 0004 lists the evaluated dependencies and the two copyleft exclusions. README, AGENTS.md, the docs index and `engineering/` describe CAS.
- `cas-domain`: the slot state machine, leases with a `LeaseLedger` port, evidence tiers and envelopes, plus a `cas demo` that walks a slot through its lifecycle.

### Changed
- The promotion PR from `development` to `main` now merges on green CI, by operator ruling on 2026-10-08. AGENTS.md, CONTRIBUTING.md, the release skill and spec 0003 say so. Spec 0003 also names the real required check `rust / cargo`.

### Fixed
- The `release` workflow installs a pinned `gh` 2.101.0 before release-plz. Before this, every push to `main` failed with `gh: command not found`, and no tag or release was made. The release-plz step now runs only on `main`.
