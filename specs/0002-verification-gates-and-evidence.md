---
status: in-progress
created: 2026-10-08
tags: [verification, gates, evidence, make]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout]
---

# 0002-verification-gates-and-evidence

## Overview

A claim of "done" needs proof. This spec names the gates that give proof. It
also names the evidence tiers, and it sets the `-j` rule.

Evidence from 2026-10-08:

- The `ci-rust` image bakes Rust 1.94.1. Its rustup directory is read-only to
  the runner user. CI cannot install a toolchain.
- The cortex CI `rust` job runs without `-j`. This is drift from the house rule.
- `nextest` is not installed in cortex or revenant CI. Their workflows use
  `|| cargo test`. That fallback hides failures.

## Design

### Gates

| Target | Runs | Use |
|---|---|---|
| `make check` | `fmt-check clippy test doc-test deny` | Before every push. House SOP. |
| `make hygiene` | `typos machete spec-check toolchain-check` | Before every push. |
| `make smoke` | Builds the CLI, runs `cas --version`, greps the domain crate for I/O | Before every PR. |
| `make ci` | `fmt-check clippy test doc-test` with `NEXTEST_PROFILE=ci` | What the CI `rust` job runs. |
| `make template-check` | `check`, `spec-check`, `smoke`, then the rename probe | Template repo only. |

The CI jobs `rust` and `hygiene` call these same targets. A gate that passes on
a workstation passes in CI.

### Evidence tiers

| Tier | Meaning | Example |
|---|---|---|
| verified | A command ran and the output proves the claim. | `make check` exit 0 |
| reported | Another agent or person said it. | "CI is green" in a handoff |
| heuristic | Inferred from a pattern. | "Same code, so it passes" |
| asserted | Stated with no source. | "It should work" |

Only `verified` satisfies a gate. A PR body lists the command and the result
for each claim. A grep that finds nothing is not proof of absence.

### The `-j` rule

Operator decision, the operator, 2026-10-08.

- Makefile sets `JOBS ?= 2`. Every cargo build, test and clippy call passes
  `-j $(JOBS)`.
- Nextest uses `--build-jobs $(JOBS)`. Its own `-j` flag sets test threads.
  Never use it for build jobs.
- Moon tasks and lefthook scripts use `-j 2` and `--build-jobs 2`.
- Shared workstations keep `JOBS=2`.

CI exception: CI calls make targets with `JOBS=10`. The `ci-rust` pod has a
CPU limit of 10 and runs one job at a time. The pod is ephemeral. No other
user shares it. The `_rust.yml` workflow also sets `CARGO_BUILD_JOBS` to the
same number.

## Plan

- [ ] Write the Makefile targets with exact names from the table above.
- [ ] Add `.config/nextest.toml` with profiles `default`, `ci`, `pre-push`.
- [ ] Add `toolchain-check` to `xtask`.
- [ ] Wire `rust` and `hygiene` CI jobs to the make targets.
- [ ] Prove each gate fails on a planted fault, then passes.

## Test

Run the full gate set with the workstation default.

```bash
make check && make hygiene && make smoke
```

Passing means all three exit 0. Then plant one fault per gate. A misformatted
file must fail `fmt-check`. A GPL crate must fail `deny`. A duplicate spec
number must fail `spec-check`. Remove each fault and confirm the gate passes
again.

## Notes

Nextest profile `ci` retries once and does not stop at the first failure.

Open question: add `cargo audit` and semver checks to `make check`. Today
`audit` runs weekly in CI only.

References:

- `cerebral-work/cortex@524a97b:Makefile`
- `codervisor/leanspec@ee122d6:skills/leanspec/SKILL.md`
