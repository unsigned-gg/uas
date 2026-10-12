---
status: planned
created: 2026-10-10
tags: [pinning, ci, security, compliance]
priority: high
depends_on: [0004-dependency-and-licence-policy, 0005-ci-cache-and-runners]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0034-tool-pinning-beyond-cargo

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

AGENTS.md requires all external dependencies to be pinned before use. Currently, non-crate tools like `gitleaks` and `cargo-cyclonedx` slip past `make pin-check` because the check only inspects Cargo lockfiles. This spec defines how external tools outside of Cargo are pinned, recorded, and checked. This specification amends Spec 0004.

### Facts
- **F1** [verified 2026-10-10]: `gitleaks` is used in CI but unpinned in Spec 0004. Command: `grep -in "GITLEAKS" .github/workflows/ci.yml`.
- **F2** [verified 2026-10-10]: `cargo-cyclonedx` is used in the Makefile but unpinned. Command: `grep -in "cyclonedx" Makefile`.
- **F3** [verified 2026-10-10]: The `xtask` pin-check does not scan CI tools. Command: `! grep -nE 'gitleaks|workflow|\.github' xtask/src/pin_check.rs`.
- **F4** [verified 2026-10-11]: `install-action` floats `tool:` without a version. Command: `grep -nE 'tool: [a-z-]+$' .github/workflows/*.yml`.

## Design

### Decisions

| ID | Decision | Source |
|---|---|---|
| D1 | Pinning rules extend to all external tools: GitHub Actions (per Spec 0005), CI-installed CLIs (by version/hash), Makefile-invoked Cargo subcommands (by version), and toolchains (by version or channel date). | lane w10:pN, 2026-10-10 |
| D2 | The canonical record for these pins lives in Spec 0004 (amended). | lane w10:pN, 2026-10-10 |
| D3 | The `make pin-check` (xtask) gate is expanded to parse YAML workflows and Makefiles against the Spec 0004 markdown tables. | lane w10:pN, 2026-10-10 |
| D4 | A CI tools row is keyed by the first backticked token of its first cell. | proposed |
| D5 | Actions are keyed by `owner/repo`; a `uses: owner/repo/path@ref` maps to `owner/repo`; local `./` uses are skipped; Pin cell holds the 40-hex SHA. | proposed |
| D6 | A `with: tool:` entry is `name@X.Y.Z`; bare names or `@latest` are misses; `moon-version:` maps to `moon`; `*_VERSION` maps to the row whose Pin cell carries `env NAME`. | proposed |
| D7 | `cargo <sub>` outside built-ins needs a row keyed `cargo-<sub>` with an `X.Y.Z` version; `cargo install` needs `--rev`, `--version` or `@X.Y.Z`. | proposed |
| D8 | Toolchain channels in `rust-toolchain.toml`, `crates/uas-ebpf/ebpf/rust-toolchain.toml`, `EBPF_TOOLCHAIN` and `rustup toolchain install` must be `X.Y.Z` or `nightly-YYYY-MM-DD` and appear as a toolchain row. | proposed |
| D9 | Baked tools (`cargo-deny`, `typos`) are recorded in spec 0004 and the Makefile check requires the row to exist. | proposed |
| D10 | Success output prints `pin-check: ok (M tool references checked)`; misses print on stderr and exit code is 1. | proposed |

The record belongs in Spec 0004 because it is the established dependency and licence policy registry. A centralized registry prevents policy fragmentation.

### Requirements (EARS)

- **[REQ-0034-01]** (Ubiquitous): The canonical record for external tool pins shall live in Spec 0004's CI tools table.
- **[REQ-0034-02]** (Ubiquitous): The substrate shall pin all GitHub Actions by their exact commit SHA, per Spec 0005 ### Pinning.
- **[REQ-0034-03]** (Ubiquitous): The substrate shall pin all CI-installed CLIs by version or cryptographic hash.
- **[REQ-0034-04]** (Ubiquitous): The substrate shall pin all Makefile-invoked Cargo subcommands by version.
- **[REQ-0034-05]** (Ubiquitous): The substrate shall pin all rustup toolchains by version (e.g., 1.94.1) or channel date.
- **[REQ-0034-06]** (Event-driven): WHEN `make pin-check` runs, it shall parse `.github/workflows/*.yml` and `Makefile` to verify that every GitHub Action, CI CLI, Cargo subcommand, and toolchain matches the pinned version in Spec 0004.
- **[REQ-0034-07]** (Unwanted): IF a GitHub Action, CI CLI, Cargo subcommand, or toolchain is found in a workflow or Makefile that is missing from Spec 0004, THEN `make pin-check` shall exit non-zero.

## Plan

- [ ] Amend Spec 0004 to include `gitleaks` and `cargo-cyclonedx` in the CI tools table, alongside the existing `gh` and `bpf-linker` entries. Spec 0055 build plan P1 wrote the `gitleaks` row and, as it was absent, the `cargo-cyclonedx` row (landed, PR #204).
- [ ] Add a `Tool` table parser to `xtask/src/pin_check.rs` to read the Spec 0004 CI tools table.
- [ ] Add a fixture entry point to `xtask` so tests can run against mock Makefiles and workflows.
- [ ] Update `xtask/src/pin_check.rs` to extract `uses:` fields from YAML, `env:` version variables from YAML, and CLI invocations from `Makefile`.

## Test

| Requirement | Negative Control |
|---|---|
| REQ-0034-01 | Remove a tool row from spec 0004. `pin-check` fails naming the missing row. |
| REQ-0034-02 | Modify a GitHub Action SHA by one hex digit. `pin-check` fails naming the action. |
| REQ-0034-03 | Strip `@version` from a `with: tool:` line. `pin-check` fails naming the unversioned tool. |
| REQ-0034-04 | Invoke an unpinned Cargo subcommand in Makefile. `pin-check` fails naming the subcommand. |
| REQ-0034-05 | Set a toolchain channel to `stable`. `pin-check` fails naming the invalid channel. |
| REQ-0034-06 | `make pin-check` on a fixture Makefile with an unpinned `cargo-fake`. Goes red if the Makefile parser is removed. |
| REQ-0034-07 | `make pin-check` returning non-zero. Goes red if the check incorrectly succeeds on an unpinned tool. |

```bash
make pin-check
```
A test in `xtask/tests/pin_check_test.rs` runs `cargo xtask pin-check --root <fixture>`; the fixture is built in a temp dir; every failing case asserts stderr names the tool and has a passing control.

## Notes

- Defect: `gitleaks` is pinned via `GITLEAKS_VERSION` in `ci.yml` but unrecorded in Spec 0004.
- Defect: `cargo-cyclonedx` is used in `Makefile` but unrecorded in Spec 0004.
