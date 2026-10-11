<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plan: 0045-security-disclosure-and-advisory-policy

Spec: `specs/0045-security-disclosure-and-advisory-policy.md` (status planned, 11 requirements, 8 decisions, 9 Test rows). Base: `origin/development` `ec47b1f`, the merge of PR #198.
Planner: `uas-planner`, 2026-10-11. Every path below was read in a worktree cut from that base.
Tiers: `verified` = I ran or read it at the path or command named; `reported` = the spec or a release API says it; `asserted` = my judgement.

## Findings the builders must know

1. **`make sbom` is one line from 1.5 [verified]:** `Makefile` line 147 is `cargo cyclonedx --format json`; the target is `sbom:` at line 146. `*.cdx.json` is in `.gitignore` line 16, so a local `make sbom` leaves nothing to commit. Plan 0055 P7 is BLOCKED on this one-word change, so P1 is alone-and-first.
2. **`sbom.yml` install path decided: `curl` plus `sha256sum -c` [verified/asserted]:** spec 0004 line 263 says a downloaded tool is pinned "by version and sha256", and REQ-0034-03 asks for version or hash. `taiki-e/install-action` verifies against its own manifest, and that manifest hashes a different tarball than the row would record: for cargo-cyclonedx 0.5.9 the manifest at `f7e5d7c` lists the `musl` tarball `9bd3e599...`, while plan 0055 finding 7 records the `gnu` tarball `fb8dbee9...` [verified: `gh api repos/taiki-e/install-action/contents/manifests/cargo-cyclonedx.json?ref=f7e5d7c`]. With `curl` the digest in the spec 0004 row is the digest the workflow checks, as in `semver.yml` lines 40 to 41. Cost: one 12-line block in each of `sbom.yml`, `release.yml` and `audit.yml`. The choice is the planner's under spec 0034 REQ-03; spec 0045 D4 requires only "pinned in spec 0004 (spec 0034)". Cite REQ-0034-03 and spec 0004 line 263 in the commit.
3. **Release assets [verified]:** `gh api` on cargo-cyclonedx release `cargo-cyclonedx-0.5.9` lists `cargo-cyclonedx-x86_64-unknown-linux-gnu.tar.xz` with digest `sha256:fb8dbee9f182173e062a64a387b21a0badc6fab8b2abf9294973f012972bf6d8`. cargo-audit release `cargo-audit/v0.22.1` (published 2026-02-04) lists `cargo-audit-x86_64-unknown-linux-gnu-v0.22.1.tgz` with digest `sha256:1890badd5f15831a9af4b074399fcd21e6f7c0fe42c84e9254cdffc9f813765c`. Both are [reported] by the release API; the builder re-checks each with a local `sha256sum`, as spec 0004 did for `cargo-semver-checks`. Pin cargo-audit at 0.22.1: spec F7 measured it, and 0.22.2 exists (install-action manifest latest) but nobody measured it.
4. **The `cargo-cyclonedx` row of spec 0004 belongs to plan 0055 P1 [verified]:** `grep -c cyclonedx specs/0004-dependency-and-licence-policy.md` prints 0 at the base. This plan does not write it. P5 and P6 carry the digest from finding 3 and check `grep -c '^| `cargo-cyclonedx` ' specs/0004-dependency-and-licence-policy.md` prints 1 before they merge; if it prints 0, the 0055 P1 builder has not landed and the package waits.
5. **`deny.toml` has no `ignore` key today [verified]:** `[advisories]` holds only `yanked = "deny"` (`deny.toml` lines 24 to 25). `advisory-check` therefore passes on the real file with zero entries, and `make audit` runs `cargo audit --deny warnings` with no `--ignore`. REQ-0045-04 is true vacuously until the first exception.
6. **cargo-deny is baked into the CI image [verified]:** `ci.yml` line 63, "cargo-deny 0.19.9 is baked into the ci-rust image". The `release.yml` step in P6 runs `cargo deny check advisories` with no install. ceres has 0.19.0 (spec F5), so a local run can differ from CI on advisory-db handling only, not on the keys this plan uses.
7. **`xtask` has no date library and spec 0004 pins none for it [verified]:** `xtask/Cargo.toml` holds `anyhow` and `sha2` only. P4 adds `toml` (pinned, spec 0004 `toml` row, `1.1.8+spec-1.1.0`) and nothing else. The date compare is a civil-date to day-count function written in the module (about 15 lines). Do not add `chrono` or `time`; rule 12 of `AGENTS.md` (line 45) forbids an unpinned dependency.
8. **The `toml` pin row names its users [verified]:** spec 0004 line 198 has `uas-cli, uas-engine` in the "Used by" cell, and the `### toml` section (line 113) is a "Superseded" note. Spec 0045's Plan item says to amend the section to name `xtask` first (P2). `pin-check` compares versions only, so the amendment cannot turn it red.
9. **`development` now lists six required checks [verified]:** `gh api repos/unsigned-gg/uas/branches/development/protection --jq '.required_status_checks.contexts|join(",")'` prints `secrets,rust / cargo,cargo-deny,hygiene,ebpf-live,fork-gate`; `main` prints `secrets,rust / cargo,cargo-deny,hygiene`. Spec F3 omits `fork-gate`. REQ-0045-01 holds on both branches today and no package closes it. This does not change the spec's meaning; spec F3 can add `fork-gate` in a later amendment.
10. **`audit.yml` and `release.yml` overlap other plans [verified by reading plan 0055]:** plan 0055 P4 adds a Makefile target, P7 edits `sbom.yml`, and P1 edits spec 0004. P1 below (Makefile `sbom`) and P7 below (Makefile `audit`, `hygiene`, `release-notes`) touch other lines than 0055 P4. P5 touches `sbom.yml` lines 20 to 22 and 0055 P7 touches the `make sbom` step and `on:`. The integrator lands each overlapping pair one at a time; a rebase is expected and no logic conflicts.

## Plan items to packages

| Spec Plan item | Package |
|---|---|
| `xtask`: `advisory-check` subcommand, `toml` dependency, five fixtures | P2 (spec 0004 amendment naming `xtask`), P4 (the code) |
| `Makefile`: `hygiene` gains `advisory-check`; `audit` with `--deny warnings` and `--ignore`; `release-notes` runs `cargo deny check advisories` first | P7 |
| `Makefile`: `sbom` gains `--spec-version 1.5` | P1 (own early package) |
| `audit.yml`: pin `cargo-audit`; `advisory-check`; the D7 Linear step | P8 (pin and `advisory-check`), P9 (Linear step, BLOCKED) |
| `sbom.yml`: install `cargo-cyclonedx@0.5.9` | P5 |
| `release.yml`: deny step; install, `make sbom`, stage and `gh release upload` | P6 |
| Spec 0004 CI tools rows for `cargo-audit` and `cargo-cyclonedx` | P2 (`cargo-audit`); `cargo-cyclonedx` is plan 0055 P1, not planned here |
| `SECURITY.md`: the D3 text | P3 |
| Status move (plan README) | P10 |

## Requirement coverage

| Requirement | Package |
|---|---|
| REQ-0045-01 | no package: already true on both branches (finding 9; `Makefile` line 40 is `make deny`). Test row 1 is the proof |
| REQ-0045-02 | P7 (`make audit`), P8 (workflow steps) |
| REQ-0045-03 | P9 (BLOCKED on the Linear key); P8 keeps `audit` out of the required checks, which satisfies "blocks no PR and no release" |
| REQ-0045-04, 05 | P4 (the check and fixtures); P7 puts it in `hygiene` |
| REQ-0045-06 | P7 |
| REQ-0045-07 | P6 |
| REQ-0045-08 | P6 |
| REQ-0045-09 | P1 (the flag), P5 (the pin; retention is already 90 days at `sbom.yml` line 36) |
| REQ-0045-10 | P3 |
| REQ-0045-11 | no package: not exercisable while PVR returns 404 (spec F2, Test row 9). P3 keeps the conditional PVR sentence |

9 of 11 requirements have a package (02 to 10). Of the 9, REQ-0045-03 waits on P9, which is BLOCKED. REQ-0045-01 and 11 have none. D8 has no requirement; its README change is plan 0048 P12.

## Waves

| Wave | Packages | Disjoint because | State after the wave |
|---|---|---|---|
| First | P1, P2, P3 | P1: `Makefile` line 147. P2: `specs/0004-dependency-and-licence-policy.md`. P3: `SECURITY.md` | `make sbom` writes 1.5, so plan 0055 P7 unblocks. Spec 0004 allows `toml` in `xtask` and pins `cargo-audit`. `SECURITY.md` holds the 5 business day target |
| Second | P4, P5, P6 (P4 needs P2; P6 needs P1) | P4: `xtask/src/advisory_check.rs` (new), `xtask/src/main.rs`, `xtask/Cargo.toml`, `Cargo.lock`, `xtask/tests/` (new files). P5: `.github/workflows/sbom.yml`. P6: `.github/workflows/release.yml` | `advisory-check` exists and is tested. `sbom.yml` installs a digest-checked 0.5.9. A release is blocked on advisories and carries SBOM assets |
| Third | P7, P8 (both need P4) | P7: `Makefile`. P8: `.github/workflows/audit.yml`. Land them together; P8's run calls the P7 target | `hygiene` runs `advisory-check`. `make audit` reads the ignore list. The weekly job is pinned and runs both checks |
| Fourth | P9 (BLOCKED, needs P8), P10 (needs P1 to P9) | P9: `.github/workflows/audit.yml`. P10: `specs/0045-security-disclosure-and-advisory-policy.md`, `specs/README.md` | REQ-0045-03 closed; spec status `complete` |

The spec status move to `in-progress` rides with the first landed wave: the integrator changes the frontmatter, the body status line and the registry row in one commit (`engineering/plans/README.md`).

## Packages

### P1: `make sbom` writes CycloneDX 1.5 (wave 1)
- **Persona:** uas-builder
- **Files:** `Makefile` (line 147 only)
- **Change:** `cargo cyclonedx --format json` becomes `cargo cyclonedx --format json --spec-version 1.5`. Nothing else. The `.PHONY` list and the help text stay. About 1 changed line. Commit body cites REQ-0045-09 and says plan 0055 P7 unblocks.
- **Done-when:** `test "$(make -n sbom | grep -c -e '--spec-version 1.5')" = 1`. Where `cargo-cyclonedx` 0.5.9 is installed (`cargo install cargo-cyclonedx --version 0.5.9 --locked`; it is not on ceres today, `command -v cargo-cyclonedx` prints nothing), also `make sbom && find crates xtask -name '*.cdx.json' -not -path '*/target/*' | xargs jq -e '.specVersion == "1.5"'`, then delete the generated files.
- **Red-first:** run the first command before the edit: it prints 0 and exits non-zero. After the edit, remove `--spec-version 1.5` once and watch it fail again. With the tool installed, `jq` prints `false` for `"1.3"` before the edit.

### P2: spec 0004 names `xtask` as a `toml` user and pins `cargo-audit` (wave 1)
- **Persona:** uas-engineer (spec amendment)
- **Files:** `specs/0004-dependency-and-licence-policy.md`
- **Change:** (a) the `toml` row (line 198): add `xtask` to the "Used by" cell; append "and, for `cargo xtask advisory-check`, `deny.toml` (spec 0045)" to the notes cell. (b) The `### toml` section (line 113): add one sentence that `xtask` reads `deny.toml` with it (spec 0045 plan; the version stays 1.1.8). (c) In the CI tools table, insert a `cargo-audit` row directly after the `gh` row (line 268), not at the table end, so it cannot collide with plan 0055 P1's appended rows. Cells: version `0.22.1, published 2026-02-04`; licence from `cargo info cargo-audit@0.22.1` (the builder reads it); use `audit.yml` and `make audit`; pin `sha256 for cargo-audit-x86_64-unknown-linux-gnu-v0.22.1.tgz` `1890badd5f15831a9af4b074399fcd21e6f7c0fe42c84e9254cdffc9f813765c` with the date of a local `sha256sum` of the downloaded tarball beside the release digest (the `cargo-semver-checks` row is the shape). Cite spec 0045 D1. The first cell is the tool name in backticks.
- **Done-when:** `test "$(grep -c '^| `cargo-audit` ' specs/0004-dependency-and-licence-policy.md)" = 1 && grep -n '^| `toml` ' specs/0004-dependency-and-licence-policy.md | grep -q xtask && make spec-check JOBS=2 && make pin-check && make typos`
- **Red-first:** the `grep -c` prints 0 before the edit, so the command fails. After, change the digest by one character and confirm the builder's local `sha256sum` comparison would catch it (read, not a gate); remove the `xtask` word once and the second grep fails.

### P3: `SECURITY.md` carries the D3 text (wave 1)
- **Persona:** uas-builder
- **Files:** `SECURITY.md`
- **Change:** replace line 17 ("We acknowledge a report as soon as we can. This repo sets no fixed response time.") with "We acknowledge a report within 5 business days." Keep line 7 (the `chris@todie.io` channel), line 13 (the PVR sentence, which is already conditional: "if it is enabled"), line 23 (latest release on `main` only) and line 27 (a found secret is compromised and the owner rotates it). Leave out "the operator gate for secrets" and "the spec 0003 flow": D3 names them as internal references, and the public text needs neither (REQ-0045-10 asks for the rotation rule only). About 1 changed line.
- **Done-when:** `test "$(grep -c '5 business days' SECURITY.md)" = 1 && ! grep -q 'no fixed response time' SECURITY.md && make typos` (Test row REQ-0045-10).
- **Red-first:** before the edit the first grep prints 0 and the second matches, so the command exits non-zero. After, restore the old sentence once and watch it fail.

### P4: `cargo xtask advisory-check` (wave 2, needs P2)
- **Persona:** uas-builder
- **Files:** `xtask/src/advisory_check.rs` (new), `xtask/src/main.rs`, `xtask/Cargo.toml`, `Cargo.lock`, `xtask/tests/advisory_check.rs` (new), `xtask/tests/fixtures/advisory/valid.toml` (new), `expired.toml` (new), `too-far.toml` (new), `bare-id.toml` (new), `no-ticket.toml` (new). New `.rs` files carry `// SPDX-License-Identifier: Apache-2.0 OR MIT` on line 1 (`make spdx-check`).
- **Change:** (a) `xtask/Cargo.toml`: `toml = { workspace = true }` under `[dependencies]`; `Cargo.lock` gains the edge only (no version change; `make pin-check` stays green). (b) `main.rs`: `mod advisory_check;`, an arm `Some("advisory-check") => advisory_check::run(&root, &rest)` shaped like `text-check` at `main.rs` lines 36 to 37, the doc-comment list and the usage string. (c) `advisory_check.rs`: `pub fn run(root, args) -> ExitCode` with flags `--today YYYY-MM-DD` (default: the UTC date from `SystemTime`), `--config <path>` (default `root/deny.toml`) and `--ids`. A pure `fn check(text: &str, today: Date) -> Vec<String>` does the work so tests need no process. It reads `advisories.ignore` from a `toml::Table`. Per entry: a string is a bare id and fails; a table needs string `id` and `reason`; `reason` splits on `"; "` into at least three non-empty parts, the first `expires YYYY-MM-DD` with a real calendar date (ticket and why are non-empty text, no id pattern, spec D2 sets none); expiry before `today` fails (an expiry equal to today passes); expiry more than 90 days after `today` fails. Each failure prints `advisory-check: <id>: <problem>` to stderr and the exit is 1. Success prints `advisory-check: ok (<n> ignore entries)` and exits 0. A missing key means zero entries. `--ids` prints each id, one per line, and exits 0 without validating dates; an unparsable file or a missing `--config` path exits 1. Write the civil-date to day-count function in the module (finding 7). (d) Fixtures, each a minimal `[advisories]` table: `valid.toml` one entry expiring `2027-01-08`; `expired.toml` `2026-10-09`; `too-far.toml` `2027-01-09`; `bare-id.toml` `ignore = ["RUSTSEC-2021-0139"]`; `no-ticket.toml` reason `expires 2027-01-08; unmaintained, no fix`. With `--today 2026-10-10` the 90-day edge is exact: 2027-01-08 is 90 days out, 2027-01-09 is 91. (e) Tests, every name starting `advisory_`: one per fixture through `check`, one for `--ids`, one for a file with no `ignore` key, and one integration test that runs `env!("CARGO_BIN_EXE_xtask")` (as `xtask/tests/text_check.rs` line 210 does) on `valid.toml` and `expired.toml` and asserts exit 0 and 1. Estimate 280 changed lines including fixtures.
- **Done-when:** `cargo nextest run -p xtask advisory -j 2 && cargo xtask advisory-check` (the spec's own Test command, first half, plus the real `deny.toml`), then `make hygiene JOBS=2` for `machete`, `pin-check` and `spdx-check`.
- **Red-first:** write the fixtures and the tests first against a `check` that returns no errors: the four failing-fixture tests fail. Implement. Then delete the date compare once and watch the `expired` and `too-far` tests fail. Also change `valid.toml` to `2027-01-09` once and watch its test fail.

### P5: `sbom.yml` installs a digest-checked cargo-cyclonedx 0.5.9 (wave 2, soft needs plan 0055 P1)
- **Persona:** uas-builder
- **Files:** `.github/workflows/sbom.yml`
- **Change:** replace the `taiki-e/install-action` step (lines 20 to 22) with a step shaped like `semver.yml` lines 27 to 41: env `CYCLONEDX_VERSION: 0.5.9` and `CYCLONEDX_SHA256: fb8dbee9f182173e062a64a387b21a0badc6fab8b2abf9294973f012972bf6d8`; `curl -sSfL --retry 3` the `cargo-cyclonedx-x86_64-unknown-linux-gnu.tar.xz` asset from `https://github.com/CycloneDX/cyclonedx-rust-cargo/releases/download/cargo-cyclonedx-${CYCLONEDX_VERSION}/`; `echo "${CYCLONEDX_SHA256}  <file>" | sha256sum -c -`; extract with `tar -xJf`; put the directory holding the `cargo-cyclonedx` binary on `$GITHUB_PATH`. The binary sits under a directory named for the tarball [reported: install-action manifest `bin` field for the musl build]; the builder lists the archive (`tar -tJf`) and uses the real path. If `xz` is missing on the `ci-rust` image the step fails, which is a finding for the integrator and is not worked around by an `apt` install. Keep retention at 90 days (line 36, no edit). Cite finding 2 in the commit. Do not touch the `make sbom` step or `on:`: plan 0055 P7 owns them. About 18 changed lines.
- **Done-when:** `grep -c 'taiki-e/install-action' .github/workflows/sbom.yml` prints 0 and `grep -q 'sha256sum -c' .github/workflows/sbom.yml && grep -q 'retention-days: 90' .github/workflows/sbom.yml && make hygiene JOBS=2` exits 0. The workflow run itself is the integrator's: `gh workflow run sbom.yml --ref <branch>` then `gh run view <id> --json conclusion` prints `success`, and the artifact's `jq .specVersion` prints `"1.5"` (needs P1 merged first).
- **Red-first:** before the edit the first grep prints 1. After the edit, flip one hex digit of `CYCLONEDX_SHA256` once in a local copy and run the download-and-check lines by hand: `sha256sum -c` exits 1.

### P6: `release.yml` blocks on advisories and attaches the SBOM (wave 2, needs P1)
- **Persona:** uas-builder
- **Files:** `.github/workflows/release.yml`
- **Change:** (a) After "Check gh and the token" (line 62 to 70) and before the release-plz step (line 73), add a step `cargo deny check advisories` with no `if:` (it must run on a dispatch from a branch too, spec Test row REQ-0045-07). It needs no install (finding 6). (b) Give the release-plz step `id: release-plz`. (c) After it, three steps, each `if: github.ref == 'refs/heads/main' && steps.release-plz.outputs.releases_created == 'true'`: install the pinned cargo-cyclonedx (the P5 block, same digest); `make sbom` then stage `find . -name '*.cdx.json' -not -path './target/*' -not -path './sbom/*' -exec cp {} sbom/ \;` into `sbom/` as `sbom.yml` lines 27 to 28 do, with `test -n "$(ls -A sbom)"`; and `gh release upload "$TAG" sbom/*.cdx.json` with `GH_TOKEN: ${{ github.token }}` (`contents: write` is already granted, line 31). Take `TAG` from the `releases` output (`jq -r '.[].tag'`; the JSON field name is [asserted], the builder checks it in the release-plz docs or `action.yml` at `18641b6`) or, if absent, from `v` plus the workspace version in `cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version'`. Update the header comment with one line about the advisory step. About 45 changed lines.
- **Done-when:** `grep -q 'cargo deny check advisories' .github/workflows/release.yml && grep -q 'releases_created' .github/workflows/release.yml && grep -q 'gh release upload' .github/workflows/release.yml && make hygiene JOBS=2` exits 0. Spec Test rows REQ-0045-07 and 08 need a GitHub act and are the integrator's: push a scratch branch with `ansi_term = "=0.12.1"` in `xtask/Cargo.toml`, `gh workflow run release.yml --ref <branch>`, and `gh run view <id> --json conclusion` must print `failure` with the deny step in `--log-failed`; after the next real release, `gh release view v<version> --json assets` lists one `.cdx.json` per package (11 on 2026-10-10) with `specVersion` `1.5`.
- **Red-first:** before the edit the first grep fails. For the dispatch row, the control is the run on the same scratch branch without the deny step: it succeeds, because release-plz runs only on `main` (line 74). Remove the `*.cdx.json` find once and the staging `test -n` fails.

### P7: Makefile gates (wave 3, needs P4)
- **Persona:** uas-builder
- **Files:** `Makefile`
- **Change:** (a) `.PHONY` (line 15) and a new target `advisory-check: ## Check the deny.toml advisory ignore entries (spec 0045)` running `cargo xtask advisory-check`; add `advisory-check` to the `hygiene` prerequisite list (line 49). (b) `audit` (lines 143 to 144) becomes, under the file's `.ONESHELL` and `-euo pipefail`: `ids="$$(cargo xtask advisory-check --ids)"`; `args=(); for id in $$ids; do args+=(--ignore "$$id"); done`; `cargo audit --deny warnings $${args[@]+"$${args[@]}"}`. The `+` expansion keeps an empty list working under `set -u` on bash older than 4.4. (c) `release-notes` (line 152): the first recipe line, before the usage check, is `cargo deny check advisories || { echo "release-notes: advisories fail on this tree; no release (spec 0045 D5)" >&2; exit 1; }`. Nothing before it edits a file. Plan 0055 P4 also edits this Makefile (a new target); keep the hunks apart. About 20 changed lines.
- **Done-when:** the spec rows in a scratch worktree of the branch. (1) `make hygiene JOBS=2` exits 0. (2) REQ-0045-06: add `ansi_term = "=0.12.1"` to `xtask/Cargo.toml`, `git add xtask/Cargo.toml Cargo.lock`, then `make release-notes VERSION=9.9.9` exits non-zero and `git diff --quiet` exits 0. (3) REQ-0045-02: with that pin, `make audit` exits non-zero; add `ignore = [{ id = "RUSTSEC-2021-0139", reason = "expires <date within 90 days>; CER-0; scratch" }]` under `[advisories]` in the scratch `deny.toml` and `make audit` exits 0. (4) `cargo nextest run -p xtask advisory -j 2 && make deny hygiene JOBS=2` exits 0 on the real tree.
- **Red-first:** run step (2) before the edit: it exits 0 and edits `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md` (spec Test row 6, measured 2026-10-10). Drop `--deny warnings` once: step (3)'s first run exits 0. Drop the `--ignore` expansion once: step (3)'s second run exits non-zero. The ansi_term pin is the scratch control from spec F5 and F7; undo it with `git checkout .` in the scratch worktree.

### P8: `audit.yml` pinned, with `advisory-check` (wave 3, needs P4; lands with P7)
- **Persona:** uas-builder
- **Files:** `.github/workflows/audit.yml`
- **Change:** replace the `taiki-e/install-action` step (lines 21 to 23) with the P5 `curl` plus `sha256sum -c` shape for `cargo-audit` 0.22.1 (`CARGO_AUDIT_SHA256: 1890badd5f15831a9af4b074399fcd21e6f7c0fe42c84e9254cdffc9f813765c`, asset `cargo-audit-x86_64-unknown-linux-gnu-v0.22.1.tgz` from `https://github.com/rustsec/rustsec/releases/download/cargo-audit/v0.22.1/`; the builder lists the archive for the binary path). In the `make audit` step (lines 24 to 30) run `cargo xtask advisory-check` as well and fold both into the status line, so either failing leaves the job red: capture each exit code, write both to the summary, `test "$status" = clean`. Update the header comment (lines 1 to 3): `cargo deny` gates, this job detects, the ignore list is shared. Do not add a secret reference or a ticket step (P9). About 30 changed lines.
- **Done-when:** `grep -c 'taiki-e/install-action' .github/workflows/audit.yml` prints 0 and `grep -q 'cargo xtask advisory-check' .github/workflows/audit.yml && grep -q 'make audit' .github/workflows/audit.yml && make hygiene JOBS=2` exits 0 (Test row REQ-0045-02, first command). The `gh workflow run audit.yml --ref <branch>` legs of rows 2 and 3 need a GitHub act and are the integrator's.
- **Red-first:** before the edit the first grep prints 1. After, remove the `advisory-check` line once and the second grep fails.

### P9: Linear ticket step on a red weekly run (wave 4, BLOCKED, needs P8)
- **BLOCKED on the operator adding the Linear API key** as a repository secret: spec 0045 D7, "adding that secret is an operator-gated secret action, and `audit.yml` references no secret today". The secret's name is also unset; the operator picks it and the integrator passes it. Unblock check: `gh secret list --repo unsigned-gg/uas` shows the name (the name only, never the value).
- **Persona:** uas-builder
- **Files:** `.github/workflows/audit.yml`
- **Change:** a final step `if: failure()` that, using the secret, finds the open ticket titled `cargo audit: advisories on main` on team CER and comments on it with the run URL, or creates it if none is open (Linear GraphQL at `https://api.linear.app/graphql` through `curl`; the CER team id is read from the API, not typed in). The job keeps `permissions: contents: read`. The step must not echo the secret and must keep the credential read and the call in separate steps if the repo guard hooks say so. About 50 changed lines.
- **Done-when:** spec Test row REQ-0045-03: `grep -c 'secrets\.' .github/workflows/audit.yml` prints at least 1, then the scratch-branch run (`ansi_term = "=0.12.1"` pushed, `gh workflow run audit.yml --ref <branch>`) leaves a created or commented CER ticket. The run is the integrator's.
- **Red-first:** `grep -c 'secrets\.' .github/workflows/audit.yml` prints 0 today (spec row 3; control: the same grep on `_rust.yml` prints 4 [reported]). A run with the secret unset must fail the step visibly, not skip it.

### P10: spec status move (wave 4, needs P1 to P9)
- **Persona:** uas-engineer
- **Files:** `specs/0045-security-disclosure-and-advisory-policy.md`, `specs/README.md`
- **Change:** status `planned` to `complete` in the frontmatter, the body status line and the registry row (`specs/README.md` line 52) in one commit, once the spec Test command exits 0 on `development`. The `in-progress` move rides with wave 1 (integrator).
- **Done-when:** `cargo nextest run -p xtask advisory -j 2 && make deny hygiene JOBS=2` exits 0 on `development`, and `make spec-check JOBS=2` exits 0.
- **Red-first:** edit the registry row to differ from the frontmatter once and `make spec-check` should fail (the three places must agree); this assumes spec 0052 checks it, which I did not verify.

## Open

- **P9 and REQ-0045-03 are BLOCKED** on the Linear key (finding above, spec D7). P10 waits on P9, so the spec cannot reach `complete` before the operator acts.
- **Manual GitHub acts are the integrator's:** Test rows REQ-0045-02 (workflow legs), 03, 07 and 08 need pushed scratch branches, `gh workflow run` and, for row 8, a real release. No package runs them.
- **Plan 0055 P1 ordering:** P5 and P6 use the cargo-cyclonedx digest from finding 3 and expect the spec 0004 row to exist. If 0055 P1 has not landed, the integrator holds P5 and P6 or accepts a digest in the workflows with no row yet.
- **`xz` on the `ci-rust` image** is unverified. If absent, P5, P6 fail at extraction; the fix is an image change or a different asset, and the integrator decides.
- **The release-plz `releases` JSON field name for the tag** is [asserted]; P6 has a fallback.
- **Not run by me:** `cargo-cyclonedx` is not installed here, so I did not run `make sbom` with the flag; I ran no cargo test, because no code exists yet. The ansi_term control rows are the spec's own measurements (F5, F7) on 0.19.0 and 0.22.1.
- **Ticket pattern in a reason:** spec D2 says `<ticket>` without a pattern, so P4 only requires non-empty text. A stricter pattern (for example `CER-<n>`) is a spec amendment.
- **Spec findings:** F3 omits `fork-gate` from the `development` required checks (finding 9). D3 names "the operator gate for secrets" in `SECURITY.md`; P3 leaves it out as internal text. Neither changes a requirement.
