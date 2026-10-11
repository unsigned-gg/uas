---
status: planned
created: 2026-10-11
tags: [process, agents, plans, waves, ci, changelog]
priority: high
depends_on: [0002-verification-gates-and-evidence, 0003-branch-model-and-promotion, 0005-ci-cache-and-runners, 0042-cas-binary-conventions, 0049-cpm-migration-to-saga]
---

# 0050-build-handoff-pipeline

> **Status**: planned, **Priority**: high, **Created**: 2026-10-11

## Overview

Every build lane in this repo runs one procedure: a planner turns a landed spec into a plan,
builders build the packages in their own worktrees, a critic reads the merged wave, and the
integrating lane lands it on green. The procedure is written in two short places only,
`.claude/agents/README.md` ("Build handoff", lines 36 to 58) and `engineering/plans/README.md`.
the operator ruled on 2026-10-11 about 03:10Z (AskUserQuestion, label "Build-handoff pipeline")
that this spec is drafted. The content decisions are the integrating lane's unless marked
`proposed`.

This spec states the procedure once, records where the README and the practice differ, and
decides each difference. It adds no code. Spec 0025 is the machine form of the same chain
(Builder, Critic, Probe, PR); spec 0049 moves that machine form to saga. This pipeline is the
manual form that stays.

### Facts

Measured 2026-10-11 at `origin/development` e0e903f. Tiers: `verified` names the command.

- **F1 [verified]:** `.claude/agents/README.md` lines 36 to 58 give five steps: plan, dispatch,
  critic per SHA, merge and PR, next wave and status. Line 24 says "up to three subagents";
  lines 47 and 48 say three by the global rule, raised to five for this lane on 2026-10-10.
  Command: `sed -n 24p,36,58p` on the file.
- **F2 [verified]:** `engineering/plans/README.md` is seven lines. It names the plan fields
  (packages, files, done-when, red-first, persona, wave) and says "at most three builders" and
  "deletes or marks a plan done". Command: `cat` on the file.
- **F3 [verified]:** the plans for 0040, 0041, 0042, 0043 and 0048 share one shape: a header
  (spec, base sha, planner, date, tiers), "Findings the builders must know", "Plan items to
  packages", "Requirement coverage", a Waves table, one `### P<n>` section per package with
  Persona, Files, Change, Done-when and Red-first, and an Open list. BLOCKED packages carry the
  spec line that blocks them (plan 0048 P13 and P17; plan 0041 P8 and P9). Command:
  `grep -n '^## \|^### P' engineering/plans/0041-*.md engineering/plans/0048-*.md`.
- **F4 [verified]:** plans land in their own docs PRs before any build: #133 (0043), #134
  (0040), #143 (0041), #149 (0048), subjects `docs(plans): build plan for spec <n>`. Command:
  `gh pr list --repo unsigned-gg/uas --state merged --limit 30 --json number,title,mergedAt`.
- **F5 [verified]:** a wave is one PR from a branch `build/spec-<n>-wave<k>`. It holds merge
  commits of builder branches `build/<n>-p<k>-<slug>`, then a `docs(changelog)` commit, then
  merges of `origin/development` (PR #154, #156, #153, #157 via
  `gh pr view <n> --json commits`). Spec 0041 landed in five wave PRs: #147, #148, #150, #154,
  #157.
- **F6 [verified]:** PR #154, #156 and #157 each have a `ready_for_review` event, so each was
  a draft. The draft stood 29, 4 and 6 minutes before ready, and the PR merged 2 minutes later
  (#154: created 02:03:45Z, ready 02:32:42Z, merged 02:34:55Z). Command:
  `gh api repos/unsigned-gg/uas/issues/<n>/timeline --jq '.[].event'`. Each body has a
  "Critic pass (in-house) and fixes" section (#154, #157) naming the defects found.
- **F7 [verified]:** the last 25 non-merge commits that touch `CHANGELOG.md` all have the subject
  `docs(changelog): ...` and none carries code. Command: `git log origin/development
  --no-merges --format=%s -25 -- CHANGELOG.md`. Plan 0048 says "The integrator owns the single
  `CHANGELOG.md` line". Yet `uas-builder.md` line 17, `uas-ebpf-builder.md` line 17 and
  `uas-k8s-builder.md` line 17 let a builder write that line.
- **F8 [verified]:** `gh api repos/unsigned-gg/uas/branches/development/protection --jq
  '.required_status_checks.contexts'` prints `secrets`, `rust / cargo`, `cargo-deny`,
  `hygiene`, `ebpf-live`. Spec 0003 lines 59 to 63 list the same five. `ci.yml` also defines
  `moon-affected`, which is not required. `required_signatures.enabled` and
  `required_linear_history.enabled` print `false`, so signing rests on AGENTS.md rule 5 and
  the push hooks, not on the server.
- **F9 [verified]:** the persona sizes differ. `uas-planner.md` line 41 cuts a package under
  400 changed lines; `uas-engineer.md` line 50 writes a brief for under about 800.
  `uas-planner.md` line 50 and README line 43 cap a wave at three builders; the plans comply
  (plan 0041 wave 1: P1, P2, P4; plan 0048 wave 6: P14 to P16).
- **F10 [verified]:** spec status lags the README rule (line 54 to 55). Spec 0041 is `planned` in
  frontmatter and registry after five landed waves. Specs 0040 and 0043 are `in-progress` in
  frontmatter and `planned` in the registry. `spec-check` does not compare the two. Spec 0042
  moved to `in-progress` at its first landed wave (its Notes line 91).
- **F11 [verified]:** commit 61bf3cd (`docs(changelog): drop two stale diff3 base markers left
  by a merge resolution`) fixed `|||||||` markers that a scripted conflict resolution left in
  `CHANGELOG.md`. Command: `git show 61bf3cd --stat`.
- **F12 [verified]:** no file in the repo tells a lane to remove worktrees after a merge.
  Command: `grep -rn 'worktree remove\|worktree prune' --include=*.md . | grep -v vendor`
  hits only spec 0025 (the Critic's scratch tree). The head branch is deleted on GitHub
  2 seconds after merge (`head_ref_deleted` in the F6 timeline), which does not touch local
  branches or worktrees.
- **F13 [verified]:** the name in the brief, "spec 0038 (uas binary conventions)", is stale.
  Spec 0038 is `uas-operator-and-crds`. The binary conventions are spec 0042 (its H1 and
  registry title). Command: `head -12 specs/0042-cas-binary-conventions.md`.

## Design

### Decisions

Classes: `ruled` (the operator ruled it), `lane-decided` (the integrating lane, 2026-10-11;
the operator may override), `proposed: <for whom>`.

| ID | Decision | Rationale | Class and source |
|---|---|---|---|
| D1 | This spec is the procedure of record. The README "Build handoff" section and `engineering/plans/README.md` shrink to a pointer to it, keeping the persona table. | One statement cannot drift from itself (F1, F2, F9). | `lane-decided` |
| D2 | The plan file carries the F3 sections: header with base sha and tiers, findings, plan item to package map, requirement coverage, Waves table with a "disjoint because" column, packages, open list. | The shape the five plans share; builders run from it without the spec (F3). | `lane-decided` |
| D3 | A package is one builder, one worktree, one crate or file group. Target under 400 changed lines; a package that exceeds 800 is split. Each package has Persona, Files (existing or `new`), Change, Done-when and Red-first. Done-when is a command with an exit code from the spec Test section or a Makefile gate. A package with no runnable Done-when is BLOCKED and names the spec line. | The planner cuts packages, so its 400 governs; the engineer's 800 is the ceiling for a brief (F9). | `lane-decided` |
| D4 | At most five agents run at once per lane, counting builders, critics and probers. A wave holds at most three builders, with disjoint files. The next wave is cut from the new `origin/development` after the wave PR lands. During a development hold, planning runs and dispatch waits (README lines 57, 58). | Five: the operator, 2026-10-10. Three per wave leaves room for a critic and a prober inside the cap (F9). | cap `ruled`; wave size `lane-decided` |
| D5 | Builders commit signed and never push. The integrating lane merges builder branches into one wave branch with merge commits, pushes with `git -C`, and opens the PR as a draft. | Push hooks judge the shared checkout (README line 66); the draft starts CI while the critic reads (F5, F6). | `lane-decided` |
| D6 | The integrating lane owns the CHANGELOG line: one `docs(changelog)` commit per wave PR, under `## [Unreleased]`. A builder writes none. The three builder personas drop their CHANGELOG clause. | One writer cannot conflict with itself; 25 of 25 recent changelog commits are the integrator's (F7). | `lane-decided` |
| D7 | The in-house `uas-critic` re-reads the merged wave branch once the draft PR is open and before it is marked ready, and never before the draft exists. It returns numbered defects with file and line, or no defects plus what it checked. The lane applies each item (a defect in a builder file goes back to that builder) and records the pass in the PR body. | F6: every wave PR shows the critic pass between draft and ready. | `lane-decided` |
| D8 | A per-SHA critic pass on a builder branch is optional. It is required for a package that touches `unsafe`, the sandbox, the pool's kernel calls or `crates/uas-ebpf`. | README step 3 and line 15 ask for it on every SHA; the trail shows the wave-level pass only (F6). | `lane-decided: integrating lane w25:p1, 2026-10-11` |
| D9 | The lane marks the PR ready and lands it when all five required checks read SUCCESS on the head: `secrets`, `rust / cargo`, `cargo-deny`, `hygiene`, `ebpf-live`. The merge is a merge commit pinned to the head sha. `moon-affected` is advisory. Red stops the wave. | AGENTS.md rules 7 and 9, approval routes; spec 0003 table; F8. | `ruled` by spec 0003 and the lane approval route |
| D10 | After the PR state reads MERGED, the lane removes the wave's worktrees, prunes, and deletes the local builder and wave branches. It reads the state from `gh pr view <n> --json state`, never from the merge button. A branch whose PR is open or closed unmerged stays. | F12: nothing in the repo says it, and a worktree removed before MERGED loses unlanded work. | `lane-decided` |
| D11 | Conflicts resolve with merge commits only. Merging `origin/development` into the wave branch is routine. In `CHANGELOG.md` keep every line from both sides; in `specs/README.md` keep one row per spec in number order. A scripted resolution drops the whole `|||||||` base section. No conflict or diff3 marker reaches a pushed commit. | F11. | `lane-decided` |
| D12 | A verdict block is 15 lines or fewer, bad news first, with a tier on each claim. A builder returns STATUS, SHA, FILES, CHECKS, TESTS, SCOPE, OPEN. A critic returns VERDICT, numbered DEFECTS, CHECKED, UNCHECKED. A planner returns PLAN, SPEC, BLOCKED, PINS, FIRST, OPEN. An engineer returns STATUS, SHA, SPEC, DECIDES, PINS, BRIEFS, OPEN. | The persona Output sections already say so; D12 makes them the contract. | `lane-decided` |
| D13 | A plan is done when every unblocked package has landed. The package heading gains `(DONE <date>, #<PR>)`; the plan file stays as the record. | README line 54 and plan 0048 P12 already use the suffix. | `lane-decided` |
| D14 | The spec status moves in the wave PR that lands the work: `planned` to `in-progress` with the first landed wave, `complete` when the plan is done and the spec Test command exits 0 on `development`. Frontmatter and registry change in the same commit. The move is a package in the plan that lands with the last wave. | F10: three specs disagree with the README rule today. | `lane-decided: integrating lane w25:p1, 2026-10-11` |
| D15 | Relations. Spec 0003 supplies the branch model, merge commits and the required checks (lines 28, 43 to 47, 59 to 63, 69, 70). Spec 0042 supplies the binary conventions that `uas wave` followed (F13); spec 0038 has no part in this pipeline. Spec 0049 removes `uas wave` and the in-process wave loop; the steps of D5 to D11 do not call them. | F13; spec 0049 REQ-0049-01. | `lane-decided` |

### Requirements (EARS)

- **[REQ-0050-01]** (Ubiquitous): Every plan file shall hold, for each package, a Persona, Files,
  Done-when and Red-first, or the word BLOCKED with the spec line that blocks it (0050 D2, 0050 D3).
- **[REQ-0050-02]** (Ubiquitous): A builder shall write only the files its package names, commit
  signed, and return a verdict block of 15 lines or fewer (0050 D3, 0050 D12).
- **[REQ-0050-03]** (Unwanted behavior): IF a builder pushes, opens a PR or edits `CHANGELOG.md`, THEN
  the critic shall report it as a scope defect (0050 D5, 0050 D6).
- **[REQ-0050-04]** (State-driven): WHILE a lane runs agents, it shall run at most five at once, and
  no wave shall hold more than three builders with overlapping files (0050 D4).
- **[REQ-0050-05]** (Event-driven): WHEN a wave's builders return, the integrating lane shall merge
  their branches into one wave branch with merge commits and open the PR as a draft (0050 D5).
- **[REQ-0050-06]** (Event-driven): WHEN the draft PR is open, the critic shall re-read the merged
  wave branch and return numbered defects with file and line, or no defects with what it checked.
  The PR shall not be marked ready before that pass (0050 D7).
- **[REQ-0050-07]** (Ubiquitous): Each wave PR shall carry exactly one `docs(changelog)` commit from
  the integrating lane when the wave changes user-visible behavior (0050 D6).
- **[REQ-0050-08]** (Event-driven): WHEN all five required checks read SUCCESS on the head, the lane
  shall land the PR with a merge commit pinned to that head (0050 D9).
- **[REQ-0050-09]** (Unwanted behavior): IF any required check is red or pending, THEN the lane shall
  not merge, and shall not mark a pending check as passed (0050 D9).
- **[REQ-0050-10]** (Event-driven): WHEN `gh pr view` reads MERGED, the lane shall remove the wave's
  worktrees and local branches. Until then it shall remove none (0050 D10).
- **[REQ-0050-11]** (Unwanted behavior): IF a merge leaves a conflict marker or a diff3 base section
  in a file, THEN the lane shall remove it before pushing (0050 D11).
- **[REQ-0050-12]** (Event-driven): WHEN the last unblocked package of a plan lands, the lane shall
  mark the plan done and move the spec status in frontmatter and registry together (0050 D13, 0050 D14).
- **[REQ-0050-13]** (State-driven): WHILE a development hold stands, the lane shall plan and shall
  not dispatch builders (0050 D4).
- **[REQ-0050-14]** (Optional feature): WHERE a package touches `unsafe`, the sandbox, the pool's
  kernel calls or `crates/uas-ebpf`, the critic shall also read the builder branch before the merge (0050 D8).

## Plan

Each item is one builder package under D3. They touch `.claude/` and `engineering/`, so a builder
brief names those files; the spec itself is the only file this PR changes.

- [ ] P1 (`uas-builder`): shrink `.claude/agents/README.md` lines 36 to 58 to a pointer to this
      spec; fix line 24 to five. Shrink `engineering/plans/README.md` the same way.
- [ ] P2 (`uas-builder`): drop the CHANGELOG clause from `uas-builder.md`, `uas-ebpf-builder.md`
      and `uas-k8s-builder.md` (D6), set the engineer's brief ceiling to D3. Run `prompt-linter`.
- [ ] P3 (`uas-engineer`): reconcile statuses of specs 0040, 0041 and 0043 per D14, and add the
      status move as the last package of each plan (D14).
- [ ] P4 (not ruled): extend `cargo xtask spec-check` to compare frontmatter status with
      the registry row, and to check plan files against REQ-0050-01. Rows 1 and 3 below are the
      manual forms until then.

## Test

```bash
set -u
chk() { awk '/^### P[0-9]+/{if(h&&!ok)print h; h=$2; ok=0} h&&/[Dd]one-when|BLOCKED/{ok=1} END{if(h&&!ok)print h}' "$1"; }
t=$(mktemp); sed 's/[Dd]one-when/x/g;s/BLOCKED/x/g' engineering/plans/0043-*.md > "$t"
echo "1 plans lacking done-when: $(for f in engineering/plans/0*.md; do chk "$f"; done | wc -l); control: $(chk "$t" | wc -l)"
echo "2 changelog files in branch: $(git diff --name-only origin/development...HEAD | grep -cx CHANGELOG.md); control a9d2d1c: $(git show --format= --name-only a9d2d1c | grep -cx CHANGELOG.md)"
for f in specs/0*.md; do s=$(basename "$f" .md); a=$(sed -n 's/^status: //p' "$f" | head -1); b=$(grep "($s.md)" specs/README.md | awk -F'|' '{gsub(/ /,"",$4); print $4}'); [ "$a" = "$b" ] || echo "3 status mismatch $s: $a vs $b"; done
got=$(gh api repos/unsigned-gg/uas/branches/development/protection --jq '.required_status_checks.contexts|sort|join(",")'); want="cargo-deny,ebpf-live,hygiene,rust / cargo,secrets"
[ "$got" = "$want" ] && echo "4 required checks ok" || echo "4 RED: $got"
printf '<<<<<<< a\n||||||| b\n' > "$t"; pat='^(<<<<<<< |>>>>>>> |\|\|\|\|\|\|\| )'
echo "5 markers in tree: $(git grep -cE "$pat" -- CHANGELOG.md specs | wc -l); control: $(grep -cE "$pat" "$t")"
echo "6 control, merged build branch state: $(gh pr list --repo unsigned-gg/uas --head build/spec-0041-wave5 --state all --json state --jq '.[0].state')"
git worktree list --porcelain | awk '/^branch /{sub("refs/heads/","",$2); print $2}' | while read -r b; do
  [ "$(gh pr list --repo unsigned-gg/uas --head "$b" --state all --json state --jq '.[0].state')" = MERGED ] && echo "6 RED worktree left after MERGED: $b"; done
rm -f "$t"
```

Run from the repo root of a clone with `gh` signed in. Read the lines, not the exit code.

| # | Check | Today (2026-10-11) | Passes when | Covers |
|---|---|---|---|---|
| 1 | Plans with a package that has no done-when and no BLOCKED | `0`; the stripped control prints the packages it catches, more than 0 | `0` while the control stays above 0 | REQ-0050-01 |
| 2 | CHANGELOG in the wave PR's diff | `0` on this docs branch (the integrator adds it); control a9d2d1c prints `1` | `1` on a wave branch | REQ-0050-07 |
| 3 | Frontmatter status against the registry row | two lines: 0040 and 0043 (F10) | no line | REQ-0050-12 |
| 4 | The five required checks, exactly | `4 required checks ok` | still ok; a sixth or a missing name prints `4 RED` | REQ-0050-08, 09 |
| 5 | Conflict markers in `CHANGELOG.md` and `specs/` | `0`; control prints `2` | `0` | REQ-0050-11 |
| 6 | A worktree whose PR reads MERGED | control prints `MERGED`; no `RED` line | no `RED` line. Control run once by hand: a worktree on a branch named `build/spec-0041-wave5` printed `6 RED` | REQ-0050-10 |

Row 1 has no failing plan today, so its control proves the awk can go red. Rows 2 and 3 are red on a real failure: a wave branch with no changelog commit
prints `0`, and the two status lines show row 3 fails today. Rows for REQ-0050-02 to 06 are
reviewer checks: the critic reads `git log --format=%G?` on the wave branch (non-merge commits
read `G`), `git diff --stat` against each package's files, and the PR timeline for
`ready_for_review` after the critic section.

## Notes

- The ruling that this spec is drafted: the operator, lane w25:p1, 2026-10-11 about 03:10Z, by
  AskUserQuestion, label "Build-handoff pipeline" [reported: w25:p1 brief].
- The cap of five: the operator, 2026-10-10 [reported: `.claude/agents/README.md` lines 47, 48].
- Difference, README against practice, decided here: critic per SHA (D8, D7); CHANGELOG written
  by a builder (D6); a cap of three at README line 24 (D4); a status move with no timing (D14).
- Difference, persona against persona: package size 400 against 800 (D3).
- F8 caveat: `required_signatures` is `false` on the server. A lane checks `%G?` itself.
  Commits made by GitHub's merge button read `E` locally (no key to verify), which is expected.
- Parked: a `plan-check` and a status-parity check in `xtask` (Plan P4), a PR template that
  asks for the critic pass and the changelog commit (`.github/pull_request_template.md` has
  Problem, What this does and Verified only).
- Parked: saga spec 0040 restates the machine form of this pipeline. When it passes parity (spec
  0049 D5), this spec gains a pointer to it, not a rewrite.
