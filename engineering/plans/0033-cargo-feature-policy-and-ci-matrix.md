<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plan: 0033-cargo-feature-policy-and-ci-matrix

Spec: `specs/0033-cargo-feature-policy-and-ci-matrix.md` (status planned, 12 requirements, 8 decisions, 6 Test rows). Base: `origin/development` `ec47b1f`, the merge of PR #198 that carries spec 0054.
Planner: `uas-planner`, 2026-10-11. Every path below was read in a worktree cut from that base.
Tiers: `verified` = I ran or read it at the path or command named; `reported` = the spec or a release API says it; `asserted` = my judgement.

## Findings the builders must know

1. **The `cortex-grpc` leg cannot build in CI today [verified]:** `cargo check -p uas-engine --features cortex-grpc --locked -j 2` in the worktree exits 101 with `.contracts/cortex/proto/pool.proto is missing: run cargo xtask fetch-contracts` (`crates/uas-engine/build.rs` line 9 to 24). That is a missing fetch, not a compile error. `fetch-contracts` reads the private repo `cerebral-work/cortex` (`contracts.lock`). Spec 0048 Plan line 152 says the CI credential for it is an operator-gated secret and "until it exists, CI builds default features only" [reported]. So REQ-0033-06 has no runnable CI leg until that secret exists. P4 is BLOCKED on it.
2. **A workstation can fetch [verified]:** `git ls-remote git@github.com:cerebral-work/cortex HEAD` prints a sha and exits 0 here. I did not run `fetch-contracts` and `.contracts/` is absent in the worktree. `make features JOBS=2` from the spec Test block needs `make fetch-contracts` first. P3 documents that and the Makefile does not fetch by itself.
3. **The spec contradicts itself on the CI list [verified: spec Plan bullet 5 against REQ-0033-09]:** Plan bullet 5 says to derive the CI feature list, and REQ-0033-09 compares against legs "as written, not a list derived from the manifests". A derived CI list makes REQ-09's CI half unable to fail. This plan follows the requirement: the Makefile and `ci.yml` each hold the legs as text, and `feature-check` compares both against the manifests. P1 amends bullet 5.
4. **Only one feature is declared [verified]:** `grep -rn '^\[features\]' crates/*/Cargo.toml Cargo.toml xtask/Cargo.toml` prints `crates/uas-engine/Cargo.toml:35` only. Plan bullet 6 says "the current three". `crdt-collab` (spec 0028) and `delegation` (spec 0029) are named in specs and declared nowhere. Filing the follow-up for those two is a tracker item, so it is the integrator's, not a builder's (spec 0057 F3). P1 corrects the wording.
5. **Spec 0054 lands today and binds the new CI job [verified: `specs/0054-ci-pipeline-policy.md` on `ec47b1f`]:** D3 and REQ-04 want the same-repo `if` on every job except `fork-gate`. D8 and REQ-13 want group `ent-default` with exactly one of `self-hosted`, `ci-rust`, `ci-kernel`. D9 wants `timeout-minutes`. D11 and REQ-14 forbid `${{` in a `run:` line except `inputs.*`, so a matrix value reaches the shell through `env:`. Its Test block, run from the repo root, flags any breach.
6. **A new job is non-required and needs no per-item go [reported: 0054 D7, 0057 D7]:** D7 says a job lands non-required and becomes required after seven days with no false failure, by a protection edit in one landing. P4 adds a job and edits no protection. Making `features` required is a later step that needs the operator's per-item go (0057 D7, route 3). It is not a package here.
7. **`xtask` parses manifests as text, and `pin_check.rs` is shared [verified: `xtask/Cargo.toml` holds `anyhow` and `sha2`; `xtask/src/pin_check.rs`]:** `feature_check` follows the same text approach and adds no dependency. It must not edit `pin_check.rs`: plan 0055 P2 and P5 own that file in the same period. `pin_check::member_dirs` is private, so `feature_check` lists `crates/*` with `std::fs::read_dir` itself.
8. **The default build must not change [reported: spec D2]:** `make check` and `make ci` keep their present meaning. `features` is not a prerequisite of either.
9. **Whether `cortex-grpc` is green today is unknown [asserted: not run].** I stopped at the missing contracts. If P3's first run is red on existing code, that is the spec's premise and a finding: the builder stops, reports the compiler or test output, and the integrator adds a fix package. P3 does not edit `crates/`.

## Plan items to packages

| Spec Plan item | Package |
|---|---|
| xtask check `feature-check` for REQ-09, REQ-10, REQ-11 | P2 |
| `feature-check` into `make hygiene` beside `pin-check` | P5 |
| Red-first fixtures for the xtask | P2 |
| `make features` target, one `clippy` and one `test` leg per feature, honouring `JOBS` | P3 |
| Matrix leg in `ci.yml`, derived rather than hard-coded | P4, as hard-coded legs (finding 3); P1 amends the bullet |
| Record the current features and open a follow-up for the two undeclared | P1 records it in the spec; the follow-up ticket is the integrator's (finding 4) |
| Status move (plans README) | P6 |

## Requirement coverage

| REQ | Package | How it closes |
|---|---|---|
| REQ-0033-01 | none | "The crate that owns the code" is not readable from a manifest. It holds today for `cortex-grpc` in `uas-engine` [verified: finding 4]. P2 enforces the part it can: a feature passed to a crate that lacks it (REQ-10). |
| REQ-0033-02 | P2 | The default-list check (REQ-11) is the mechanical form of "off by default". |
| REQ-0033-03 | P2 | Kebab-case is checked. "One name, one capability" is a review matter and no package checks it. |
| REQ-0033-04 | P1, P2 | P1 adds the missing Test row; P2 checks that a comment line naming `spec NNNN` precedes each `[features]` entry. |
| REQ-0033-05 | P3 | `make features`. |
| REQ-0033-06 | P4 | BLOCKED on the CI credential (finding 1). |
| REQ-0033-07 | P3, P4 | P3's loop runs every leg and names each failed leg. P4's matrix has `fail-fast: false` and the feature in the job name. P4 half BLOCKED. |
| REQ-0033-08 | P3 | `JOBS ?= 2` reaches every cargo call of a leg. |
| REQ-0033-09 | P2, P5 | P2 compares manifests to the Makefile legs and the `ci.yml` legs. P5 wires it into hygiene. The CI half is BLOCKED with P4. |
| REQ-0033-10 | P2 | Both forms: a dependency entry `features = [...]` and a `"crate/feature"` string in a `[features]` list. |
| REQ-0033-11 | P2 | `default = [...]` check. |
| REQ-0033-12 | none | Optional, and no pair of features is specified to work together [verified: only `cortex-grpc` is declared]. |

Closed by an unblocked package: 02, 03, 04, 05, 08, 10, 11 (7 of 12). Closed except a CI half that waits on P4: 07, 09. Blocked: 06. Closed by no package: 01, 12.

## Waves

| Wave | Packages | Files | Disjoint because | Done when |
|---|---|---|---|---|
| First | P1, P3 | P1: `specs/0033-cargo-feature-policy-and-ci-matrix.md`. P3: `Makefile`, `AGENTS.md` | One spec file, against a Makefile and the commands table. | The spec Test table has a REQ-04 row and the Plan matches this plan. `make features` runs one `clippy` and one `test` per leg and names a failed leg. |
| Second | P2, P4 | P2: `xtask/src/feature_check.rs` (new), `xtask/src/feature_check_tests.rs` (new), `xtask/src/main.rs`. P4: `.github/workflows/ci.yml` | One crate directory, against one workflow file. | `cargo test -p xtask -j 2 feature_check` exits 0. P4 is BLOCKED and does not run until the integrator rules (Open 1). |
| Third | P5 | `Makefile` | Alone: it waits on P2 and P4. | `make hygiene JOBS=2` exits 0 with `feature-check` inside it. |
| Fourth | P6 | `specs/0033-cargo-feature-policy-and-ci-matrix.md`, `specs/README.md` | Alone: it is the status move. | The spec Test command exits 0 on `development`. |

Ordering that keeps `hygiene` green on `development`: `hygiene` is a required check. P5 wires `feature-check` into it, and `feature-check` fails if the Makefile or `ci.yml` lacks a leg for `cortex-grpc`. So P5 lands only after P3 and P4. Until P4 can land, P5 is blocked, and so are the CI halves of REQ-07 and REQ-09. P2 lands unwired and cannot turn any gate red.

The spec status moves from `planned` to `in-progress` with the first landed wave: the integrator changes the frontmatter, the body status line and the registry row in `specs/README.md` in one commit, as `engineering/plans/README.md` says. P6 does the move to `complete`. The integrator writes the CHANGELOG line (spec 0053 D2); no builder does.

## Packages

### P1: amend spec 0033 to match this plan (wave 1)
- **Persona:** `uas-engineer` (spec amendment).
- **Files:** `specs/0033-cargo-feature-policy-and-ci-matrix.md` (exists, 136 lines). Nothing else.
- **Change:** (a) Plan bullet 5: replace "deriving the feature list rather than hard-coding it" with "listing each leg in the `ci.yml` matrix as written, as REQ-0033-09 requires". (b) Plan bullet 6: replace "the current three features" with "the one declared feature (`cortex-grpc` in `uas-engine`) and the two (`crdt-collab`, `delegation`) that specs 0028 and 0029 assume", keeping the follow-up. (c) Test table: add the row `a [features] entry with no comment naming a spec fails hygiene | xtask fixture | REQ-0033-04`. (d) Notes: add two sentences. The `cortex-grpc` leg needs `.contracts/` from `make fetch-contracts` and SSH read access to `cerebral-work/cortex`. Until the CI credential of spec 0048 exists, the `ci.yml` leg cannot pass. (e) Notes: state the leg formats `feature-check` reads: the Makefile line `FEATURE_LEGS := <crate>:<feature> ...` and the `ci.yml` lines `- {crate: <crate>, feature: <feature>}`. Add a "Amended 2026-10-11" Notes line in the style of the existing one.
- **Done-when:** `make spec-check JOBS=2 && make typos && test "$(grep -c 'REQ-0033-04' specs/0033-cargo-feature-policy-and-ci-matrix.md)" -ge 3 && ! grep -q 'deriving the feature list' specs/0033-cargo-feature-policy-and-ci-matrix.md` exits 0.
- **Red-first:** before the edit the last two legs fail (`REQ-0033-04` appears twice, the phrase is present). After green, put the phrase back in a scratch copy and watch the last leg fail.

### P2: xtask `feature-check` and its fixtures (wave 2, needs P1)
- **Persona:** `uas-builder`.
- **Files:** `xtask/src/feature_check.rs` (new), `xtask/src/feature_check_tests.rs` (new, included with `#[path = "feature_check_tests.rs"] mod tests;` as `text_check.rs` line 422 does), `xtask/src/main.rs` (exists, 47 lines: add `mod feature_check;`, one match arm, and the word in the usage line and the doc list). `xtask/Cargo.toml` and `xtask/src/pin_check.rs` are untouched (finding 7). Estimate 300 changed lines [asserted].
- **Change:** `cargo xtask feature-check`, run from the repo root, reads as text: every `crates/*/Cargo.toml` (listed with `read_dir`), `Makefile`, `.github/workflows/ci.yml`. A pure function `check(manifests: &[(String, String)], makefile: &str, ci_yml: &str) -> Vec<String>` returns one line per breach, so the fixtures call it without a file system.
  - Collect each crate's `[package] name` and its `[features]` table (entry name, value list, and the comment lines directly above the entry).
  - REQ-03: a feature name must match `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`; the line names crate and feature.
  - REQ-04: each entry needs a comment line directly above it that contains `spec ` followed by four digits.
  - REQ-11: a name in the `default = [...]` list of the same table is a breach that names the feature. A crate with `default` in `[features]` and an empty list passes.
  - REQ-10: for each workspace-member dependency edge, a `features = [...]` list in the dependency entry, and each `"member/feature"` or `"member?/feature"` string in a `[features]` value list, must name a feature the member declares. A breach names both crates. The `dep:x` form is not an edge.
  - REQ-09: the Makefile legs are the whitespace-separated `crate:feature` words of the first line that starts `FEATURE_LEGS :=`. The CI legs are every `{crate: X, feature: Y}` pair found in `ci.yml`. Each declared (crate, feature) must appear in both; a breach names the feature and which file lacks it. A leg naming a feature nobody declares is also a breach (a stale leg).
  - Output: breaches to stderr, `feature-check: ok (N features checked)` to stdout, exit 1 on any breach.
  - Fixtures, one test per Test-table row of the spec plus one per extra clause (each fn name contains `feature_check`, so the Done-when filter selects it): uncovered feature (Makefile has no leg; `ci.yml` has no leg), wrong crate (`uas-engine` passes `"uas-domain/foo"` where only `uas-engine` declares `foo`), dependency lacks the feature (`features = ["foo"]` on a member that has none), on by default, CamelCase name, no spec comment, stale leg, and a clean fixture that returns no breach. Write them first.
- **Done-when:** `cargo test -p xtask -j 2 feature_check && make fmt-check clippy JOBS=2` exits 0. This is the first leg of the spec Test command. `cargo xtask feature-check` is expected to exit 1 on the base until P3 and P4 have landed (no `ci.yml` leg); the builder records its output and does not wire it.
- **Red-first:** write the tests against `check` returning an empty vector and watch every breach test fail and the clean test pass. After green, delete the REQ-11 branch and watch the on-by-default test go red; restore. Repeat for REQ-09 (delete the `ci.yml` comparison) and REQ-10. Each deletion must turn exactly its own tests red, which is the spec's rule "goes red when its own check is deleted".

### P3: `make features` and `make feature-leg` (wave 1)
- **Persona:** `uas-builder`.
- **Files:** `Makefile` (exists; `.PHONY` list at lines 15 to 17, the `check` and `ci` targets at lines 43 to 46, the `fetch-contracts` comment at lines 70 to 73), `AGENTS.md` (exists; the Commands table, add one row). Estimate 30 changed lines [asserted].
- **Change:** a variable `FEATURE_LEGS := uas-engine:cortex-grpc` on its own line (the format P2 reads). Target `feature-leg` (needs `CRATE` and `FEATURE`; both must match `^[a-z][a-z0-9-]*$` or it exits 2 with a usage line, as `new-repo` does) that runs `cargo clippy -p $$CRATE --features $$FEATURE --all-targets --locked -j $(JOBS) -- -D warnings` then `cargo nextest run -p $$CRATE --features $$FEATURE --locked --build-jobs $(JOBS) --profile $(NEXTEST_PROFILE)`. Target `features` loops `FEATURE_LEGS`, runs `$(MAKE) --no-print-directory feature-leg CRATE=... FEATURE=...` for each, keeps going after a failure, prints `features: FAILED <crate>:<feature>` on stderr for each, and exits 1 if any failed (REQ-05, REQ-07, REQ-08). Add both to `.PHONY`. Neither joins `check` or `ci` (finding 8). Rewrite the comment at lines 70 to 71 to say that `features` needs the same fetch. `AGENTS.md`: one Commands row, `make features`, "`clippy` and `test` once per declared cargo feature. Needs `make fetch-contracts` first."
- **Done-when:** `make fetch-contracts && make features JOBS=2` exits 0 on a workstation with SSH read access (finding 2). Also `make -n features JOBS=2 | grep -c -- '-j 2'` prints at least 1, and `make features JOBS=2` run with `.contracts/` absent exits 1 and prints `features: FAILED uas-engine:cortex-grpc`. Note that the spec Test row for `make features` is this same command. If the first run is red on existing code, stop and report it (finding 9).
- **Red-first:** before the change `make features` exits 2 (`No rule to make target`). After green, break it once on purpose with the contracts present: append `#[cfg(feature = "cortex-grpc")] const _BREAK: u32 = "x";` to `crates/uas-engine/src/lib.rs` in the worktree, run `make features JOBS=2`, and watch it exit 1 with `FAILED uas-engine:cortex-grpc`; then `git checkout crates/uas-engine/src/lib.rs`. This is a compile break under the feature, not the missing-contracts failure, which is red for a different reason. Run `make check JOBS=2` on the broken tree too and watch it stay green: that is the spec's control that matters most.

### P4: `features` matrix job in `ci.yml` (wave 2, needs P3, BLOCKED)
- **BLOCKED on the CI read credential for `cerebral-work/cortex`** (spec 0048 Plan line 152: "until it exists, CI builds default features only"). Without it the only leg fails on every PR with the missing-contracts error of finding 1, and a permanently red job trains reviewers to ignore it. Unblock check: the integrator names how the job gets the credential (a secret name and a step that runs `cargo xtask fetch-contracts`), or rules one of the options in Open 1. The secret itself is operator-gated; this plan does not create it.
- **Persona:** `uas-builder`.
- **Files:** `.github/workflows/ci.yml` (exists, 266 lines at `ec47b1f`). Append one job after `text-check`. Spec 0054's plan edits the header and line 118 of the same file (Open 3). Estimate 40 changed lines [asserted].
- **Change:** job id `features`, `name: features (${{ matrix.crate }}, ${{ matrix.feature }})`. `if:` the same-repo condition used by `hygiene` (line 71 of `ci.yml`). `runs-on: {group: ent-default, labels: [ci-rust]}`, `timeout-minutes: 30` (asserted; the longest `rust / cargo` run is 29 s, spec 0054 F9, so a cold build with the feature is the unknown). `strategy: {fail-fast: false, matrix: {include: [...]}}` with one line per leg, `- {crate: uas-engine, feature: cortex-grpc}`, the format P2 reads. Steps: checkout, the same `Swatinem/rust-cache` action pin as `hygiene` with `shared-key: features-${{ matrix.feature }}`, the same `taiki-e/install-action` pin for `cargo-nextest`, the credential step the integrator names, then `make feature-leg JOBS=10 NEXTEST_PROFILE=ci` with `CRATE: ${{ matrix.crate }}` and `FEATURE: ${{ matrix.feature }}` set in `env:` and no `${{` in the `run:` line (finding 5). The job is not added to the header's required list (finding 6).
- **Done-when:** `grep -c '{crate: uas-engine, feature: cortex-grpc}' .github/workflows/ci.yml` prints 1, `grep -c 'fail-fast: false' .github/workflows/ci.yml` prints at least 1, and the spec 0054 Test block exits 0 on the tree: `bash <(awk '/^```bash/{c++; if(c==1){p=1;next}} /^```/{p=0} p' specs/0054-ci-pipeline-policy.md)` (needs `gh` auth and PyYAML; every row prints `ok`). After merge the PR run shows the leg: `gh pr checks <N> | grep 'features (uas-engine, cortex-grpc)'` prints a pass.
- **Red-first:** the first grep prints 0 today. After green, put `${{ matrix.feature }}` into the `run:` line in a scratch copy and watch the 0054 block report `run interpolation`; remove the `if:` and watch it report `fork guard`; break the feature code as in P3 and watch the PR leg go red alone while other jobs stay green (REQ-07).

### P5: wire `feature-check` into `make hygiene` (wave 3, needs P2, P4, BLOCKED with P4)
- **Persona:** `uas-builder`.
- **Files:** `Makefile` (exists; the `hygiene` line at line 49, the `.PHONY` list, and a new target beside `pin-check` at line 107).
- **Change:** target `feature-check` running `cargo xtask feature-check`; add it to `hygiene` after `pin-check`, to `.PHONY`, with a `##` help line. Nothing else. `hygiene` runs `make hygiene JOBS=10` in `ci.yml` (line 85), so CI picks it up with no workflow edit.
- **Done-when:** `make hygiene JOBS=2` exits 0 and `cargo xtask feature-check` prints `feature-check: ok (1 features checked)`. This is the third leg of the spec Test command.
- **Red-first:** before the change `make feature-check` has no rule. After green, delete `uas-engine:cortex-grpc` from `FEATURE_LEGS` in a scratch copy and watch `make hygiene JOBS=2` exit 1 naming `cortex-grpc` and the Makefile; restore; delete the `ci.yml` line and watch the same for `ci.yml`; restore.

### P6: spec status move (wave 4, needs P1 to P5)
- **Persona:** `uas-engineer`.
- **Files:** `specs/0033-cargo-feature-policy-and-ci-matrix.md` (frontmatter `status`, body status line, Plan checkboxes), `specs/README.md` (registry row for 0033, line 40).
- **Change:** status `planned` to `complete` in the three places in one commit, and tick the Plan items the packages above closed. If P4 and P5 have not landed, the status stays `in-progress` and the blocked items are listed.
- **Done-when:** `make spec-check JOBS=2 && grep -c 'status: complete' specs/0033-cargo-feature-policy-and-ci-matrix.md | grep -qx 1 && grep -q '0033-cargo-feature-policy-and-ci-matrix.md) | Cargo feature policy and CI matrix | complete' specs/README.md && cargo test -p xtask -j 2 feature_check && make features JOBS=2 && make hygiene JOBS=2` exits 0 on `development`.
- **Red-first:** before the edit the second and third legs fail. After green, set the README row back to `planned` in a scratch copy and watch the grep leg fail.

## Open

1. **P4 and P5 wait on the CI credential (finding 1).** Three ways out, the integrator's to rule: (a) wait for the spec 0048 secret; (b) land P4 with the fetch step guarded by the secret being present, skipping the job body when it is empty, the way the `sccache` steps in `_rust.yml` skip on an empty bucket. That makes REQ-09 pass and leaves REQ-06 open, and a skipped job counts as a pass, so the leg proves nothing until the secret exists; (c) rule that `feature-check` compares the Makefile only until the secret exists, which needs a spec amendment to REQ-09. I did not pick one.
2. **Making `features` a required check** is the operator's per-item go (0057 D7) after seven days of runs with no false failure (0054 D7). It is not a package.
3. **Sequencing with spec 0054's plan:** its P1 and P2 edit `ci.yml` (header lines 2 to 3, line 118). P4 appends a job at the end, so a textual conflict is unlikely, but the two must not share a wave.
4. **Not run by me:** `make fetch-contracts`, `make features`, and any clippy or test run with `cortex-grpc` on. Whether the feature is green today is unknown (finding 9). The 0054 Test block was read, not run, since it needs `gh` auth and PyYAML.
5. **The undeclared features:** the follow-up for `crdt-collab` and `delegation` is a tracker item for the integrator. REQ-0033-09 cannot see a feature nobody declares, so spec 0029's Test block, which passes `--features delegation` to three crates, fails on today's tree until its author declares it.
6. **The 30 minute timeout in P4 is a guess** until the first run on the credential-bearing runner is measured.
