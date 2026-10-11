---
status: planned
created: 2026-10-11
tags: [dependabot, sbom, gitleaks, supply-chain, ci]
priority: medium
depends_on: [0004-dependency-and-licence-policy, 0034-tool-pinning-beyond-cargo, 0045-security-disclosure-and-advisory-policy, 0051-repository-guards, 0053-changelog-and-commit-text-policy]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0055-supply-chain-automation

> **Status**: planned, **Priority**: medium, **Created**: 2026-10-11

## Overview

Three automations guard what enters the tree: Dependabot proposes bumps, the SBOM job records
what shipped, and gitleaks blocks a secret. Each works today and none has a contract. Dependabot
opened four PRs on 2026-10-10 and three are still blocked (F3). The SBOM guard accepts an empty
file (F7). The two gitleaks scanners share a config and a version only by habit (F9, F10).

This spec owns the contract for all three. It cites and does not restate spec 0004 (pin tables
and `pin-check`, D3), spec 0034 (tool pins), spec 0045 (SBOM format and release attachment, D4),
spec 0051 (fail-closed hook, D7) and spec 0053 (Dependabot is exempt from the CHANGELOG line,
D6). the operator ruled on 2026-10-11 about 05:50Z that this spec is drafted [reported:
AskUserQuestion in lane w25:p1, label "Supply-chain automation"]. Content decisions are the
integrating lane's unless marked proposed.

### Facts

Read 2026-10-11 on ceres in worktree `/tmp/uas-w55`, first at `origin/development` ebc7be8, then
re-read at merge commit 65467c9 (origin/development merged in). The counts below held at both.

- **F1 [verified]:** `.github/dependabot.yml` (22 lines) has two entries, `cargo` and
  `github-actions`, each `directory: /`, `target-branch: development`, `interval: weekly`, and
  one group that takes only `minor` and `patch` (lines 3 to 22). It has no `ignore` key and no
  `major`. Command: `cat -n .github/dependabot.yml; grep -c -i major .github/dependabot.yml`
  (prints 0).
- **F2 [verified]:** the config has no entry for `crates/uas-ebpf/ebpf`, a separate workspace
  with its own `Cargo.lock` that `pin-check` reads (spec 0004 D3, `aya-ebpf` 0.2.1, line 193).
  Dependabot watches `/` only.
- **F3 [verified]:** `gh pr list -R unsigned-gg/uas --author app/dependabot --state all` lists
  four PRs, all created 2026-10-10 about 22:52Z, all into `development`. #122 (toml 0.8.23 to
  1.1.6, a major) merged at 23:58Z. #121 (tokio 1.53.1 to 1.53.2, group `cargo-minor-patch`),
  #123 (agent-client-protocol 2.2.0 to 3.1.0) and #124 (protox 0.9.1 to 0.10.0) are open and
  `BLOCKED`, each with `hygiene` red and no other check failed. Zero Dependabot PRs have
  opened since. Titles use `chore(deps)` on #121 and #122 and `build(deps)` on #123 and #124;
  both pass the 0051 subject regex.
- **F4 [verified]:** `hygiene` is red because of `pin-check`. Applying each PR diff
  (`gh pr diff N`) to a clean tree and running `cargo xtask pin-check` prints, for #121,
  `tokio pinned 1.53.1 (specs/0004), Cargo.lock has 1.53.2`; for #123, `agent-client-protocol
  pinned 2.2.0 ... Cargo.lock has 3.3.0`; for #124, `protox pinned 0.9.1 ... has 0.10.0`. The
  clean tree prints `ok (22 pinned crates checked)`. #123 asks for 3.1.0 in `Cargo.toml` and
  locks 3.3.0, so the title version and the locked version differ.
- **F5 [verified]:** the table already records two of the decisions. Spec 0004 line 252 says
  `protox` "0.10.0 not taken (0026 D3)". Line 200 says `agent-client-protocol` "3.x is under 14
  days old" and the manifest keeps `"2.2.0"` (spec 0016). Line 201 pins `tokio` 1.53.1 and
  `Cargo.toml` line 44 says `=1.53.1`. Line 198 records that #122's `toml` bump was ratified
  after the merge (the operator, 2026-10-11, "Ratify 1.1.8"). No `toml` row existed at the merge,
  0856f37 (`git show 0856f37:specs/0004-dependency-and-licence-policy.md | grep -c '^| .toml. |'`
  prints 0). `pin-check` compares a row with the lock as strings. Until build plan P2 it skipped
  the `toml` row, whose version carries build metadata (F13).
- **F6 [verified]:** `sbom.yml` runs on `push` to `main` and `workflow_dispatch` (lines 6 to 8),
  installs `cargo-cyclonedx` with no version (line 22), stages every `*.cdx.json` outside
  `target/`, guards with `test -n "$(ls -A sbom)"` (line 30) and uploads with
  `if-no-files-found: error` and `retention-days: 90` (lines 35 and 36). Format, version pin and
  release attachment belong to spec 0045 D4, REQ-0045-08 and REQ-0045-09.
- **F7 [verified]:** with cargo-cyclonedx 0.5.9 (installed 2026-10-11 into a scratch root:
  `cargo install cargo-cyclonedx --version 0.5.9 --locked --root <scratch>`), `cargo cyclonedx --format json --spec-version
  1.5` writes 11 files for the 11 workspace packages (`cargo metadata --no-deps`), 1.6 KB to 315
  KB each, `bomFormat` `CycloneDX`, `specVersion` `1.5`, 0 to 255 components (`uas-storage` has
  0; the other ten have 9 to 255). Without the flag the same command writes `1.3`. The
  workflow guard of F6 passes a directory holding one file that contains `{}` (Test row 8).
- **F8 [verified]:** `.gitleaks.toml` is 2 lines: `[extend]`, `useDefault = true`. It holds no
  custom rule and no allowlist. `git grep -n 'gitleaks:allow' -- . ':!specs'` prints nothing, and the tree has
  no `.gitleaksignore`.
- **F9 [verified]:** CI runs `gitleaks git . --redact --no-banner --verbose` over a full clone,
  version 8.30.1 and a sha256 in `ci.yml` lines 38 to 45. The hook runs `gitleaks protect
  --staged --redact --no-banner` and its missing-binary message names v8.30.1 (`lefthook.yml`
  lines 16 to 25; fail closed, spec 0051 D7). gitleaks reads `.gitleaks.toml` from the scan
  target when no `--config` is given (`gitleaks --help`, order of precedence item 4), and both
  commands run from the repository root, so both read the same file.
- **F10 [verified]:** nothing compares the two versions. The pin-check of spec 0004 reads
  Cargo lockfiles only, and spec 0034 (planned) has not built the tool parser. Spec 0004's CI
  tools table has no `gitleaks` or `cargo-cyclonedx` row. The workstation binary reports
  `version is set by build process` (`gitleaks version`), a source build, so the hook cannot
  compare versions without blocking such a binary.
- **F11 [verified]:** a random `ghp_` token planted in a scratch repo that holds a copy of
  `.gitleaks.toml` is reported by both scanners with rule `github-pat`, exit 1 each; the clean
  repo exits 0 (Test rows 11 to 14).
- **F12 [reported]:** `SECURITY.md` line 27 and spec 0045 D3 say a secret found in the repo or
  its history is compromised and is rotated by its owner. AGENTS.md line 75 puts secrets behind
  the operator. Not re-read from the estate rule file here.
- **F13 [verified]:** `pin-check` compares the table cell and the locked version as strings. At
  57e0bc7 it reads the `toml` row: it prints `ok (23 pinned crates checked)`, and with the cell
  set to `1.1.9+spec-1.1.0` it exits 1 and names `toml` (2026-10-11, xtask built at 57e0bc7).
  Before build plan P2 (`engineering/plans/0055-supply-chain-automation.md`, PR #204) it read
  digits-only cells, skipped that row and printed `ok (22 pinned crates checked)` after the same
  edit [reported: unsigned-gg/uas@57e0bc7:engineering/plans/0055-supply-chain-automation.md,
  finding 2].
- **F14 [verified]:** `cargo xtask` runs in the root workspace, so cargo re-resolves a planted
  lock before `pin-check` reads it. With the `tokio` entry of `Cargo.lock` set to `9.9.9`,
  `cargo xtask pin-check` rewrote it to 1.53.1 and exited 0. The xtask binary built before the
  edit exited 1 and named `tokio`. `cargo info tokio@9.9.9` finds no such version (2026-10-11,
  at 57e0bc7).

## Design

### Decisions

Classes: `ruled` (the operator decided it), `lane-decided` (the integrating lane decided it on
2026-10-11; the operator may override), `proposed` (this spec proposes it; the integrating lane or
the operator rules).

| # | Decision | Why | Class |
|---|---|---|---|
| D1 | Dependabot cadence is weekly for both ecosystems, as configured. Version updates open against `development` only (spec 0003). | F1 holds; F3 shows the first run opened four PRs at once and none since, so weekly is not noisy. | lane-decided: keeps the shipped config |
| D2 | Minor and patch updates are grouped, one PR per ecosystem per run. A major update is never grouped and gets its own PR. | F1: the group takes `minor` and `patch` only, so a major falls outside it. #122 to #124 are the three majors and each opened alone (F3). | lane-decided: keeps the shipped config |
| D3 | A Dependabot PR that moves a crate named in a spec 0004 pin table is refused by `hygiene` until the same PR carries the table edit. The refusal is `pin-check` (0004 D3), a required check. No new gate is added. | F4: all three open PRs fail exactly this way today. F13: it compares strings, and until build plan P2 it skipped the `toml` row. | lane-decided: follows 0004 D3 |
| D4 | The integrating lane makes the edit: one signed commit on the Dependabot branch that sets the row version and adds the licence read and the date (spec 0004 licence rules). A Dependabot PR is never merged with a red `hygiene`. | A human edit on the branch stops Dependabot rewriting it [asserted: GitHub docs, not read here]. The row needs a licence read that only a person can sign. | lane-decided |
| D5 | A major bump of a pinned crate is a reviewed upgrade. Before merge the row records the licence, the MSRV result of `make toolchain-check`, and a ruling when the row cites a prior decision. | F5: #122 merged a major before any row existed and needed a ratification afterwards. | lane-decided |
| D6 | The three open PRs. #121 (tokio patch): take it under D4, row to 1.53.2. #123 and #124: close with a comment that cites 0004 lines 200 and 252 and ends with `@dependabot ignore this major version`, so they do not reopen. | F5: both bumps are recorded as not taken. #123's own title and lock disagree (F4). | lane-decided: integrating lane w25:p1, 2026-10-11; executed as a plan package, not by this spec |
| D7 | Add a `cargo` entry for `/crates/uas-ebpf/ebpf`, same cadence, target branch and group. Its bumps pass the same D3 and D4. | F2: that lockfile is pin-checked yet unwatched. | lane-decided: integrating lane w25:p1, 2026-10-11 |
| D8 | SBOM format, spec version, tool pin, 90-day artifact and release attachment are as ruled in spec 0045 D4. This spec adds no format decision. | One owner per contract (F6). | lane-decided, citing 0045 D4 (that row cites the lane brief of 2026-10-10, not a ruling) |
| D9 | The SBOM artifact is produced on `push` to `main` and on manual dispatch, as today. A pull request does not upload. | An upload per PR costs storage and a runner for no reader (F6). | lane-decided: keeps the shipped trigger |
| D10 | `sbom-check` fails the job when the staging directory is empty, the file count differs from the workspace package count, a file is zero bytes or lacks `bomFormat` `CycloneDX` and `specVersion` `1.5`, or the files hold no component in total. It replaces the `test -n` line and runs before the upload. A per-file component floor is not used, because `uas-storage` has none (F7). | F7 and Test row 8: the shipped guard accepts `{}`. | lane-decided |
| D11 | A PR that changes `Cargo.toml`, `Cargo.lock`, the `Makefile` or `sbom.yml` also runs generation and `sbom-check`, with no upload and a fork guard like `ci.yml` line 28. It starts non-required. | A Dependabot bump that breaks the SBOM would otherwise surface only after merge to `main`. | lane-decided: integrating lane w25:p1, 2026-10-11 |
| D12 | The gitleaks rule set is the tool default: `useDefault = true` and no custom rule. | F8: the file holds nothing else, and a custom rule is code that nobody has read. | lane-decided: keeps the shipped config |
| D13 | An allowlist entry is an `[[allowlists]]` table in `.gitleaks.toml` with a `description` that names a spec or ticket. It uses `regexes` or `paths` narrow enough to match one finding. There is no inline `gitleaks:allow` and no `.gitleaksignore`. A live secret is never allowlisted. | One reviewable file, shown in a PR diff. F8: there is no allowlist today, so this sets the entry rule before the first one. | lane-decided |
| D14 | The CI job and the hook are the same scanner: one version, one config. Spec 0004's CI tools table is the single owner of the version, as spec 0034 REQ-0034-06 and REQ-0034-07 rule, and P2 adds the `gitleaks` row (0034 plan). `make pin-check` fails when `GITLEAKS_VERSION` in `ci.yml` or the version in the `lefthook.yml` message differs from that row. The hook does not compare the workstation binary (F10); CI is authoritative. | F9 and F10: parity is now by habit. | lane-decided; the hook stays fail closed on a missing binary (0051 D7) |
| D15 | A hit means stop. The secret is compromised: the owner rotates it through the operator gate, and a history rewrite does not clear it. The author does not allowlist it and does not bypass the hook with `LEFTHOOK=0`. A false positive goes through D13. | F12. `LEFTHOOK=0` is a bypass of the hook, not of CI. | lane-decided: cites 0045 D3 and the estate rule |

### Requirements (EARS)

- **[REQ-0055-01]** (Ubiquitous): The `dependabot.yml` file shall declare the `cargo` and
  `github-actions` ecosystems, each with `interval: weekly` and `target-branch: development`
  (D1).
- **[REQ-0055-02]** (Ubiquitous): The `dependabot.yml` file shall group `minor` and `patch`
  updates per ecosystem and shall name no `major` in any group (D2).
- **[REQ-0055-03]** (Unwanted behavior): IF a Dependabot PR moves a crate that a spec 0004 pin
  table names and the table row does not carry the new version, THEN `make pin-check` shall
  exit non-zero and `hygiene` shall block the merge (D3).
- **[REQ-0055-04]** (Event-driven): WHEN the integrating lane accepts a bump of a pinned crate,
  it shall push one signed commit to the Dependabot branch that sets the table row, the licence
  read and the date, and `pin-check` shall then exit zero (D4).
- **[REQ-0055-05]** (Event-driven): WHEN a major bump of a pinned crate is proposed, the lane
  shall not merge it until its table row records the licence, the `toolchain-check` result and,
  where the row cites a prior decision, a ruling (D5).
- **[REQ-0055-06]** (Optional feature): WHERE a workspace outside the root has its own
  `Cargo.lock` that `pin-check` reads, `dependabot.yml` shall carry a `cargo` entry for that
  directory (D7).
- **[REQ-0055-07]** (Event-driven): WHEN a commit lands on `main` or the workflow is
  dispatched, the `sbom` workflow shall generate the SBOM and upload it, and no other trigger
  shall upload (D9, D8).
- **[REQ-0055-08]** (Unwanted behavior): IF the SBOM staging directory is empty, or a file in
  it is zero bytes, is not valid JSON, or lacks `bomFormat` `CycloneDX` or `specVersion`
  `1.5`, THEN `sbom-check` shall exit non-zero and name the file (D10).
- **[REQ-0055-09]** (Unwanted behavior): IF the count of `*.cdx.json` files differs from the
  `cargo metadata --no-deps` package count, or the files hold no component in total, THEN
  `sbom-check` shall exit non-zero (D10).
- **[REQ-0055-10]** (State-driven): WHILE `sbom-check` exits non-zero, the `sbom` job shall run
  no upload step (D10).
- **[REQ-0055-11]** (Optional feature): WHERE a PR changes `Cargo.toml`, `Cargo.lock`, the
  `Makefile` or `sbom.yml`, the `sbom` workflow shall generate the SBOM and run `sbom-check`
  without upload and shall not run on a fork (D11).
- **[REQ-0055-12]** (Ubiquitous): The `.gitleaks.toml` file shall extend the default rule set
  and shall define no custom rule (D12).
- **[REQ-0055-13]** (Ubiquitous): The allowlist in `.gitleaks.toml` shall hold only entries that are an
  `[[allowlists]]` table whose `description` names a spec or ticket, and the repository shall
  hold no `gitleaks:allow` comment and no `.gitleaksignore` (D13).
- **[REQ-0055-14]** (Unwanted behavior): IF the gitleaks version in `lefthook.yml` or
  `GITLEAKS_VERSION` in `ci.yml` differs from the `gitleaks` row of the spec 0004 CI tools
  table, THEN `make pin-check` shall exit non-zero and name the file and both values (D14).
- **[REQ-0055-15]** (Unwanted behavior): IF a secret is staged or committed, THEN the hook and
  the CI `secrets` job shall each exit non-zero and, for the same secret, report the same rule
  id (D14).
- **[REQ-0055-16]** (Event-driven): WHEN a scan reports a real secret, the author shall stop,
  treat the secret as compromised, hand rotation to its owner through the operator gate, and
  leave the finding out of every allowlist (D15).

## Plan

Order: P1 to P3 need no ruling. P4 and P5 carry out D6, D7 and D11. No item here edits CHANGELOG.md
or a config in this commit.

- [ ] P1, `make sbom-check` and a script it runs (`scripts/sbom-check.sh`): REQ-08, 09. Red
      first: Test rows 5 to 10. `sbom.yml` calls it in place of `test -n` and before the upload
      (REQ-10). The cargo-cyclonedx pin and `--spec-version 1.5` are spec 0045 plan items.
- [ ] P2, spec 0004 CI tools table: rows for `gitleaks` 8.30.1 (sha256 from `ci.yml` line 39)
      and `cargo-cyclonedx` (0045 plan), then the comparison of `lefthook.yml` and `ci.yml` against that row in
      `xtask/src/pin_check.rs` (REQ-14; Test rows 15, 16 compare the two files until the row exists). This is spec 0034's parser work.
- [ ] P3, `.gitleaks.toml` comment that cites D13 and spec 0055 above `[extend]` (REQ-12, 13).
- [ ] P4, the integrating lane applies D6: the D4 commit on #121; close #123 and
      #124 with the comment. These are outward actions on GitHub, so they wait for the lane.
- [ ] P5, `dependabot.yml`: the D7 entry. `sbom.yml`: the D11 pull request leg, non-required
      first, with the fork guard (REQ-06, 11).

## Test

The block runs from the repo root today. It builds a scratch worktree and a scratch git repo,
changes no tracked file, and needs `gitleaks`, `cargo-cyclonedx` 0.5.9 and `jq` on PATH. A
missing tool exits 1. Each row prints `ok` only on a positive assertion. Rows 2, 5 to 7, 9, 10, 12, 13
and 16 pass only when the bad input is rejected, so a check that accepts everything turns them
red. The plant for row 2 moves the `tokio` entry of `Cargo.lock` with no table edit, the shape
of PR #121. It uses `9.9.9`, a version no release has, so rows 2 and 3 hold whatever `tokio`
the tree pins and need no network. `pc` runs the xtask binary built before the plant, because
`cargo xtask` would re-resolve the planted lock first (F14). The `sc` function is the reference
for `sbom-check` (P1).

Install the tools first (F7): `cargo install cargo-cyclonedx --version 0.5.9 --locked`;
`gitleaks` from its release page, at the version in the spec 0004 CI tools row; `jq` from the
system package manager.

```bash
for t in gitleaks cargo-cyclonedx jq; do command -v $t >/dev/null || { echo "missing $t"; exit 1; }; done
b=origin/development; s=$(mktemp -d); g=$(mktemp -d); git worktree add -q --detach "$s" "$b"; trap 'git worktree remove --force "$s"; rm -rf "$g"' EXIT
r=0; want() { [ "$2" = "$3" ] && echo "row $1: ok ($3)" || { echo "row $1: RED got $2 want $3"; r=1; }; }
rc() { "$@" >/dev/null 2>&1; echo $?; }
(cd "$s" && cargo build -q -j 2 -p xtask); x=$(cd "$s" && cargo metadata --no-deps --format-version 1 | jq -r .target_directory)/debug/xtask
pc() { "$x" pin-check; }
want 1 "$(rc pc)" 0
sed -i '/^name = "tokio"$/{n;s/^version = ".*"/version = "9.9.9"/}' "$s/Cargo.lock"
want 2 "$(rc pc)" 1
sed -i 's/^\(| `tokio` | \)[0-9.]*/\19.9.9/' "$s/specs/0004-dependency-and-licence-policy.md"
want 3 "$(rc pc)" 0; git -C "$s" checkout -q -- Cargo.lock specs/0004-dependency-and-licence-policy.md
sc() { n=$(ls "$1" 2>/dev/null | grep -c 'cdx\.json$'); [ "$n" -eq "$2" ] || { echo "sbom-check: count $n want $2: $1"; return 1; }; for f in "$1"/*.cdx.json; do [ -s "$f" ] && jq -e '.bomFormat=="CycloneDX" and .specVersion=="1.5"' "$f" >/dev/null || { echo "sbom-check: invalid: $f"; return 1; }; done; [ "$(cat "$1"/*.cdx.json | jq -s '[.[].components|length]|add')" -gt 0 ] || { echo "sbom-check: no components: $1"; return 1; }; echo "sbom-check: ok"; }
ro() { o=$("$@" 2>&1); echo "$? ${o//$g\//}"; }
(cd "$s" && cargo cyclonedx --format json --spec-version 1.5 >/dev/null 2>&1); n=$(cd "$s" && cargo metadata --no-deps --format-version 1 | jq '.packages|length')
mkdir "$g/ok" "$g/empty" "$g/zero" "$g/obj" "$g/bad"; find "$s" -name '*.cdx.json' -not -path '*/target/*' -exec cp {} "$g/ok/" \;
want 4 "$(ro sc "$g/ok" "$n")" "0 sbom-check: ok"
want 5 "$(ro sc "$g/empty" "$n")" "1 sbom-check: count 0 want $n: empty"
: > "$g/zero/a.cdx.json"; want 6 "$(ro sc "$g/zero" 1)" "1 sbom-check: invalid: zero/a.cdx.json"
echo '{}' > "$g/obj/a.cdx.json"; want 7 "$(ro sc "$g/obj" 1)" "1 sbom-check: invalid: obj/a.cdx.json"
want 8 "$([ -n "$(ls -A "$g/obj")" ] && echo old-guard-passes)" old-guard-passes
cp "$g/ok"/* "$g/bad/"; rm "$g/bad/uas-cli.cdx.json"; want 9 "$(ro sc "$g/bad" "$n")" "1 sbom-check: count $((n-1)) want $n: bad"
cp "$g/ok"/* "$g/bad/"; jq '.specVersion="1.3"' "$g/ok/uas-cli.cdx.json" > "$g/bad/uas-cli.cdx.json"; want 10 "$(ro sc "$g/bad" "$n")" "1 sbom-check: invalid: bad/uas-cli.cdx.json"
d=$g/repo; mkdir "$d"; cp .gitleaks.toml "$d/"; git -C "$d" init -q; git -C "$d" add .; git -C "$d" -c user.name=t -c user.email=t@t commit -qm base
want 11 "$(cd "$d" && rc gitleaks git . --redact --no-banner)" 0
printf 'token = "ghp_%s"\n' "$(tr -dc A-Za-z0-9 </dev/urandom | head -c36)" > "$d/leak.txt"; git -C "$d" add leak.txt
hook=$(cd "$d" && gitleaks protect --staged --redact --no-banner -f json -r - 2>/dev/null | jq -r '.[0].RuleID'); want 12 "$(cd "$d" && rc gitleaks protect --staged --redact --no-banner)" 1
git -C "$d" -c user.name=t -c user.email=t@t commit -qm leak
ci=$(cd "$d" && gitleaks git . --redact --no-banner -f json -r - 2>/dev/null | jq -r '.[0].RuleID'); want 13 "$(cd "$d" && rc gitleaks git . --redact --no-banner)" 1
want 14 "$([ -n "$ci" ] && [ "$ci" = "$hook" ] && echo "same:$ci")" "same:github-pat"
cv=$(sed -n 's/^ *GITLEAKS_VERSION: //p' .github/workflows/ci.yml); lv=$(sed -n 's/.*install v\([0-9.]*\) .*/\1/p' lefthook.yml)
pv() { [ -n "$1" ] && [ "$1" = "$2" ] && echo "ok $1" || echo "gitleaks mismatch: ci.yml=$1 lefthook.yml=$2"; }
want 15 "$(pv "$cv" "$lv")" "ok 8.30.1"
want 16 "$(pv "$cv" "$(echo "$lv" | sed 's/30/31/')")" "gitleaks mismatch: ci.yml=8.30.1 lefthook.yml=8.31.1"
want 17 "$(grep -c 'target-branch: development' .github/dependabot.yml)$(grep -c 'interval: weekly' .github/dependabot.yml)$(grep -c -i major .github/dependabot.yml)" 330
exit $r
```

Row 8 shows the shipped guard accepting a `{}` file; rows 6, 7 and 9 to 10 show `sc` refusing it.
Row 4 is the control for them: the real SBOM passes, so a validator that fails everything turns
it red. Row 3 is the control for row 2. Rows 12 and 13 need the plant, and row 11 is their
clean control. Row 16 proves the comparison of row 15 can see a mismatch. The xtask and script of
P1 and P2 replace the shell and become their unit tests.

| REQ | Row or check |
|---|---|
| 01, 02 | 17 (counts 3, 3, 0 for the three entries; a `major` in a group raises the third) |
| 03, 04 | 2, 3 (row 1 is the clean control) |
| 05 | manual, reviewer: the row of the PR carries the licence, the MSRV result and the ruling |
| 06 | green since PR #204 (wave 1 P3): `grep -c 'uas-ebpf/ebpf' .github/dependabot.yml` prints 1 |
| 07, 11 | manual, reviewer, until P5: `grep -n -A4 '^on:' .github/workflows/sbom.yml` |
| 08, 09 | 4 to 10 |
| 10 | manual, reviewer: the `sbom-check` step precedes `upload-artifact` in `sbom.yml` |
| 12, 13 | `git grep -c 'gitleaks:allow' -- . ':!specs'` prints nothing; `ls .gitleaksignore` fails |
| 14 | 15, 16 assert the message names both values; they compare the two files. Green since PR #204 (wave 1 P1) for the 0004 leg: `grep -c '^| .gitleaks. |' specs/0004-dependency-and-licence-policy.md` prints 1; green after P2 |
| 15 | 11 to 14 |
| 16 | manual, lane: a hit stops the author and goes to the operator gate (D15) |

## Notes

- The ruling that this spec is drafted: the operator, lane w25:p1, 2026-10-11 about 05:50Z, by
  AskUserQuestion, label "Supply-chain automation" [reported: lane brief].
- Overlap kept out: spec 0045 owns the SBOM format and release attachment and the advisory
  gate. Spec 0051 owns the fail-closed hook (D7). Spec 0034 owns the tool pin tables. Spec 0053
  D6 exempts a `dependabot/` head ref from the CHANGELOG line, and a D4 commit on that branch
  keeps the exemption because the head ref does not change.
- Cross-reference: spec 0034 REQ-0034-06 and REQ-0034-07 own the general tool parser of
  `pin-check`. D14 is its narrow `gitleaks` slice and does not replace it. The
  `cargo-cyclonedx` row of the spec 0004 CI tools table is also a spec 0045 plan item. Build
  plan P1 wrote it in PR #204, because the row was absent.
- Caveat: F3's subject prefix differs between PRs (`chore(deps)` and `build(deps)`). Both pass
  the 0051 regex, so the spec sets none. A `commit-message` prefix in `dependabot.yml` would
  settle it and is not proposed.
- Caveat: D4 relies on a GitHub behavior [asserted] that Dependabot stops rewriting a branch
  once a person pushes to it. If it still rebases over the table edit, the fallback is to close
  the PR and open the bump from a lane branch.
- Parked: Dependabot security updates (advisory-driven PRs) are a repository setting, not a
  file, and spec 0045 owns the advisory gate. Not decided here.
- Parked: the SBOM does not carry the toolchain, `bpf-linker` or the actions. It lists crates.
