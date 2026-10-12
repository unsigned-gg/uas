---
status: in-progress
created: 2026-10-11
tags: [changelog, commits, pull-request, release, ci]
priority: medium
depends_on: [0003-branch-model-and-promotion, 0005-ci-cache-and-runners, 0048-licence-and-publication]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0053-changelog-and-commit-text-policy

> **Status**: in-progress, **Priority**: medium, **Created**: 2026-10-11

## Overview

`AGENTS.md` rules 6 and 8 state three text rules: one CHANGELOG line per user-visible PR, no
ticket ID in a commit subject, and `Part of CER-N` in the PR body. Spec 0005 (item 5) and spec
0048 D13 repeat the CHANGELOG rule. No check enforces any of them, and no spec owns their
detail. This spec owns the contract and adds one check, `text-check`, that runs the contract.

The practice is mostly good, so the check mostly confirms it. The CHANGELOG rule holds in 25 of
28 recent code PRs (F2). The ticket-ID rule holds in all recent subjects (F6). The PR template
is used by none of PRs #130 to #159 (F7), so the body checks start advisory (0053 D9).

Spec 0051 owns the commit-subject regex and lefthook. This spec does not restate it. It adds
what that regex cannot see: a ticket ID after a valid type (F8).

### Facts

Measured 2026-10-11 on ceres at `origin/development` 3e60a55, worktree `/tmp/uas-w53`. The base
is pinned: development has moved since, and the counts below are for 3e60a55.

- **F1 [verified]:** `[Unreleased]` spans lines 8 to 118 and holds seven headings: Added,
  Changed, Fixed, Changed, Added, Changed, Fixed. Command: `awk '/^## \[0/{exit} /^## \[Unreleased\]/{u=1}
  u&&/^### /{print NR": "$0}' CHANGELOG.md`. Keep a Changelog 1.1.0 allows each heading once.
- **F2 [verified]:** of the last 60 first-parent merges into development, 28 changed a file
  outside `docs/`, `specs/`, `engineering/` and `*.md`; 25 of those also changed CHANGELOG.md.
  The three that did not: a dependabot bump (#122), PR #130, which changed only `Cargo.lock`
  and `Cargo.toml` (it adds features, so it is a real miss), and an old cerebral-work PR (#108). Command: the loop in Test row 0, `git diff --name-only M^1 M`.
- **F3 [verified]:** 39 of the last 40 commits touching CHANGELOG.md are by the integrating
  lane's author; 25 of the 40 have the subject shape `docs(changelog): ...`. Command:
  `git log --format='%an %s' -40 -- CHANGELOG.md`. The integrating lane commits the line
  in the PR, which is the shape spec 0050 D6 rules and D2 keeps. A builder writes none.
- **F4 [verified]:** `release-plz.toml` line 7 sets `changelog_update = false`, so release-plz
  never writes CHANGELOG.md. The roll is `make release-notes` (Makefile lines 109 to 120): an
  awk that prints `## [Unreleased]`, a blank line, then `## [x.y.z] - date`. No released line is
  rewritten. The header comment of `release.yml` line 3 says the roll happens before release-plz.
  Command: `grep -n changelog_update release-plz.toml; sed -n 109,120p Makefile`.
- **F5 [verified]:** no workflow reads CHANGELOG.md: `grep -rn -i changelog .github/workflows`
  prints one comment line (release.yml line 3). The rule is unenforced today.
- **F6 [verified]:** `git log --no-merges --format=%s -300 3e60a55 | grep -cE
  '\b(CER|OPS|EST|SEC|ONB|TOD|RD|BIZ|BRAND|VES)-[0-9]+\b'` prints 1 (70b34b1, spec 0025); over all history it prints 2.
  GitHub merge subjects can carry a branch name with an ID (4dfd100), which no author chose.
- **F7 [verified]:** over merged PRs #130 to #159 (a fixed window; 29 PRs): 0 bodies hold
  `## Problem` and 0 hold `Part of CER` or `Part of spec`. Command: `gh pr list -R unsigned-gg/uas
  --state merged --limit 200 --json number,body`, filtered to numbers 130 to 159 with `test()`
  on each pattern. In the 300 commit messages up to 3e60a55, 44 hold a `Part of` line
  (`git log -300 --format=%B 3e60a55 | grep -c '^Part of'`).
- **F8 [verified]:** the lefthook regex (`lefthook.yml` line 10) accepts `feat(uas): CER-123 add
  x` and rejects `CER-123: x` only because `CER-123` is not a type. Test row 10 runs it. So a
  ticket ID after a valid type passes today.
- **F9 [verified]:** `.github/pull_request_template.md` lines 1 to 25 hold five sections
  in order: Problem, What this does, Verified, Gated steps after merge, Rollback. `Part of CER-N`
  is line 28. `CONTRIBUTING.md` lines 33 to 41 repeat the rules.
- **F10 [verified]:** `CHANGELOG.md` holds no conflict marker line today (`grep -cE
  '^(<<<<<<<|=======|>>>>>>>|\|\|\|\|\|\|\|)' CHANGELOG.md` prints 0). Commit 61bf3cd had to drop
  two stale diff3 base markers a merge resolution left, so the failure has happened.
- **F12 [reported]:** the Linear team keys are CER, OPS, EST, SEC, ONB, TOD, RD, BIZ, BRAND and
  VES (integrating lane w25:p1 brief, 2026-10-11, from the lane's tooling). Not re-read from
  Linear here. A key added later is not in the D7 pattern until the constant is edited.
- **F13 [verified, 2026-10-11, plan 0053 finding 1]:** a release-roll PR fails the check as D6 first read. After
  `make release-notes VERSION=9.9.9` on a scratch worktree, `git status --porcelain` lists CHANGELOG.md,
  Cargo.lock and Cargo.toml, `[Unreleased]` is empty, and the Test block's `verdict` prints `FAIL-nolog`.
- **F14 [verified, 2026-10-11, plan 0053 finding 2]:** D6 clashed with itself on agent files. A PR that changes
  only `.claude/agents/uas-planner.md` printed `PASS`, because the `.md` exemption matched first.
- **F11 [verified]:** `xtask/src/main.rs` lines 26 to 29 route `spec-check`, `toolchain-check` and
  `pin-check`. The CI `hygiene` job runs `make hygiene` on push and pull request.

## Design

### Decisions

Classes: `ruled` (the operator decided it), `lane-decided` (the integrating lane decided it on
2026-10-11 and the operator may override), `proposed: the operator` (a decision only the operator can take). The operator ruled on 2026-10-11 about 03:10Z (AskUserQuestion,
lane w25:p1, "CHANGELOG and commit/PR text policy") that this spec is drafted.

| # | Decision | Why | Class |
|---|---|---|---|
| D1 | One bullet per user-visible PR, under `## [Unreleased]`, under one heading of D3. A wave PR gets one bullet for the wave. The bullet is one paragraph with no hard wrap and no nested list. It names the spec and plan package when there is one. No length cap: the practice runs to several hundred characters. | Spec 0048 D13 ruled the one-line-per-PR rule. The wave bullets in F1 are the practice. | ruled: 0048 D13 (the operator, 2026-10-11 about 02:35Z); wave and shape lane-decided |
| D2 | The integrating lane writes the line, as a `docs(changelog)` commit inside the PR before it is marked ready. A builder writes none. A human author writes their own. | One writer cannot conflict with itself (0050 D6). F3 shows this is the practice. | lane-decided (0050 D6) |
| D3 | `[Unreleased]` may carry the Keep a Changelog 1.1.0 headings, each at most once, in this order: Added, Changed, Deprecated, Removed, Fixed, Security. A bullet goes under the heading that fits it. Deprecated sits after Changed and before Fixed; a roll that empties the section recreates it there. The heading rule fails a PR only when the merge base's headings were valid, so a branch cut before the cleanup is not failed for old duplicates (amended 2026-10-11, plan 0053 finding 6). Cleaning F1 is a Plan item, not part of this commit. | F1 has duplicates; the format forbids none but the roll reads cleaner without them. | lane-decided |
| D4 | A released section is never edited. Enforcement is the check, not release-plz: release-plz writes no CHANGELOG (F4). Base means the merge base of `--base` and `--head`, never the tip of `--base`, so a roll that merged after the branch was cut does not fail the PR (amended 2026-10-11, plan 0053 finding 5). The test: every line of base from its first `## [x.y.z]` heading to the end shall be a suffix of head. The roll inserts above that point, so a roll PR passes; an edit inside it fails. | `changelog_update = false` means the tool offers no protection. A suffix test needs no knowledge of which PR is a roll. | lane-decided |
| D5 | The check is `cargo xtask text-check --base <ref>` (diffed from the merge base of `--base` and `--head`), with `make text-check`, and a CI job `text-check` on `pull_request` only. It uses `std` and the `git` binary, no new crate, so no spec 0004 pin. The job ships non-required. Making it required is a branch-protection change, which D10 rules. | A Rust xtask matches `spec-check` (F11). A job in `hygiene` would run on push without a base. | lane-decided: integrating lane w25:p1, 2026-10-11; required status: ruled (D10) |
| D6 | Tested in this order. (1) A path under `.claude/` or `.agents/` is never exempt, even a `.md` file: agent files are user-visible to the lanes (the 2026-10-10 repo-agents bullet). This rule wins over the Markdown exemption. (2) The CHANGELOG-line rule applies to a PR that changes a file outside this exempt list (a PR that changes only `Cargo.toml` or `Cargo.lock` is not exempt, except under 3): `docs/**`, `specs/**`, `engineering/**`, any `*.md` other than CHANGELOG.md, and `**/tests/**`. A PR whose head ref begins `dependabot/` is exempt. Any other PR must add a line to `[Unreleased]`: a line of head's section that base's lacks. (3) Release-roll exemption: a PR is exempt from the line when every changed file is a `Cargo.toml` (any depth), `Cargo.lock` or `CHANGELOG.md`, head's `[Unreleased]` holds no bullet, and head's CHANGELOG.md has a `## [x.y.z]` heading directly after `## [Unreleased]` that base lacks. That is the shape `make release-notes` writes (F13). The D4 suffix test still runs on it. | F2: 25 of 28. One miss is exempt (dependabot); #130 and the old PR are real misses. A manifest-only PR is not exempt, because a feature or dependency change is user-visible. F13: without 3 every release PR fails. A code PR that fakes the CHANGELOG shape changes a file outside the set, so it still fails. F14: the precedence in 1. | lane-decided: integrating lane w25:p1, 2026-10-11; amended 2026-10-11 from plan 0053 findings 1 and 2 |
| D7 | A non-merge, non-revert commit subject in `base..head` shall match no ticket pattern: `\b(CER|OPS|EST|SEC|ONB|TOD|RD|BIZ|BRAND|VES)-[0-9]+\b`. The ID goes in the PR body as a line `Part of CER-N` or `Part of spec NNNN`. The check reads `git log`, so it also catches a commit made with `LEFTHOOK=0`. The prefix list is the team keys in F12 and is edited in one constant. Spec 0051 owns the type regex (0051 D4); this rule adds only the ID. | F8: the regex cannot see it. F6: one violation in 300. | lane-decided: integrating lane w25:p1, 2026-10-11; the key list is reported (F12) |
| D8 | A CHANGELOG conflict is resolved by keeping every bullet from both sides and dropping the markers, including diff3 base sections (`|||||||`). Order within a heading does not matter. The check fails on any marker line. Fewer conflicts come from D2. | F10. Both-sides is lossless for a list of independent bullets. | lane-decided (0050 D6 reduces the rate) |
| D9 | The PR body contract is the five template sections of F9 in order, and one `Part of` line. The check reports a missing section or line. It is advisory for now: it prints `warn:` and exits 0. | F7: 0 of 29 bodies use the template, so a hard fail would fail every PR on day one. | lane-decided: integrating lane w25:p1, 2026-10-11 |
| D10 | Required status follows a date rule: `text-check` lands non-required; after seven days with no false failure the lane adds it to the required checks on `development`. The count is seven required checks on `development` once `text-check` joins: six live contexts today (`secrets`, `rust / cargo`, `cargo-deny`, `hygiene`, `ebpf-live`, `fork-gate`; `gh api repos/unsigned-gg/uas/branches/development/protection --jq .required_status_checks.contexts`, 2026-10-11, spec 0003 table), plus `text-check`. `fork-gate` became required on 2026-10-11 (the operator, lane w25:p1, about 05:25Z; spec 0051 D8). The earlier text said six (amended 2026-10-11, plan 0053 finding 8); the six is now the live count and the seventh is `text-check`. The body checks (D9) stay advisory until the lane rules they fail; this spec sets no date for that. | A required check with a false failure blocks every PR, so a week of runs comes first. | ruled: the operator, lane w25:p1, 2026-10-11 about 03:40Z (AskUserQuestion), "Required once it runs green for a week"; the body-check flip is lane-decided |
| D11 | `RELEASING.md` (0048 D13, REQ-0048-15) links to this spec for the CHANGELOG rule and does not copy it. `AGENTS.md` rules 6 and 8 and `CONTRIBUTING.md` line 35 get a pointer here (Plan). Spec 0005 item 5 stays as the release-flow statement. | One owner for the contract text keeps four copies from drifting. | lane-decided |

### Requirements (EARS)

- **[REQ-0053-01]** (Ubiquitous): The repository shall carry one bullet in `## [Unreleased]` of
  CHANGELOG.md for each user-visible PR, under a heading of the D3 set (0053 D1, 0053 D3).
- **[REQ-0053-02]** (Ubiquitous): The `## [Unreleased]` section shall hold each of the headings Added,
  Changed, Deprecated, Removed, Fixed and Security at most once, in that order (0053 D3), and `text-check` shall fail a PR for this only when the merge base's headings met it.
- **[REQ-0053-03]** (Event-driven): WHEN a PR changes a file outside the exempt list of 0053 D6,
  `text-check` shall fail unless head's `[Unreleased]` section holds a line that base's lacks.
- **[REQ-0053-04]** (Optional feature): WHERE the PR head ref begins `dependabot/`, or every
  changed file is on the exempt list, `text-check` shall not require a CHANGELOG line.
- **[REQ-0053-05]** (Unwanted behavior): IF any line of base from its first released heading to
  the end is not an unchanged suffix of head's CHANGELOG.md, THEN `text-check` shall fail and
  name the first differing line (0053 D4).
- **[REQ-0053-06]** (Event-driven): WHEN `make release-notes` runs, it shall insert the new
  version heading directly under `## [Unreleased]` and rewrite no released line (0053 D4).
- **[REQ-0053-07]** (Ubiquitous): The `release-plz.toml` file shall keep `changelog_update = false`, so
  no tool other than `make release-notes` writes CHANGELOG.md (0053 D4).
- **[REQ-0053-08]** (Unwanted behavior): IF CHANGELOG.md holds a line that starts with
  `<<<<<<<`, `=======`, `>>>>>>>` or `|||||||`, THEN `text-check` shall fail (0053 D8).
- **[REQ-0053-09]** (Event-driven): WHEN a CHANGELOG conflict is resolved, the resolver shall
  keep every bullet from both sides and no marker line (0053 D8).
- **[REQ-0053-10]** (Ubiquitous): The commit subject shall carry no ticket ID; the PR body shall
  carry a line `Part of CER-N` or `Part of spec NNNN` (0053 D7).
- **[REQ-0053-11]** (Unwanted behavior): IF a non-merge, non-revert subject in `base..head`
  matches the ticket pattern of 0053 D7, THEN `text-check` shall fail and print the SHA.
- **[REQ-0053-12]** (State-driven): WHILE 0053 D10 has not turned the body checks to failing, `text-check` shall print
  `warn:` for a PR body that lacks a template section or the `Part of` line and shall exit 0.
- **[REQ-0053-13]** (Ubiquitous): The integrating lane shall write the CHANGELOG line as a
  `docs(changelog)` commit in the PR before it is marked ready (0053 D2, 0050 D6).
- **[REQ-0053-14]** (Ubiquitous): The `text-check` job shall read the PR body from a file the workflow
  writes from an environment variable, never from text interpolated into a shell command.
- **[REQ-0053-15]** (Ubiquitous): The `RELEASING.md` file shall link to this spec for the CHANGELOG
  rule and shall not restate it (0053 D11, 0048 D13).

- **[REQ-0053-16]** (Event-driven): WHEN `text-check` has run seven days on pull requests into
  `development` with no false failure, the integrating lane shall add it to the required checks
  (0053 D10).
- **[REQ-0053-17]** (Optional feature): WHERE every changed file is a `Cargo.toml`, `Cargo.lock` or
  CHANGELOG.md, head's `[Unreleased]` holds no bullet and head has a new `## [x.y.z]` heading
  directly under it, `text-check` shall not require a CHANGELOG line and shall still run the D4
  suffix test (0053 D6 rule 3).
- **[REQ-0053-18]** (Unwanted behavior): IF a changed path is under `.claude/` or `.agents/`, THEN
  `text-check` shall treat the PR as needing a CHANGELOG line even when the path ends in `.md`
  (0053 D6 rule 1).

## Plan

Order: Plan 1 is the check, 2 the CI job, 3 the pointers, 4 the cleanup. Plan 4 touches
CHANGELOG.md, which this commit leaves alone.

- [ ] P1, `xtask/src/text_check.rs` and the `text-check` arm in `main.rs`: REQ-03, 04, 05, 08,
      11, 12. Red first: Test rows 1, 3, 4, 6, 7. `--base`, `--head` (default `HEAD`),
      `--body-file`, `--head-ref`. `std` and `git` only. Unit tests hold the exempt list and
      the suffix test.
- [ ] P2, `make text-check` (`BASE ?= origin/development`), and a job `text-check` in
      `.github/workflows/ci.yml`: `pull_request` only, checkout with full history, the PR body
      and head ref passed as environment variables (REQ-14). Non-required (0053 D5). Mind the
      fork condition of 0051 D8.
- [ ] P3, pointers: `AGENTS.md` rules 6 and 8, `CONTRIBUTING.md` line 35, and a line in the PR
      template above `Part of CER-N` ("one line under [Unreleased]: see spec 0053"). `RELEASING.md`
      (0048 P-day-one) links here (REQ-15). Spec 0003 line 45 is 0051's amendment, not this one.
- [ ] P4, cleanup of F1: merge the seven `[Unreleased]` headings into the D3 set. One PR,
      `docs(changelog)`, nothing else in it.
- [ ] P5, seven days after P2 with no false failure: add `text-check` to the required checks (REQ-16). When the lane rules the body checks fail: remove the advisory exit in REQ-12.

## Test

The block runs today from the repo root. It builds a scratch worktree, so it changes no
tracked file, and it implements the contract in shell as the reference for P1. Each row prints
`row N: ok` when the contract holds; the "red" rows pass only when the bad input is rejected,
so a check that accepts everything turns them red. The block fails closed: an `ok` is printed
only by a positive assertion.

```bash
b=origin/development; s=$(mktemp -d); git worktree add -q --detach "$s" "$b"; trap 'git worktree remove --force "$s"' EXIT
ex='^(docs|specs|engineering)/|(^|/)tests/|\.md$'; ag='^\.(claude|agents)/'
unrel() { awk '/^## \[Unreleased\]/{u=1;next} /^## \[[0-9]/{u=0} u' "$1"; }
rel()   { awk '/^## \[[0-9]/{p=1} p' "$1"; }
code()  { f=$(git -C "$s" diff --name-only "$b" | grep -v '^CHANGELOG.md$'); { echo "$f" | grep -E "$ag"; echo "$f" | grep -vE "$ex"; } | grep -c . || true; }
roll()  { [ -z "$(git -C "$s" diff --name-only "$b" | grep -vE '(^|/)Cargo\.toml$|^Cargo\.lock$|^CHANGELOG\.md$')" ] || return 1; [ "$(unrel "$s/CHANGELOG.md" | grep -c '^- ')" -eq 0 ] || return 1; h=$(awk '/^## \[Unreleased\]/{u=1;next} u&&NF{print;exit}' "$s/CHANGELOG.md"); case "$h" in '## ['[0-9]*) ;; *) return 1;; esac; ! git -C "$s" show "$b:CHANGELOG.md" | grep -qxF "$h"; }
added() { comm -13 <(git -C "$s" show "$b:CHANGELOG.md" | unrel /dev/stdin | sort) <(unrel "$s/CHANGELOG.md" | sort) | grep -c .; }
suffix(){ n=$(git -C "$s" show "$b:CHANGELOG.md" | rel /dev/stdin | wc -l); [ "$(tail -n "$n" "$s/CHANGELOG.md")" = "$(git -C "$s" show "$b:CHANGELOG.md" | rel /dev/stdin)" ]; }
verdict() { if [ "$(code)" -gt 0 ] && [ "$(added)" -eq 0 ] && ! roll; then echo FAIL-nolog; elif ! suffix; then echo FAIL-released; elif grep -qE '^(<<<<<<<|=======|>>>>>>>|\|\|\|\|\|\|\|)' "$s/CHANGELOG.md"; then echo FAIL-marker; else echo PASS; fi; }
reset() { git -C "$s" checkout -q -- .; }
want() { [ "$2" = "$3" ] && echo "row $1: ok ($3)" || { echo "row $1: RED got $2 want $3"; r=1; }; }
r=0
echo "// x" >> "$s/crates/uas-cli/src/main.rs";                              want 1 "$(verdict)" FAIL-nolog; reset
echo "// x" >> "$s/crates/uas-cli/src/main.rs"; sed -i '0,/^### Added/s//&\n- A new line./' "$s/CHANGELOG.md"; want 2 "$(verdict)" PASS; reset
echo "x" >> "$s/README.md"; want 3 "$(verdict)" PASS; reset
echo "// x" >> "$s/crates/uas-cli/src/main.rs"; sed -i '0,/^### Added/s//&\n- A new line./' "$s/CHANGELOG.md"; sed -i '/^## \[0.1.0\]/,$ s/^- /- EDITED /' "$s/CHANGELOG.md"; want 4 "$(verdict)" FAIL-released; reset
sed -i '0,/^### Added/s//<<<<<<< HEAD\n&/' "$s/CHANGELOG.md"; want 6 "$(verdict)" FAIL-marker; reset
t='\b(CER|OPS|EST|SEC|ONB|TOD|RD|BIZ|BRAND|VES)-[0-9]+\b'
sub() { printf '%s\n' "$1" | grep -qE "$t" && echo FAIL-ticket || echo PASS; }
want 7 "$(sub 'feat(uas): CER-123 add x')" FAIL-ticket
want 8 "$(sub 'feat(uas): add x')" PASS
want 9 "$(sub 'docs(spec): SHA-256 note')" PASS
want 10 "$(echo 'feat(uas): CER-123 add x' | grep -qE '^((feat|fix|perf|refactor|docs|test|ci|build|infra|security|style|chore)(\(.+\))?!?: .+)' && echo ACCEPT || echo REJECT)" ACCEPT
pl='^Part of (CER-[0-9]+|spec [0-9]{4})$'
want 11 "$(printf 'Part of CER-9\n' | grep -qE "$pl" && echo ACCEPT || echo REJECT)" ACCEPT
want 12 "$(printf 'Part of nothing\n' | grep -qE "$pl" && echo ACCEPT || echo REJECT)" REJECT
want 13 "$(grep -q '^changelog_update = false' release-plz.toml && echo PASS || echo FAIL)" PASS
want 5 "$(cd "$s" && make -s release-notes VERSION=9.9.9 >/dev/null 2>&1; sed -n 8,10p CHANGELOG.md | tr '\n' '|')" '## [Unreleased]||## [9.9.9] - '"$(date -u +%F)"'|'
want 14 "$(suffix && echo PASS || echo FAIL)" PASS; reset
rn() { (cd "$s" && make -s release-notes VERSION=9.9.9 >/dev/null 2>&1); }
rn;                                                                              want 19 "$(verdict)" PASS; reset
rn; sed -i '/^## \[0.1.0\]/,$ s/^- /- EDITED /' "$s/CHANGELOG.md";               want 20 "$(verdict)" FAIL-released; reset
echo x >> "$s/.claude/agents/uas-planner.md";                                    want 21 "$(verdict)" FAIL-nolog; reset
rn; echo "// x" >> "$s/crates/uas-cli/src/main.rs";                              want 22 "$(verdict)" FAIL-nolog; reset
body() { for h in Problem 'What this does' Verified 'Gated steps after merge' Rollback; do grep -q "^## $h" "$1" || { echo "warn: missing $h"; :; }; done; grep -qE "$pl" "$1" || { echo "warn: no Part of"; :; }; return 0; }
printf 'nothing\n' > "$s/b1"; cat .github/pull_request_template.md | sed 's/Part of CER-N/Part of CER-9/' > "$s/b2"
want 15 "$(body "$s/b1" | grep -c '^warn:'; body "$s/b1" >/dev/null; echo rc=$?)" $'6\nrc=0'
want 16 "$(body "$s/b2" | grep -c '^warn:')" 0
hd() { awk '/^### /{print $2}' | awk 'BEGIN{split("Added Changed Deprecated Removed Fixed Security",o," ");for(i in o)r[o[i]]=i} {if($1 in seen||r[$1]<=last){bad=1} seen[$1]=1;last=r[$1]} END{print bad?"FAIL":"PASS"}'; }
want 17 "$(printf '### Added\n### Changed\n### Fixed\n' | hd)" PASS
want 18 "$(printf '### Added\n### Changed\n### Added\n' | hd)" FAIL
exit $r
```

Rows 1 and 4 are the negative controls for 2 and 5: a check that passes every PR turns rows
1, 4 and 6 red, and a check that fails every PR turns rows 2, 3 and 5 red. Row 10 proves F8, the
gap that D7 closes: the 0051 type regex accepts the subject, so row 7 is the only guard. Row 5
runs the real `make release-notes` on the scratch copy, so a Makefile edit that rewrites a
released line turns row 14 red. The xtask replaces the shell functions in P1, and these rows
become its unit tests.

| REQ | Row or check |
|---|---|
| 01, 03 | 1, 2 |
| 02 | 17, 18 (the headings test). Today's file fails it (F1); P4 fixes that. The merge-base scope has no row yet; the xtask adds one |
| 04 | 3 |
| 05 | 4 |
| 06 | 5, 14 |
| 17 | 19, 20, 22 |
| 18 | 21 |
| 07 | 13 |
| 08 | 6 |
| 10, 11 | 7 to 9 (subjects), 11, 12 (the `Part of` line) |
| 12 | 15, 16 |
| 09, 13 | manual, reviewer: D8 and D2 are process rules; the reviewer reads the PR commits |
| 14 | manual, reviewer, until P2 lands; then `grep -n 'PR_BODY' .github/workflows/ci.yml` |
| 15 | manual, reviewer, when `RELEASING.md` lands (0048) |
| 16 | manual, lane: the date rule of D10 |

## Notes

- The ruling that this spec is drafted: the operator, lane w25:p1, 2026-10-11 about 03:10Z, by
  AskUserQuestion, label "CHANGELOG and commit/PR text policy" [reported: lane brief].
- The ruling on the gate: the operator, lane w25:p1, 2026-10-11 about 03:40Z, by AskUserQuestion,
  option label verbatim "Required once it runs green for a week" [reported: lane brief].
- Sibling specs: 0050 (build handoff pipeline) rules who writes the line (D6) and is not yet on
  `origin/development`; 0051 (repository guards) owns the subject regex. Both are cited by
  number only, so `depends_on` names none of them.
- Caveat: F2 counts a PR as "code" by path. A test-only PR outside `tests/` or a refactor with
  no user-visible effect still needs a line under D6. The lane may write "internal" under
  Changed; the check cannot judge user-visible, so it errs toward a line.
- Caveat: the ticket key list in D7 is reported (F12), not read from Linear. A new team needs a constant edit.
- Parked: a `skip-changelog` PR label as an escape hatch. Not added: a label is a bypass that
  the check cannot audit, and D6 already covers the real exemptions.
- Parked: the merge-subject branch-name IDs (F6, 4dfd100) come from GitHub, not an author.
- Amendment log: 2026-10-11, from plan 0053's findings (`engineering/plans/0053-changelog-and-commit-text-policy.md`):
  D6 gains the release-roll exemption (REQ-17, F13, Test rows 19, 20, 22) restricted to Cargo.toml, Cargo.lock and
  CHANGELOG.md, and the `.claude/` and `.agents/` precedence (REQ-18, F14, row 21); D4 and D5 diff from the merge base
  (finding 5); D3 and REQ-02 apply only when the merge base's headings were valid, and Deprecated sits between Changed
  and Fixed (findings 4 and 6); D10 counts seven required checks (finding 8). Rows 1 to 18 are unchanged.
- Amended 2026-10-11: opening words of the flagged requirement bullets reworded to begin with a standard EARS keyword; no change of meaning (spec 0052 P6).
