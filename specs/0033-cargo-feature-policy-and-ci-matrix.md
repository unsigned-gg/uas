---
status: planned
created: 2026-10-10
tags: [cargo, features, ci, gates, cas-engine]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0004-dependency-and-licence-policy, 0005-ci-cache-and-runners, 0026-cortex-pool-member-adapter]
---

# 0033-cargo-feature-policy-and-ci-matrix

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

Three specs now gate requirements behind a cargo feature, and no spec says how a
feature is named, defaulted, declared or tested. CI never builds with one on, so
every feature-gated requirement in the repo is untested by construction. A test
suite that cannot reach the code it names is the decorative control spec 0002
rejects, and the repo has that at the gate level rather than in one file.

Facts, measured 2026-10-10 on `origin/development` at `51c6dac`:

- **F1 [verified]:** Three feature names appear in `specs/`: `cortex-grpc` 14
  times, `delegation` 25, `crdt-collab` 3. Command:
  `grep -rhoE '(cortex-grpc|crdt-collab|delegation)' specs/*.md | sort | uniq -c`.
- **F2 [verified]:** One `[features]` section exists in the whole workspace,
  `crates/cas-engine/Cargo.toml` line 35, declaring `cortex-grpc` only. Command:
  `grep -rn '^\[features\]' crates/*/Cargo.toml Cargo.toml`.
- **F3 [verified]:** No CI workflow and no Makefile target passes `--features`
  or `--all-features`. Command:
  `grep -rnE 'features|--all-features' .github/workflows/*.yml Makefile`, which
  returns nothing. Positive control: `grep -rc 'cargo' .github/workflows/_rust.yml`
  returns 8, so the files and the pattern are both reachable.
- **F4 [verified]:** `make check` is `fmt-check clippy test doc-test deny` and
  `make ci` is `fmt-check clippy test doc-test`, both with default features only.
  Command: read `Makefile` lines 43 to 46.
- **F5 [verified]:** Specs 0004 and 0005 say nothing about cargo features.
  Command: `grep -nicE 'cargo feature|\[features\]|--features|feature flag'
  specs/0004*.md specs/0005*.md` returns 0 for both. Positive control: the same
  file matches `job` 27 times.

The consequence is narrow and concrete. Spec 0026's adapter, spec 0028's CRDT
adapter and spec 0029's delegation rules are all specified to live behind a
feature that is off by default. On the current gates, turning any of them on
could break the build and no check would say so before a merge.

## Design

This spec sets the policy. It adds no feature of its own.

### Decisions

| | Decision | Source |
|---|---|---|
| D1 | A feature is declared in the crate that owns the code, never in the workspace root, and it is listed in that crate's `[features]` with a comment naming the spec that introduced it. `cas-engine`'s `cortex-grpc` is the pattern. | CAS lane w25:p1, 2026-10-10; `crates/cas-engine/Cargo.toml` line 36 |
| D2 | A feature is off by default. The default build stays the shipped build, so `make check` keeps its present meaning. | spec 0026 D1; CAS lane w25:p1, 2026-10-10 |
| D3 | A feature name is kebab-case, names the capability rather than the crate, and is unique across the workspace, so one `--features` value cannot mean two things. | CAS lane w25:p1, 2026-10-10 |
| D4 | A spec that introduces a feature must name it, say which crate declares it, and say what the default build loses without it. Specs 0026, 0028 and 0029 are retrofitted by their own authors, not here. | CAS lane w25:p1, 2026-10-10 |
| D5 | The gate is a matrix, not `--all-features`. Each feature gets its own leg, because `--all-features` hides a pair that only breaks alone and says nothing about which one failed. A combined leg may be added later for pairs that are specified to work together. | CAS lane w25:p1, 2026-10-10 |
| D6 | The matrix runs `clippy` and `test` per feature, not the full `check`. `fmt` and `deny` are feature-independent, and `doc-test` is covered by the default leg. | CAS lane w25:p1, 2026-10-10 |
| D7 | A declared feature with no matrix leg fails `make hygiene`. This is the enforcement half: without it the policy erodes exactly as the three current features did. | CAS lane w25:p1, 2026-10-10 |
| D8 | `-j 2` applies to every matrix leg on a workstation, and CI keeps `JOBS=10`. The matrix multiplies build time, so the leg count is a cost the operator sees. | AGENTS.md rule 2; spec 0002 |

### Requirements (EARS)

- **[REQ-0033-01]** (Ubiquitous): The workspace shall declare every cargo feature in the `[features]` table of the crate that owns the gated code.
- **[REQ-0033-02]** (Ubiquitous): Every declared feature shall be off in the default build.
- **[REQ-0033-03]** (Ubiquitous): Every declared feature name shall be kebab-case and unique across the workspace.
- **[REQ-0033-04]** (Ubiquitous): Each `[features]` entry shall carry a comment naming the spec that introduced it.
- **[REQ-0033-05]** (Ubiquitous): The repo shall provide a `make features` target that runs `clippy` and `test` once per declared feature.
- **[REQ-0033-06]** (Event-driven): WHEN CI runs on a pull request, the feature matrix shall run one leg per declared feature.
- **[REQ-0033-07]** (Event-driven): WHEN a matrix leg fails, CI shall report which feature failed, and shall not mask it behind another leg's result.
- **[REQ-0033-08]** (State-driven): WHILE a workstation runs the matrix, every leg shall pass `-j 2`.
- **[REQ-0033-09]** (Unwanted): IF a crate declares a feature that no matrix leg covers, THEN `make hygiene` shall fail and name the feature.
- **[REQ-0033-10]** (Unwanted): IF a feature is declared outside the crate that owns its gated code, THEN `make hygiene` shall fail and name the crate.
- **[REQ-0033-11]** (Unwanted): IF a declared feature is on in the default build, THEN `make hygiene` shall fail and name the feature.
- **[REQ-0033-12]** (Optional): WHERE two features are specified to work together, the matrix shall add one leg enabling both, in addition to their single legs.

## Plan

- [ ] Add an xtask check, `feature-check`, that reads every `crates/*/Cargo.toml`, collects the declared features, and enforces REQ-09, REQ-10 and REQ-11.
- [ ] Add `feature-check` to `make hygiene`, beside `pin-check`.
- [ ] Write the red-first tests for the xtask: a fixture with an uncovered feature, one declared in the wrong crate, and one on by default. Watch each fail before the check exists.
- [ ] Add the `make features` target, one `clippy` and one `test` leg per declared feature, honouring `JOBS`.
- [ ] Add the matrix leg to `.github/workflows/ci.yml`, deriving the feature list rather than hard-coding it, so a new feature cannot be forgotten.
- [ ] Record the current three features and their owning crates, and open a follow-up for the two that specs 0028 and 0029 assume but no crate declares.

## Test

```bash
cargo test -p xtask -j 2 feature_check && \
  make features JOBS=2 && \
  make hygiene JOBS=2
```

A pass requires each of these cases to exist and to go red when its own check is
deleted:

| Case | Where | Requirement |
|---|---|---|
| a declared feature with no matrix leg fails hygiene | xtask fixture | REQ-0033-09 |
| a feature declared in the wrong crate fails hygiene | xtask fixture | REQ-0033-10 |
| a feature on by default fails hygiene | xtask fixture | REQ-0033-11 |
| a feature name in CamelCase fails hygiene | xtask fixture | REQ-0033-03 |
| `make features` fails when a feature-gated crate does not compile with its feature on | workspace | REQ-0033-05 |

The control that matters most: with the matrix removed, `make check` still passes
on a workspace whose feature-gated code does not compile. That is today's state,
and it is what this spec exists to change.

## Notes

- This spec records no new dependency, so spec 0004 is unchanged. `xtask` depends
  on `anyhow` alone and `pin-check` reads manifests as text rather than parsing
  TOML [verified: read `xtask/Cargo.toml` and `xtask/src/pin_check.rs`, which
  calls `read(&dir.join("Cargo.toml"))` and its own `parse_pins`]. `feature-check`
  follows that pattern. If a future author wants a real TOML parser, that is a new
  pin and spec 0004 owns it.
- Specs 0026, 0028 and 0029 keep their features. This spec does not rename them,
  and D4 leaves the retrofit to their authors.
- `--all-features` was rejected in D5 for a measured reason in kind, not in this
  repo: a combined build can pass while a single feature fails to compile alone,
  and the failure then appears only when someone enables that one feature.
- Spec 0005 owns the CI jobs. This spec adds one job to it and does not restate
  the runner or cache rules.
