<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plan: 0034-tool-pinning-beyond-cargo

Spec: `specs/0034-tool-pinning-beyond-cargo.md` (status planned, 7 requirements, 3 decisions, 2 Test rows). Base: `origin/development` `ec47b1f`, the merge of PR #198.
Planner: `uas-planner`, 2026-10-11. Every path below was read in a worktree cut from that base.
Tiers: `verified` = I ran or read it at the path or command named; `reported` = the spec or a release API says it; `asserted` = my judgement.

## Findings the builders must know

1. **Plan 0055 has not landed [verified]:** at `ec47b1f`, `grep -c '^| `gitleaks` ' specs/0004-dependency-and-licence-policy.md` prints 0 and the CI tools table (lines 266 to 271) holds `gh`, `bpf-linker`, `Rust nightly`, `cargo-semver-checks`. The `gitleaks` and `cargo-cyclonedx` rows are plan 0055 P1 (spec 0034 Plan item 1 names them, so this plan does not package them again). Plan 0055 P2 and P5 edit `xtask/src/pin_check.rs`; every package here that edits that file waits for plan 0055 wave 2.
2. **The spec spells its own Plan bullet at line 46, not 47 [verified: `sed -n 44,49p` in the worktree].** Plan 0055 P6 edits "line 47 only" and will miss it. The integrator should tell the 0055 lane. This plan does not touch lines 44 to 49 until P6.
3. **The spec's Test cannot be run literally [verified]:** `make pin-check` runs `cargo xtask pin-check` on the real tree and exits 0 today, so it cannot go red. `xtask/src/main.rs` calls `pin_check::run(&root)` with a root fixed to `CARGO_MANIFEST_DIR/..` (`repo_root`). Red needs a fixture, so P3 adds `cargo xtask pin-check --root <dir>`, modelled on `text-check --repo` (`xtask/src/text_check.rs` line 6). The test cannot call `make` on a fixture; it runs the built binary. P2 corrects the Test text.
4. **A fixture can fail for the wrong reason [verified: `check_root` reads spec 0004, `Cargo.toml`, member manifests, `Cargo.lock`, and the eBPF manifest and lock].** A fixture missing any of them exits 1 with "cannot read", and a REQ-07 test passes vacuously. Every failing test asserts that stderr names the offending tool, and has a control that differs by one change and exits 0. Tests build their fixtures in a temp dir at run time, as `xtask/tests/text_check.rs` does, so no committed `Makefile` or `Cargo.toml` fixture reaches `machete`, `typos` or `spdx-check`.
5. **Nine remote actions and eleven tools have no row in spec 0004 [verified by grep of `uses:`, `tool:`, `*_VERSION`, `moon-version:` and `cargo <sub>` over `.github/workflows` and `Makefile`].**
   - Actions: `actions/checkout`, `taiki-e/install-action`, `Swatinem/rust-cache`, `release-plz/action`, `mozilla-actions/sccache-action`, `moonrepo/setup-toolchain`, `github/codeql-action` (init and analyze, one SHA), `actions/upload-artifact`, `actions/labeler`. All are pinned by 40-hex SHA with a version comment today.
   - Tools: `cargo-nextest`, `cargo-machete`, `cargo-audit`, `cargo-deny`, `typos`, `moon`, `pgt`, stable Rust `1.94.1`, plus `gitleaks` and `cargo-cyclonedx` (plan 0055 P1). `gh`, `bpf-linker`, `cargo-semver-checks` and the nightly already have rows.
   With the parser on and no rows, `pin-check` goes red, so rows (P1) and pins (P4) land before the parser (P3, P5).
6. **Four `tool:` installs float to latest [verified]:** `ci.yml` lines 83 and 111, `_rust.yml` line 82, `audit.yml` line 23 and `sbom.yml` line 22 carry `tool: cargo-nextest`, `cargo-machete`, `cargo-audit`, `cargo-cyclonedx` with no version. The CI log of run 38131440915 (`ci.yml` on `ec47b1f`) prints `installing cargo-nextest@latest` and downloads `cargo-nextest-0.9.146`, then `installing cargo-machete@latest` and `v0.9.2`. `bpf-linker@0.11.1` at `ci.yml` line 180 is the working precedent for `tool@version`.
7. **install-action manifests at `f7e5d7c` [verified: `gh api repos/taiki-e/install-action/contents/manifests/<tool>.json?ref=f7e5d7c...`]:** `cargo-nextest` latest `0.9.146`, `cargo-machete` latest `0.9.2`, `cargo-cyclonedx` latest `0.5.9`, `cargo-audit` latest `0.22.2`. The builder re-checks that each pinned version has a per-version entry with checksums; a version without one installs without the sha256 check. `audit.yml` has never run (`gh run list --workflow audit.yml` prints `[]`), so `0.22.2` is the manifest's latest and not an observed CI version [reported].
8. **`moon` has two homes [verified]:** `ci.yml` line 106 `moon-version: 2.2.6` and `.prototools` `moon = "2.2.6"`. `moonrepo/setup-toolchain` also installs `proto` globally with no version in the repo (CI log: "Installing `proto` globally"). The plan pins `moon` and records `proto` in Open.
9. **Env var to row mapping cannot be derived from names [asserted by arithmetic: `GH_VERSION` is `gh`, `SEMVER_CHECKS_VERSION` is `cargo-semver-checks`].** The three existing version variables are `GITLEAKS_VERSION` (`ci.yml` line 38), `GH_VERSION` (`release.yml` line 45) and `SEMVER_CHECKS_VERSION` (`semver.yml` line 28). Decision for the plan: a row whose version comes from a workflow variable names it in the Pin cell as the token `env NAME`. This generalizes plan 0055 P5's gitleaks comparison. Plan 0055 P5 keeps its own function; P5 here must not duplicate its output (see its Change).
10. **The spec's extraction list is short [verified against REQ-06 and the workflows].** Plan item 4 names `uses:` and `env:` version variables and Makefile CLI calls. REQ-06 also covers toolchains and Cargo subcommands, and the workflows need `with: tool:`, `with: moon-version:`, `run: rustup toolchain install <chan>`, and the Makefile needs `EBPF_TOOLCHAIN ?=` and `cargo +<chan>` (Makefile lines 12, 131). The plan adds those and P2 records them as decisions.
11. **Local tool drift is out of reach [verified in spec 0045 line 118: "ceres runs cargo-deny 0.19.0 and CI runs 0.19.9 ... Aligning the local version is spec 0034's subject"].** `pin-check` reads files; it cannot see an installed binary. No requirement in spec 0034 closes this. See Open.
12. **No new crate [verified: `xtask/Cargo.toml` has `anyhow` and `sha2`; `pin_check.rs` and `toolchain.rs` parse by line].** The new parsers scan lines and add no YAML or TOML crate, so spec 0004 needs no crate pin. The limit is documented in the module header: block scalars and flow lists spanning lines are not read.

## Decisions the plan takes (recorded in the spec by P2)

- **A. Key.** A CI tools row is keyed by the first backticked token of its first cell. The `Rust nightly (rustup channel)` row has none, so P1 renames it to `` `rustc-nightly` (rustup channel) `` and adds `` `rustc-stable` (rust-toolchain.toml) ``. Version = the first `X.Y.Z` token, or the backticked channel for the two toolchain rows.
- **B. Actions.** One row per `owner/repo`; a `uses: owner/repo/path@ref` maps to `owner/repo`; `./` local uses are skipped. The Pin cell holds the 40-hex SHA and `pin-check` requires the workflow SHA to equal it. The version comment is not compared.
- **C. Tools.** A `with: tool:` entry is `name@X.Y.Z`; a bare name, `@latest` or a range is a miss. `moon-version:` maps to `moon`. A `*_VERSION` variable maps to the row whose Pin cell carries `env NAME`; no row is a miss.
- **D. Makefile.** `cargo <sub>` with `<sub>` outside `build b check c clippy doc d fmt run r test t update metadata tree xtask bench clean fetch vendor package publish install` needs a row keyed `cargo-<sub>` with an `X.Y.Z` version. `cargo install` needs `--rev <sha>`, `--version` or `@X.Y.Z`; pgt's `--rev 54d90f5` passes. `cargo-<sub>` written as a hyphenated binary is read the same way.
- **E. Toolchains.** Every channel in `rust-toolchain.toml`, `crates/uas-ebpf/ebpf/rust-toolchain.toml`, `EBPF_TOOLCHAIN` and `rustup toolchain install` must be `X.Y.Z` or `nightly-YYYY-MM-DD` and must appear as the version of a toolchain row. `stable`, `nightly` and `beta` are misses.
- **F. Baked tools.** `cargo-deny` and `typos` are baked into the `ci-rust` image and appear in no workflow version string. Their rows are records, and the Makefile check only needs the row to exist.
- **G. Output.** The crate success line `pin-check: ok (N pinned crates checked)` stays byte for byte (plan 0055 Test rows match it). Tool results print on their own line, `pin-check: ok (M tool references checked)`. Misses print one per line on stderr and the exit code is 1.

## Plan items to packages

| Spec 0034 Plan item | Package |
|---|---|
| 1. Amend spec 0004 with `gitleaks` and `cargo-cyclonedx` rows | plan 0055 P1 owns both rows. P1 here adds the rest (finding 5) and the `env` tokens |
| 2. `Tool` table parser in `xtask/src/pin_check.rs` | P3 (parser in a new module, one call from `pin_check.rs`) |
| 3. Fixture entry point in `xtask` | P3 (`--root`) |
| 4. Extract `uses:`, `env:` versions and Makefile CLI calls | P3 (Makefile), P5 (workflows, toolchains) |
| Not in the spec Plan: workflow `tool:` versions | P4 |
| Not in the spec Plan: Test rows, decisions A to G | P2 |
| Not in the spec Plan: the status move | P6 |

## Requirement coverage

| Requirement | Closed by |
|---|---|
| REQ-0034-01 | P1 (the rows), P2 (records that actions and toolchains live in the same table) |
| REQ-0034-02 | P5 (the SHA comparison). The workflows already comply (finding 5) |
| REQ-0034-03 | P4 (versions on four `tool:` lines), P5 (the check) |
| REQ-0034-04 | P3 |
| REQ-0034-05 | P1 (the two toolchain rows), P5 (the check) |
| REQ-0034-06 | P3 (Makefile), P5 (workflows, toolchains) |
| REQ-0034-07 | P3 (Makefile), P5 (workflows, toolchains) |

7 of 7 requirements are closed by a package. None is BLOCKED on a ruling. P1, P2, P3, P5 and P6 wait on plan 0055 landing, which is a dependency and not a ruling.

## Waves

| Wave | Packages | Disjoint because | State after the wave |
|---|---|---|---|
| First | P1, P2 | P1: `specs/0004-dependency-and-licence-policy.md`. P2: `specs/0034-tool-pinning-beyond-cargo.md` | Spec 0004 holds a row for every action and tool. Spec 0034 carries decisions A to G and a Test row per requirement. `pin-check` still exits 0 (it ignores the new rows) |
| Second | P3, P4 | P3: `xtask/src/tool_pins.rs` (new), `xtask/src/pin_check.rs`, `xtask/src/main.rs`, `xtask/tests/pin_check_test.rs` (new). P4: `.github/workflows/ci.yml`, `_rust.yml`, `audit.yml`, `sbom.yml` | `pin-check` reads the Makefile and takes `--root`. Every `tool:` line carries a version |
| Third | P5 | `xtask/src/tool_pins.rs`, `xtask/src/pin_check.rs`, `xtask/tests/pin_check_test.rs`: one builder, because P3 owns the same files | `pin-check` reads workflows and toolchains |
| Fourth | P6 | `specs/0034-tool-pinning-beyond-cargo.md`, `specs/README.md` | Spec status moves |

Ordering that keeps `development` green: rows (P1) and versions (P4) land before the parser reads them (P3 for the Makefile, P5 for workflows). P3 needs P1 only for the Makefile tools (`cargo-deny`, `cargo-audit`, `cargo-machete`, `cargo-cyclonedx`, `pgt`). P5 needs P1 and P4. Packages P1 and P2 cannot go red on their own, because no parser reads the rows yet; their red control is in P3 and P5, which run the new binary on the base and list what P1 and P4 fix. The spec status moves to `in-progress` with the first landed wave (the integrator, one commit, three places, per `engineering/plans/README.md`).

## Packages

### P1: spec 0004 CI tools rows for actions, tools and toolchains (wave 1, needs plan 0055 P1)
- **Persona:** `uas-engineer` (spec amendment).
- **Files:** `specs/0004-dependency-and-licence-policy.md` (exists; the `### CI tools` table, lines 266 to 271 at `ec47b1f`, plus plan 0055 P1's rows). Nothing else.
- **Change:** add one row per item, columns `Tool | Version | Licence | Use | Pin`.
  - Nine action rows keyed `` `owner/repo` ``. Version: the tag from the `# vX` comment. Licence: the repo `LICENSE` file read with `gh api repos/<owner>/<repo>/license`. Use: the workflows that call it. Pin: the 40-hex SHA copied from `git grep -h 'uses: <owner/repo>' .github/workflows`.
  - Tool rows: `cargo-nextest` 0.9.146, `cargo-machete` 0.9.2, `cargo-audit` 0.22.2 (finding 7; the builder re-reads the manifests and `cargo info`), `moon` 2.2.6 (Use: `ci.yml` `moon-version` and `.prototools`), `pgt` (version `rev 54d90f5`, Pin: the rev, Use: `make pgt-install`; licence "none, private", as the `### pgt` section says), `cargo-deny` 0.19.9 and `typos` 1.49 (Pin: "baked in `ci-rust-runner:v0.1.6`", per `ci.yml` line 63 and `_rust.yml` line 8).
  - Toolchain rows: rename the nightly row's first cell to `` `rustc-nightly` (rustup channel) ``; add `` `rustc-stable` (rust-toolchain.toml) `` with version `` `1.94.1` ``, licence `MIT OR Apache-2.0`, Use `rust-toolchain.toml and the baked image`.
  - Add `env GH_VERSION` to the `gh` Pin cell, `env SEMVER_CHECKS_VERSION` to `cargo-semver-checks`, and `env GITLEAKS_VERSION` to plan 0055 P1's `gitleaks` row if absent. Keep the table header unchanged.
  - Each licence cell carries the date and the command read, as the existing rows do. Where a tag is a floating major (`codeql-action` `v4`), record the SHA and write "v4 (floating tag, SHA is the pin)".
- **Done-when:** `make spec-check JOBS=2 && make typos && make pin-check` exits 0, and `for t in actions/checkout taiki-e/install-action Swatinem/rust-cache release-plz/action mozilla-actions/sccache-action moonrepo/setup-toolchain github/codeql-action actions/upload-artifact actions/labeler cargo-nextest cargo-machete cargo-audit cargo-deny typos moon pgt rustc-stable rustc-nightly gitleaks cargo-cyclonedx; do test "$(grep -c "^| \`$t\` " specs/0004-dependency-and-licence-policy.md)" = 1 || echo "missing $t"; done` prints nothing. This is the only runnable gate before the parser exists (finding 4 explains why it cannot be a `pin-check` red).
- **Red-first:** run the `for` loop before the edit: it prints a `missing` line for each name that has no row at that point (all 20 at `ec47b1f`, 18 once plan 0055 P1 has landed). After green, delete one action row in a scratch copy and watch the loop print `missing` for it.

### P2: amend spec 0034, Test rows and decisions A to G (wave 1)
- **Persona:** `uas-engineer`.
- **Files:** `specs/0034-tool-pinning-beyond-cargo.md` (exists, 67 lines; keep under 120). Do not touch lines 44 to 49 (finding 2).
- **Change:** (a) Decisions table: rows D4 to D10 for plan decisions A to G above, source "planner, 2026-10-11, ruled by the integrator" only after the integrator confirms; otherwise "proposed". (b) Test: replace the fixture sentence with "A test in `xtask/tests/pin_check_test.rs` runs `cargo xtask pin-check --root <fixture>`; the fixture is built in a temp dir; every failing case asserts stderr names the tool and has a passing control". Add a row per requirement 01 to 05 with its negative control (planted row removed, unpinned `tool:` line, SHA changed by one hex digit, `stable` channel). Keep the existing rows 06 and 07. (c) Facts: add F4 "install-action floats `tool:` without a version" with the command `grep -nE 'tool: [a-z-]+$' .github/workflows/*.yml`.
- **Done-when:** `make spec-check JOBS=2 && make typos` exits 0 and `test "$(grep -c 'REQ-0034-0[1-5]' specs/0034-tool-pinning-beyond-cargo.md)" -ge 10`. The count is 5 today (the five requirement lines) and rises with five Test rows.
- **Red-first:** the count prints 5 before the edit, so the `-ge 10` leg fails. After green, remove one Test row in a scratch copy and watch it fail.

### P3: fixture entry, tool table parser and Makefile check (wave 2, needs P1, P2, plan 0055 wave 2)
- **Persona:** `uas-builder`.
- **Files:** `xtask/src/tool_pins.rs` (new), `xtask/src/pin_check.rs` (exists; one call from `check_root`, one new line in `run` for the tool line), `xtask/src/main.rs` (exists; `mod tool_pins;` and `--root <dir>` for `pin-check`, default unchanged), `xtask/tests/pin_check_test.rs` (new).
- **Change:** in `tool_pins.rs`: `parse_tool_rows(spec) -> BTreeMap<String, ToolRow>` reading the `| Tool | Version |` table per decision A; `check_makefile(makefile, rows) -> Vec<String>` per decision D, one miss per offending line, format `pin-check: Makefile line N: cargo-<sub> has no row in specs/0004 CI tools`. Skip comment lines and lines inside `$(...)` help text. `pin_check::check_root` reads `Makefile` at the root, appends the misses to `Report.misses`, and `run` prints the tool line of decision G. Integration tests (names start `pin_check_`): (1) fixture Makefile with `cargo fake` exits 1 and stderr contains `cargo-fake`; (2) the same fixture plus a `cargo-fake` row with version `1.2.3` exits 0 (control); (3) a row with version `latest` exits 1; (4) `cargo install --git x pgt` without `--rev` exits 1, with `--rev 54d90f5` exits 0; (5) a Makefile using only built-ins exits 0; (6) a fixture missing `Cargo.lock` exits 1 with stderr containing `cannot read` and not `cargo-fake` (proves tests 1 and 3 do not fail vacuously). Unit tests in `tool_pins.rs` for the row parser, including the `gh` row with its parenthesis and two-token Version cell. Keep the diff under 380 lines; if it passes that, stop and report instead of trimming tests.
- **Done-when:** `cargo nextest run -p xtask pin_check -j 2 && make fmt-check clippy JOBS=2 && make pin-check` exits 0 (green on the real tree after P1). The Test rows for REQ-0034-06 and 07 are tests 1 and 2.
- **Red-first:** before the change `cargo xtask pin-check --root .` fails on the unknown flag, and the new test file does not compile. Write test 1 first and watch it fail. After green, delete the body of `check_makefile` and watch tests 1, 3 and 4 go red while test 5 stays green. On the real tree, add `	cargo fake` to the `Makefile` in a scratch copy: `make pin-check` exits 1 naming `cargo-fake`; restore. Record in the PR body the misses the binary prints against base `ec47b1f` with P1 reverted: they name `cargo-deny`, `cargo-audit`, `cargo-machete`, `cargo-cyclonedx` and `pgt`.

### P4: version every install-action tool line (wave 2, needs P1)
- **Persona:** `uas-builder`.
- **Files:** `.github/workflows/ci.yml` (lines 83, 111), `.github/workflows/_rust.yml` (line 82), `.github/workflows/audit.yml` (line 23), `.github/workflows/sbom.yml` (line 22). No other line.
- **Change:** `cargo-machete` becomes `cargo-machete@0.9.2`; both `cargo-nextest` lines become `cargo-nextest@0.9.146`; `cargo-audit` becomes `cargo-audit@<the version P1 recorded>`; `cargo-cyclonedx` becomes `cargo-cyclonedx@0.5.9`. The numbers must equal the P1 rows. `sbom.yml` is also touched by spec 0045 and plan 0055 P7 (BLOCKED today); this is a one-token edit that P7 must rebase over. The integrator should order it ahead of any `sbom.yml` work.
- **Done-when:** `! grep -rnE 'tool: *[a-z-]+ *$' .github/workflows && actionlint .github/workflows/*.yml && make pin-check` exits 0. The pull request's own CI runs each changed job; the integrator confirms the install-action step log prints `installing cargo-nextest@0.9.146` and a verified checksum line.
- **Red-first:** the first grep prints the five lines (finding 6) before the edit, so the negated command exits 1. After green, strip `@0.9.2` from one line in a scratch copy and watch the grep print it.

### P5: workflow and toolchain checks (wave 3, needs P3, P4, plan 0055 wave 2)
- **Persona:** `uas-builder`.
- **Files:** `xtask/src/tool_pins.rs`, `xtask/src/pin_check.rs` (one more call in `check_root`), `xtask/tests/pin_check_test.rs`.
- **Change:** `check_workflows(name, text, rows)` scanning each line: `uses:` per decision B; `tool:` entries and `moon-version:` per C; `<NAME>_VERSION:` variables per C, quotes stripped, then the row version equals the workflow value; `rustup toolchain install <chan>` per E. `check_toolchains(files, rows)` reads the `channel` of `rust-toolchain.toml` and `crates/uas-ebpf/ebpf/rust-toolchain.toml` and `EBPF_TOOLCHAIN ?=` from the Makefile per E (P3's `check_makefile` already skips `cargo +chan`; the channel check lives here). `check_root` reads `.github/workflows/*.yml` sorted by name. Do not compare the gitleaks version between `ci.yml`, `lefthook.yml` and the row: that is plan 0055 P5. This package compares only the workflow value to the row, so one drift prints one line from each check at most. Tests (names start `pin_check_`, each failing case with a one-change control and a stderr assertion): a SHA differing by one hex digit; a `uses:` with no row; a `tool:` with no version and one with `@latest`; a `GITLEAKS_VERSION` differing from the row; an unknown `FOO_VERSION`; `stable` and `nightly-2026-10-07` channels; a local `./` use passes; `owner/repo/path` maps to `owner/repo`. Keep under 380 lines; the toolchain half moves to a new package if it does not fit.
- **Done-when:** `cargo nextest run -p xtask pin_check -j 2 && make fmt-check clippy JOBS=2 && make pin-check` exits 0. On the real tree `make pin-check` prints `pin-check: ok (M tool references checked)`; the builder records M (no figure is asserted here).
- **Red-first:** before the change a planted workflow SHA in `ci.yml` leaves `make pin-check` at exit 0. After the change, change one hex digit of the `actions/checkout` SHA in `ci.yml` in a scratch copy: exit 1 naming `actions/checkout`; restore. Then delete `check_workflows` bodies and watch the new tests go red. Record the misses the binary prints against base `ec47b1f` with P1 and P4 reverted: they name the nine actions and the five unversioned `tool:` lines.

### P6: spec status move (wave 4, needs P1 to P5, plan 0055 P6)
- **Persona:** `uas-engineer`.
- **Files:** `specs/0034-tool-pinning-beyond-cargo.md` (frontmatter `status`, body status line, Plan checkboxes), `specs/README.md` (registry row, line 41).
- **Change:** status `planned` to `complete` in the three places in one commit, tick the four Plan items P1 to P5 closed, and strip the notes that name `cargo-cyclonedx` and `gitleaks` as defects if plan 0055 P1 closed them.
- **Done-when:** `make spec-check JOBS=2 && make pin-check && grep -c 'status: complete' specs/0034-tool-pinning-beyond-cargo.md | grep -qx 1 && grep -q '0034-tool-pinning-beyond-cargo.md) | Tool pinning beyond cargo | complete' specs/README.md` exits 0 on `development`.
- **Red-first:** the last two legs fail before the edit (status is `planned`). After green, set the README row back to `planned` in a scratch copy and watch `spec-check` or the grep fail.

## Open

- **Local tool drift (finding 11).** Closing "ceres runs cargo-deny 0.19.0, CI 0.19.9" needs a requirement in the spec (a `make tool-check` that runs each `<tool> --version` against its row). The spec has none. The integrator rules whether to add it as a follow-up package.
- **`cargo-audit` version.** No CI run has installed it, so `0.22.2` is the manifest's latest and not an observed version (finding 7).
- **Implicit installs.** `moonrepo/setup-toolchain` installs `proto`, `mozilla-actions/sccache-action` downloads `sccache`, and `release-plz/action` fetches the `release-plz` binary. None carries a repo-visible version. P1 does not add rows for them and the parser cannot see them.
- **Spec 0039 REQ-07.** The `k3d` and `kubectl` rows are spec 0039's. After P5, a workflow that calls them without a row fails `pin-check`, so that plan's row package lands first.
- **Line-scan limits (finding 12).** A block scalar `tool: >` or a flow list across lines is not read, and a bare miss would not appear. A real YAML parser needs a spec 0004 pin; no builder adds one.
- **Concurrency.** Plan 0055 P1, P2, P5 and P6, and spec 0045, touch the same spec 0004 table, `pin_check.rs`, spec 0034 line 46 and `sbom.yml`. The integrator runs one wave at a time across both plans.
- **Not run.** I did not build `xtask` or run `make pin-check` at the base; the plan states what each package's gate should print and each builder records the real figure.
