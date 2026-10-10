---
status: planned
created: 2026-10-10
tags: [pinning, ci, security, compliance]
priority: high
depends_on: [0004-dependency-and-licence-policy, 0005-ci-cache-and-runners]
---

# 0034-tool-pinning-beyond-cargo

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

AGENTS.md requires all external dependencies to be pinned before use. Currently, non-crate tools like `gitleaks` [verified, `grep -in "GITLEAKS" .github/workflows/ci.yml`] and `cargo-cyclonedx` [verified, `grep -in "cyclonedx" Makefile`] slip past `make pin-check` because the check only inspects Cargo lockfiles [verified, `grep -nE 'gitleaks|workflow|\.github' xtask/src/pin_check.rs` exits 1]. This spec defines how external tools outside of Cargo are pinned, recorded, and checked.

## Design

### Decisions

| ID | Decision | Source |
|---|---|---|
| D1 | Pinning rules extend to all external tools: GitHub Actions (by SHA), CI-installed CLIs (by version/hash), Makefile-invoked Cargo subcommands (by version), and toolchains (by channel date). | lane w10:pN, 2026-10-10 |
| D2 | The canonical record for these pins lives in Spec 0004 (amended). | lane w10:pN, 2026-10-10 |
| D3 | The `make pin-check` (xtask) gate is expanded to parse YAML workflows and Makefiles against the Spec 0004 markdown tables. | lane w10:pN, 2026-10-10 |

The record belongs in Spec 0004 because it is the established dependency and licence policy registry. A centralized registry prevents policy fragmentation.

### Requirements (EARS)

- **[REQ-0034-01]** (Ubiquitous): This specification amends Spec 0004 (Dependency and Licence Policy).
- **[REQ-0034-02]** (Ubiquitous): The canonical record for external tool pins shall live in Spec 0004's CI tools table.
- **[REQ-0034-03]** (Ubiquitous): The substrate shall pin all GitHub Actions by their exact commit SHA.
- **[REQ-0034-04]** (Ubiquitous): The substrate shall pin all CI-installed CLIs by version or cryptographic hash.
- **[REQ-0034-05]** (Ubiquitous): The substrate shall pin all Makefile-invoked Cargo subcommands by version.
- **[REQ-0034-06]** (Ubiquitous): The substrate shall pin all rustup toolchains by channel date.
- **[REQ-0034-07]** (Event-driven): WHEN `make pin-check` runs, it shall parse `.github/workflows/*.yml` and `Makefile` to verify that every external tool invocation matches the pinned version in Spec 0004.
- **[REQ-0034-08]** (Unwanted): IF a tool is found in a workflow or Makefile that is missing from Spec 0004, THEN `make pin-check` shall exit non-zero.

## Plan

- [ ] Amend Spec 0004 to include `gitleaks` and `cargo-cyclonedx` in the CI tools table, alongside the existing `gh` and `bpf-linker` entries.
- [ ] Update `xtask/src/pin_check.rs` to extract `uses:` fields from YAML, `env:` version variables from YAML, and CLI invocations from `Makefile`.

## Test

- **Verified claim:** `grep -nE 'gitleaks|workflow|\.github' xtask/src/pin_check.rs` returns nothing (exit 1), proving the current xtask pin-check does not scan CI tools.
- **Test case:**
```bash
make pin-check
```
Add a fixture `Makefile` calling an unpinned `cargo-fake`. A test in `xtask/tests/pin_check_test.rs` asserts that `make pin-check` fails. If the Makefile parsing logic in `pin_check.rs` is deleted, this test goes red.

## Notes

- Defect: `gitleaks` is pinned via `GITLEAKS_VERSION` in `ci.yml` but unrecorded in Spec 0004.
- Defect: `cargo-cyclonedx` is used in `Makefile` but unrecorded in Spec 0004.
