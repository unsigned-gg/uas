---
status: planned
created: 2026-10-11
tags: [changelog, commits, pull-request, release, ci]
priority: medium
depends_on: [0003-branch-model-and-promotion, 0005-ci-cache-and-runners, 0048-licence-and-publication]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0053-changelog-and-commit-text-policy

> **Status**: planned, **Priority**: medium, **Created**: 2026-10-11

## Overview

`AGENTS.md` rules 6 and 8 state three text rules: one CHANGELOG line per user-visible PR, no
ticket ID in a commit subject, and `Part of CER-N` in the PR body. Spec 0005 (item 5) and spec
0048 D13 repeat the CHANGELOG rule. No check enforces any of them, and no spec owns their
detail. This spec owns the contract and adds one check, `text-check`, that runs the contract.

The practice is mostly good, so the check mostly confirms it. The CHANGELOG rule holds in 25 of
28 recent code PRs (F2). The ticket-ID rule holds in all recent subjects (F6). The PR template
is used by none of the last 30 PRs (F7), so the body checks start advisory (0053 D9).

Spec 0051 owns the commit-subject regex and lefthook. This spec does not restate it. It adds
what that regex cannot see: a ticket ID after a valid type (F8).

### Facts

Measured 2026-10-11 on ceres at `origin/development` 3e60a55, worktree `/tmp/uas-w53`.

- **F1 [verified]:** `[Unreleased]` spans lines 8 to 118 and holds seven headings: Added,
  Changed, Fixed, Changed, Added, Changed, Fixed. Command: `awk '/^## \[0/{exit} /^## \[Unreleased\]/{u=1}
  u&&/^### /{print NR": "$0}' CHANGELOG.md`. Keep a Changelog 1.1.0 allows each heading once.
- **F2 [verified]:** of the last 60 first-parent merges into development, 28 changed a file
  outside `docs/`, `specs/`, `engineering/` and `*.md`; 25 of those also changed CHANGELOG.md.
  The three that did not: a dependabot bump (#122), a manifest-test PR (#130) and an old
  cerebral-work PR (#108). Command: the loop in Test row 0, `git diff --name-only M^1 M`.
- **F3 [verified]:** 39 of the last 40 commits touching CHANGELOG.md are by the integrating
  lane's author; 25 of the 40 have the subject shape `docs(changelog): ...`. Command:
  `git log --format='%an %s' -40 -- CHANGELOG.md`. A builder PR carries its line as a separate
  commit, which is the shape spec 0050 D6 rules.
- **F4 [verified]:** `release-plz.toml` line 7 sets `changelog_update = false`, so release-plz
  never writes CHANGELOG.md. The roll is `make release-notes` (Makefile lines 109 to 120): an
  awk that prints `## [Unreleased]`, a blank line, then `## [x.y.z] - date`. No released line is
  rewritten. The header comment of `release.yml` line 3 says the roll happens before release-plz.
  Command: `grep -n changelog_update release-plz.toml; sed -n 109,120p Makefile`.
- **F5 [verified]:** no workflow reads CHANGELOG.md: `grep -rn -i changelog .github/workflows`
  prints one comment line (release.yml line 3). The rule is unenforced today.
- **F6 [verified]:** `git log --no-merges --format=%s -300 origin/development | grep -cE
  '\b(CER|OPS|TOD|RD|EST)-[0-9]+\b'` prints 1 (70b34b1, spec 0025); over all history it prints 2.
  GitHub merge subjects can carry a branch name with an ID (4dfd100), which no author chose.
- **F7 [verified]:** `gh pr list -R unsigned-gg/uas --state merged --limit 30 --json body`: 0 of
  30 bodies hold `## Problem`; 1 holds `Part of CER` or `Part of spec`. In the last 300 commit
  messages 44 hold a `Part of` line (`git log -300 --format=%B | grep -c '^Part of'`).
- **F8 [verified]:** the lefthook regex (`lefthook.yml` line 10) accepts `feat(uas): CER-123 add
  x` and rejects `CER-123: x` only because `CER-123` is not a type. Test row 10 runs it. So a
  ticket ID after a valid type passes today.
- **F9 [verified]:** `.github/pull_request_template.md` lines 1 to 25 has five sections in
  order: Problem, What this does, Verified, Gated steps after merge, Rollback, and ends with
  `Part of CER-N`. `CONTRIBUTING.md` lines 33 to 41 repeat the rules.
- **F10 [verified]:** `CHANGELOG.md` holds no conflict marker line today (`grep -cE
  '^(<<<<<<<|=======|>>>>>>>|\|\|\|\|\|\|\|)' CHANGELOG.md` prints 0). Commit 61bf3cd had to drop
  two stale diff3 base markers a merge resolution left, so the failure has happened.
- **F11 [verified]:** `xtask/src/main.rs` lines 26 to 29 route `spec-check`, `toolchain-check` and
  `pin-check`. The CI `hygiene` job runs `make hygiene` on push and pull request.

## Design

### Decisions

Classes: `ruled` (Christian decided it), `lane-decided` (the integrating lane decided it on
2026-10-11 and Christian may override), `proposed: Christian` (it adds a CI job
or changes what a PR author must do). Christian ruled on 2026-10-11 about 03:10Z (AskUserQuestion,
lane w25:p1, "CHANGELOG and commit/PR text policy") that this spec is drafted.

| # | Decision | Why | Class |
|---|---|---|---|
| D1 | One bullet per user-visible PR, under `## [Unreleased]`, under one heading of D3. A wave PR gets one bullet for the wave. The bullet is one paragraph with no hard wrap and no nested list. It names the spec and plan package when there is one. No length cap: the practice runs to several hundred characters. | Spec 0048 D13 ruled the one-line-per-PR rule. The wave bullets in F1 are the practice. | ruled: 0048 D13 (Christian, 2026-10-11 about 02:35Z); wave and shape lane-decided |
| D2 | The integrating lane writes the line, as a `docs(changelog)` commit inside the PR before it is marked ready. A builder writes none. A human author writes their own. | One writer cannot conflict with itself (0050 D6). F3 shows this is the practice. | lane-decided (0050 D6) |
| D3 | `[Unreleased]` may carry the Keep a Changelog 1.1.0 headings, each at most once, in this order: Added, Changed, Deprecated, Removed, Fixed, Security. A bullet goes under the heading that fits it. Cleaning F1 is a Plan item, not part of this commit. | F1 has duplicates; the format forbids none but the roll reads cleaner without them. | lane-decided |
| D4 | A released section is never edited. Enforcement is the check, not release-plz: release-plz writes no CHANGELOG (F4). The test: every line of base from its first `## [x.y.z]` heading to the end shall be a suffix of head. The roll inserts above that point, so a roll PR passes; an edit inside it fails. | `changelog_update = false` means the tool offers no protection. A suffix test needs no knowledge of which PR is a roll. | lane-decided |
| D5 | The check is `cargo xtask text-check --base <ref>`, with `make text-check`, and a CI job `text-check` on `pull_request` only. It uses `std` and the `git` binary, no new crate, so no spec 0004 pin. The job ships non-required. Making it required is a branch-protection change, which is Christian's. | A Rust xtask matches `spec-check` (F11). A job in `hygiene` would run on push without a base. | lane-decided: integrating lane w25:p1, 2026-10-11; required status: proposed: Christian |
| D6 | The CHANGELOG-line rule applies to a PR that changes a file outside this exempt list: `docs/**`, `specs/**`, `engineering/**`, any `*.md` other than CHANGELOG.md, and `**/tests/**`. A PR whose head ref begins `dependabot/` is exempt. Any other PR must add a line to `[Unreleased]`: a line of head's section that base's lacks. `.claude/` and `.agents/` are not exempt: agent files are user-visible to the lanes (the 2026-10-10 repo-agents bullet). | F2: 25 of 28. The three misses are the exempt shapes plus an old PR. | lane-decided: integrating lane w25:p1, 2026-10-11 |
| D7 | A non-merge, non-revert commit subject in `base..head` shall match no ticket pattern: `\b(CER|OPS|TOD|RD|EST)-[0-9]+\b`. The ID goes in the PR body as a line `Part of CER-N` or `Part of spec NNNN`. The check reads `git log`, so it also catches a commit made with `LEFTHOOK=0`. The prefix list is asserted from the estate's Linear teams and is edited in one constant. Spec 0051 owns the type regex (0051 D4); this rule adds only the ID. | F8: the regex cannot see it. F6: one violation in 300. | lane-decided |
| D8 | A CHANGELOG conflict is resolved by keeping every bullet from both sides and dropping the markers, including diff3 base sections (`|||||||`). Order within a heading does not matter. The check fails on any marker line. Fewer conflicts come from D2. | F10. Both-sides is lossless for a list of independent bullets. | lane-decided (0050 D6 reduces the rate) |
| D9 | The PR body contract is the five template sections of F9 in order, and one `Part of` line. The check reports a missing section or line. It is advisory for now: it prints `warn:` and exits 0. | F7: 0 of 30 bodies use the template, so a hard fail would fail every PR on day one. | lane-decided: integrating lane w25:p1, 2026-10-11 |
| D10 | The body checks become failing when the integrating lane rules so, after the lanes have used the template for a run of PRs. This spec sets no date. | A date invented now would be asserted. | lane-decided: integrating lane w25:p1, 2026-10-11 |
| D11 | `RELEASING.md` (0048 D13, REQ-0048-15) links to this spec for the CHANGELOG rule and does not copy it. `AGENTS.md` rules 6 and 8 and `CONTRIBUTING.md` line 35 get a pointer here (Plan). Spec 0005 item 5 stays as the release-flow statement. | One owner for the contract text keeps four copies from drifting. | lane-decided |

### Requirements (EARS)

- **[REQ-0053-01]** (Ubiquitous): The repository shall carry one bullet in `## [Unreleased]` of
  CHANGELOG.md for each user-visible PR, under a heading of the D3 set (0053 D1, 0053 D3).
- **[REQ-0053-02]** (Ubiquitous): `## [Unreleased]` shall hold each of the headings Added,
  Changed, Deprecated, Removed, Fixed and Security at most once, in that order (0053 D3).
- **[REQ-0053-03]** (Event-driven): WHEN a PR changes a file outside the exempt list of 0053 D6,
  `text-check` shall fail unless head's `[Unreleased]` section holds a line that base's lacks.
- **[REQ-0053-04]** (Optional feature): WHERE the PR head ref begins `dependabot/`, or every
  changed file is on the exempt list, `text-check` shall not require a CHANGELOG line.
- **[REQ-0053-05]** (Unwanted behavior): IF any line of base from its first released heading to
  the end is not an unchanged suffix of head's CHANGELOG.md, THEN `text-check` shall fail and
  name the first differing line (0053 D4).
- **[REQ-0053-06]** (Event-driven): WHEN `make release-notes` runs, it shall insert the new
  version heading directly under `## [Unreleased]` and rewrite no released line (0053 D4).
- **[REQ-0053-07]** (Ubiquitous): `release-plz.toml` shall keep `changelog_update = false`, so
  no tool other than `make release-notes` writes CHANGELOG.md (0053 D4).
- **[REQ-0053-08]** (Unwanted behavior): IF CHANGELOG.md holds a line that starts with
  `<<<<<<<`, `=======`, `>>>>>>>` or `|||||||`, THEN `text-check` shall fail (0053 D8).
- **[REQ-0053-09]** (Event-driven): WHEN a CHANGELOG conflict is resolved, the resolver shall
  keep every bullet from both sides and no marker line (0053 D8).
- **[REQ-0053-10]** (Ubiquitous): A commit subject shall carry no ticket ID; the PR body shall
  carry a line `Part of CER-N` or `Part of spec NNNN` (0053 D7).
- **[REQ-0053-11]** (Unwanted behavior): IF a non-merge, non-revert subject in `base..head`
  matches the ticket pattern of 0053 D7, THEN `text-check` shall fail and print the SHA.
- **[REQ-0053-12]** (State-driven): WHILE 0053 D10 has not been ruled, `text-check` shall print
  `warn:` for a PR body that lacks a template section or the `Part of` line and shall exit 0.
- **[REQ-0053-13]** (Ubiquitous): The integrating lane shall write the CHANGELOG line as a
  `docs(changelog)` commit in the PR before it is marked ready (0053 D2, 0050 D6).
- **[REQ-0053-14]** (Ubiquitous): `text-check` shall read the PR body from a file the workflow
  writes from an environment variable, never from text interpolated into a shell command.
- **[REQ-0053-15]** (Ubiquitous): `RELEASING.md` shall link to this spec for the CHANGELOG
  rule and shall not restate it (0053 D11, 0048 D13).

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
- [ ] P5, when the lane rules 0053 D10: remove the advisory exit in REQ-12.

## Test

The block runs today from the repo root. It builds a scratch worktree, so it changes no
tracked file, and it implements the contract in shell as the reference for P1. Each row prints
`row N: ok` when the contract holds; the "red" rows pass only when the bad input is rejected,
so a check that accepts everything turns them red. The block fails closed: an `ok` is printed
only by a positive assertion.

```bash
b=origin/development; s=$(mktemp -d); git worktree add -q --detach "$s" "$b"; trap 'git worktree remove --force "$s"' EXIT
ex='^(docs|specs|engineering)/|(^|/)tests/|\.md$'
unrel() { awk '/^## \[Unreleased\]/{u=1;next} /^## \[[0-9]/{u=0} u' "$1"; }
rel()   { awk '/^## \[[0-9]/{p=1} p' "$1"; }
code()  { git -C "$s" diff --name-only "$b" | grep -vE "$ex" | grep -vc '^CHANGELOG.md$' || true; }
added() { comm -13 <(git -C "$s" show "$b:CHANGELOG.md" | unrel /dev/stdin | sort) <(unrel "$s/CHANGELOG.md" | sort) | grep -c .; }
suffix(){ n=$(git -C "$s" show "$b:CHANGELOG.md" | rel /dev/stdin | wc -l); [ "$(tail -n "$n" "$s/CHANGELOG.md")" = "$(git -C "$s" show "$b:CHANGELOG.md" | rel /dev/stdin)" ]; }
verdict() { if [ "$(code)" -gt 0 ] && [ "$(added)" -eq 0 ]; then echo FAIL-nolog; elif ! suffix; then echo FAIL-released; elif grep -qE '^(<<<<<<<|=======|>>>>>>>|\|\|\|\|\|\|\|)' "$s/CHANGELOG.md"; then echo FAIL-marker; else echo PASS; fi; }
reset() { git -C "$s" checkout -q -- .; }
want() { [ "$2" = "$3" ] && echo "row $1: ok ($3)" || { echo "row $1: RED got $2 want $3"; r=1; }; }
r=0
echo "// x" >> "$s/crates/uas-cli/src/main.rs";                              want 1 "$(verdict)" FAIL-nolog; reset
echo "// x" >> "$s/crates/uas-cli/src/main.rs"; sed -i '0,/^### Added/s//&\n- A new line./' "$s/CHANGELOG.md"; want 2 "$(verdict)" PASS; reset
echo "x" >> "$s/README.md"; want 3 "$(verdict)" PASS; reset
echo "// x" >> "$s/crates/uas-cli/src/main.rs"; sed -i '0,/^### Added/s//&\n- A new line./' "$s/CHANGELOG.md"; sed -i '/^## \[0.1.0\]/,$ s/^- /- EDITED /' "$s/CHANGELOG.md"; want 4 "$(verdict)" FAIL-released; reset
awk -v v=9.9.9 -v d=2026-01-01 '/^## \[Unreleased\]/&&!x{print;print "";print "## [" v "] - " d;x=1;next}{print}' "$s/CHANGELOG.md" > "$s/c.tmp" && mv "$s/c.tmp" "$s/CHANGELOG.md"; want 5 "$(suffix && echo PASS || echo FAIL-released)" PASS; reset
sed -i '0,/^### Added/s//<<<<<<< HEAD\n&/' "$s/CHANGELOG.md"; want 6 "$(verdict)" FAIL-marker; reset
t='\b(CER|OPS|TOD|RD|EST)-[0-9]+\b'
sub() { printf '%s\n' "$1" | grep -qE "$t" && echo FAIL-ticket || echo PASS; }
want 7 "$(sub 'feat(uas): CER-123 add x')" FAIL-ticket
want 8 "$(sub 'feat(uas): add x')" PASS
want 9 "$(sub 'docs(spec): SHA-256 note')" PASS
echo 'feat(uas): CER-123 add x' | grep -qE '^((feat|fix|perf|refactor|docs|test|ci|build|infra|security|style|chore)(\(.+\))?!?: .+)' && echo "row 10: ok (the 0051 type regex accepts it, so row 7 is the only guard)"
printf 'Part of CER-9\n' | grep -qE '^Part of (CER-[0-9]+|spec [0-9]{4})$' && echo "row 11: ok"
printf 'Part of nothing\n' | grep -qE '^Part of (CER-[0-9]+|spec [0-9]{4})$' && { echo "row 12: RED"; r=1; } || echo "row 12: ok (rejected)"
grep -q '^changelog_update = false' release-plz.toml && echo "row 13: ok"
exit $r
```

Rows 1 and 4 are the negative controls for 2 and 5: a check that passes every PR turns rows
1, 4 and 6 red, and a check that fails every PR turns rows 2, 3 and 5 red. Row 10 is not a
check of this spec; it proves F8, the gap that D7 closes. Rows 3 and 11 to 13 cover REQ-04,
REQ-10, REQ-12 and REQ-07. The xtask replaces the shell functions in P1, and these rows become
its unit tests.

| REQ | Row |
|---|---|
| 01, 03 | 1, 2 |
| 04 | 3 |
| 05, 06 | 4, 5 |
| 08 | 6 |
| 10, 11 | 7 to 9 |
| 07 | 13 |
| 02, 09, 12 to 15 | read: D3, D8 and D2 are process rules; P3 and P5 hold the rest |

## Notes

- The ruling that this spec is drafted: Christian, lane w25:p1, 2026-10-11 about 03:10Z, by
  AskUserQuestion, label "CHANGELOG and commit/PR text policy" [reported: lane brief].
- Sibling specs: 0050 (build handoff pipeline) rules who writes the line (D6) and is not yet on
  `origin/development`; 0051 (repository guards) owns the subject regex. Both are cited by
  number only, so `depends_on` names none of them.
- Caveat: F2 counts a PR as "code" by path. A test-only PR outside `tests/` or a refactor with
  no user-visible effect still needs a line under D6. The lane may write "internal" under
  Changed; the check cannot judge user-visible, so it errs toward a line.
- Caveat: the ticket prefix list in D7 is asserted. A new Linear team needs a constant edit.
- Parked: a `skip-changelog` PR label as an escape hatch. Not added: a label is a bypass that
  the check cannot audit, and D6 already covers the real exemptions.
- Parked: the merge-subject branch-name IDs (F6, 4dfd100) come from GitHub, not an author.
