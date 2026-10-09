# Vendored Saga Contracts and Flows

Do not edit these files directly. Re-vendor them from the upstream repository.

- Source repository: `unsigned-gg/saga`
- Upstream paths: `.saga/flows/` and `crates/saga-domain/src/`
- Upstream commit: `1e4be866f9aba22bb4d374b8d32db8f450918939`
- Date vendored: 2026-10-08
- Licence: Business Source License 1.1 (unsigned-gg)

## Role in CAS

These files provide reference flow definitions and domain types for the Saga orchestration engine.
CAS uses these templates and types to execute multi-step workflows, handle compensations, and verify DAG execution.

## Contents

- `flows/`: 9 TOML workflow templates (`build-and-gate.toml`, `linear-raid.toml`, `review.toml`, etc.).
- `contracts/`: 13 Rust domain contract sources (`flow.rs`, `journal.rs`, `evidence.rs`, `apply.rs`, etc.).
