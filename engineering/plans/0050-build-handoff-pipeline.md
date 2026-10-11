<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plan: 0050-build-handoff-pipeline

Spec: `specs/0050-build-handoff-pipeline.md` (status planned). Base: `origin/development` `2a3d442` (the plan was written at `8d80119`; every `[verified]` below was run there).
Planner: `uas-planner`, 2026-10-11. Every path below was read in a worktree cut from that base. No package edits code: all edit Markdown under `.claude/agents/`, `engineering/plans/` and `specs/`.
Tiers: `verified` = I ran or read it at the path named; `reported` = the spec says it; `asserted` = my judgement.

## Findings the builders must know

1. **Three status copies, not two [verified]:** every spec holds its status in the frontmatter (`status:`), in a body line 12 (`> **Status**: ...`) and in the `specs/README.md` row. D14 and Test row 3 name two. `grep -n '^> \*\*Status' specs/00{40,41,42,43,48}-*.md` prints line 12 in all five. A status move edits all three or the body line goes stale. P4 and P5 edit all three.
2. **Four specs lag, not three [verified, `git log` and PR timelines at `8d80119`]:**

   | Spec | Frontmatter | Body | Registry | Landed |
   |---|---|---|---|---|
   | 0040 | in-progress | in-progress | planned | waves #138, #140 |
   | 0041 | planned | planned | planned | waves #147, #148, #150, #154, #157 |
   | 0043 | in-progress | in-progress | planned | waves #136, #139, #141, #142 |
   | 0048 | planned | planned | planned | waves #152, #153, #156, #159 |

   Test row 3 compares frontmatter with registry, so it prints 0040 and 0043 only. It cannot see 0041 and 0048, where all copies agree on a stale value. Spec 0042 is `in-progress` in all three and every package has landed (finding 4).
3. **Test commands I ran at `8d80119`, all exit 0 [verified, `cargo nextest run ... -j 2` in this worktree]:** `-p uas-pool reconcile` 18 passed; `-p uas-engine lost_slot` 4 passed; `-p uas-pool limits` 39 passed; `-p uas-domain outcome` 4 passed; `cargo test -p uas-sandbox --test escapes` 4 passed; `-p uas-sandbox seccomp` 12 passed; `-p uas-cli conventions` 6 passed. **Green is not proof here:** `limits_live_*` printed `skipped: cannot make a memory+pids cgroup under ... (Device or resource busy)` three times and nextest still reports PASS; `escapes` printed `[netns=false] network: skipped`. The live OOM and pids proof of spec 0041 (REQ-0041-07) did not run on this host. P4 records exit code and skip lines separately.
4. **Plan completion per D13 [verified from the PR commit lists, `gh pr view <n> --json commits`]:**

   | Plan | Landed | Open | D13 reading |
   |---|---|---|---|
   | 0040 | P1, P2, P3 in #138; P4, P5, P6 in #140 (P5 was cut for wave 3 and landed in wave 2) | P7, P8 BLOCKED | done (unblocked all landed) |
   | 0041 | P1, P2, P4 #147; P3 #148; P5 #150; P6 #154; P7 #157 | P8, P9 BLOCKED | done |
   | 0042 | P1, P3, P7 #129; P4 #130; P5, P6 #131; P8 #132 (heading still says BLOCKED); P2 in #119 | none | done in full |
   | 0043 | P1, P5 #136; P2 #139; P3 #141; P4 #142 | none | done in full |
   | 0048 | waves #152, #153, #156, #159 | P13, P17 BLOCKED, and later packages | not done |

   No heading of plans 0040 to 0043 carries the D13 `(DONE <date>, #<PR>)` suffix (`grep -c '^### P.*DONE'` prints 0 for each). Plan 0048 has one, P12.
5. **D13 and D14 let a spec complete with a requirement open [asserted, reading the spec text]:** "done when every unblocked package has landed" makes 0040 and 0041 `complete` once Test exits 0. But plan 0040 P7 holds REQ-0040-12 for production callers (`cgroup_base` must stay `None` until it lands, plan 0040 finding 2), and plan 0041 P8 and P9 hold the live-proof target and the operator pass-through. This plan applies the stricter reading: a spec with an *open* BLOCKED package (one whose work has not landed) stays `in-progress`. A heading that still reads BLOCKED after its work landed is stale text, not an open package: plan 0043 P1 and P2 (lines 57 and 63) landed in #136 and #139, and plan 0042 P8 in #132, so P4 replaces BLOCKED in those headings and checks that none is left. Plans 0040 and 0041 keep real open packages (P7, P8; P8, P9). The spec needs a one-line amendment (see Open).
6. **The critic persona contradicts D6 [verified]:** `.claude/agents/uas-critic.md` line 40 says "Rule 8: a CHANGELOG line when user-visible", so it would flag a builder branch that, under D6, must not carry one. REQ-0050-03 (the critic reports a builder CHANGELOG edit, push or PR as a scope defect) has no home in any persona. The spec's Plan lists three builder files for D6 and omits the critic. P2 adds `uas-critic.md`.
7. **`uas-foreman.md` repeats the old rules [verified]:** line 17 and line 36 (`every builder SHA`) and line 32 (`at most three builders working at once`). The foreman runs only in a pane gang, which spec 0050 does not govern, so no package edits it. It is listed in Open.
8. **Persona edit rights [verified, `uas-engineer.md` lines 16 to 23]:** the engineer writes `specs/` and "the brief files the caller names". The integrator's brief for P1 to P3 must name the `.claude/agents/` and `engineering/plans/` files below, or the engineer refuses them as out of boundary.
9. **Body-line limit [verified]:** `prompt-linter` allows 40 to 110 body lines. Current body counts (`awk '/^---$/{d++;next} d>=2{c++}'`): builder 67, k8s-builder 77, ebpf-builder 70, engineer 71, planner 67, critic 71. Each persona edit adds at most 3 lines.
10. **Test row 1 and row 9 patterns constrain this file [verified, spec Test script]:** every `### P<n>` block holds `Done-when` or `BLOCKED`, and a table row starting `| <number> |` is read as a wave row, so the status table below starts its rows with the word `spec`.
11. **Spec 0052 owns the former P4, and its code is not built [verified at `2a3d442`]:** `git ls-files 'specs/0052-*'` prints `specs/0052-spec-check-rule-set.md`, so the file existing proves nothing. What P5 needs is rule R13 (frontmatter `status` equals the registry Status column, REQ-0052-15), which spec 0052 marks "not built" (line 86): `xtask/src` holds `main.rs`, `pin_check.rs`, `spec_check.rs` and `toolchain.rs`, and no plan 0052 exists in `engineering/plans/`. P5 is therefore BLOCKED on the plan 0052 package that builds R13.
12. **Plan ordering, not a spec cycle [verified, `depends_on` of spec 0052 line 6]:** spec 0052 lists 0050 in `depends_on`, and 0050's P5 waits on a package of plan 0052. The spec graph stays acyclic: 0052 needs 0050's procedure text (P1 to P3), and 0050 reaches `complete` after 0052's check lands. That order is intended.

## Plan items to packages

| Spec 0050 Plan item | Package |
|---|---|
| P1 README: pointer, five cap (line 24) | P1 |
| README: one critic pass on the merged wave branch, D8 exception, verdict form, cleanup after MERGED, conflict rule (the lead's list; D7 to D12) | P1 |
| P2 builder personas drop the CHANGELOG clause; engineer ceiling | P2 (plus the planner size line and the critic, finding 6) |
| P1 `engineering/plans/README.md` shrinks to a pointer | P3 (plan file shape, REQ-0050-01; status-move rule, D14; DONE suffix, D13) |
| P3 status moves of 0040, 0041, 0043 | P4 (adds 0042 and 0048 from the diff, and the D13 suffix) |
| D14: the spec's own status move | P3 (`in-progress`, first wave) and P5 (`complete`) |
| P4 `xtask` spec-check extensions | none: spec 0052's plan |

## Requirement coverage

| Requirement | Closed by |
|---|---|
| REQ-0050-01 | P3 states the shape; this file and Test row 1 are the proof |
| REQ-0050-02 | P1 (step 3 of the README section), P2 (persona verdict blocks already conform) |
| REQ-0050-03 | P2 (critic persona, finding 6) |
| REQ-0050-04 | P1 |
| REQ-0050-05, 06 | P1, P2 (critic persona timing) |
| REQ-0050-07 | P1, P2 |
| REQ-0050-08, 09 | P1 |
| REQ-0050-10, 11 | P1 |
| REQ-0050-12 | P3 states it; P4 and P5 apply it |
| REQ-0050-13 | P1 keeps the hold paragraph |
| REQ-0050-14 | P1, P2 |

14 of 14 requirements are closed by a package, as text. REQ-0050-03, 13 and 14 have no command that can go red (the spec says so in its Test section); the critic reads them by hand. Spec Test rows 4 and 6 to 10 check live repository state and run in P5.

## Waves

| Wave | Packages | Disjoint because | State after the wave |
|---|---|---|---|
| 1 | P1, P2, P3 | P1: `.claude/agents/README.md`. P2: six persona files. P3: `engineering/plans/README.md`, `specs/0050-...md`, `specs/README.md` | The procedure text is current. Spec 0050 is `in-progress`. The integrator adds the one `docs(changelog)` commit |
| 2 | P4 | alone: it edits `specs/README.md` rows 47 to 57, next to the row P3 edits, so a same-wave merge would conflict; it waits for wave 1 | Test row 3 prints no line; five plans carry the D13 suffix |
| 3 | P5 | alone: spec 0050 and its registry row | Spec 0050 is `complete` |

All builders here are `uas-engineer` or `uas-builder`; none is `uas-ebpf-builder`. Wave 1 holds three, inside the cap of three per wave.

## Packages

### P1: the README section and cap (wave 1)
- **Persona:** `uas-engineer` (text edit; finding 8).
- **Files, all exist [verified]:** `.claude/agents/README.md` (line 15 critic row; line 24 cap; lines 36 to 58 "Build handoff"; line 64 the CHANGELOG clause of "Rules every agent carries").
- **Change:** (a) Line 15, the `uas-critic` "Runs when" cell: `once on each merged wave branch before its PR is marked ready, and on a builder SHA that touches unsafe, the sandbox, the pool's kernel calls or crates/uas-ebpf`. (b) Line 24: `The lane runs at most five agents at once (builders, critics and probers together), at most three of them builders in a wave, through the Agent tool: builders in sibling worktrees, then the lane merges them into a wave branch and opens a draft PR, then uas-critic reads the merged branch, then the lane lands on green.` (c) Replace lines 36 to 58 with this text, keeping the hold paragraph of lines 57 and 58 as its last paragraph:

  ```
  ## Build handoff

  The procedure of record is spec 0050 (`specs/0050-build-handoff-pipeline.md`). In short:

  1. `uas-planner` writes `engineering/plans/<spec>.md` from a landed spec: packages with files, a
     done-when, a red-first control, a persona and a wave (at most three builders, disjoint files).
  2. The lane resolves anything BLOCKED and dispatches one wave through the Agent tool, each builder
     in its own worktree cut from fresh `origin/development`. The lane runs at most five agents at
     once, counting builders, critics and probers (Christian, 2026-10-10).
  3. A builder commits signed, never pushes, never edits `CHANGELOG.md`, and returns the verdict
     block: 15 lines or fewer, bad news first, a tier on each claim.
  4. The lane merges the builder branches into one wave branch with merge commits, adds the wave's
     one `docs(changelog)` commit, pushes with `git -C`, and opens the PR as a draft.
  5. `uas-critic` makes one pass on the merged wave branch between draft and ready and returns
     numbered defects. A package that touches `unsafe`, the sandbox, the pool's kernel calls or
     `crates/uas-ebpf` also gets a pass on its builder branch before the merge (D8).
  6. The lane marks the PR ready and lands it with a merge commit pinned to the head when the five
     required checks read SUCCESS. Red or pending stops the wave.
  7. After `gh pr view <n> --json state` reads MERGED, and not before, the lane removes the wave's
     worktrees, runs `git worktree prune`, and deletes the local builder and wave branches.
  8. Conflicts resolve with merge commits. In `CHANGELOG.md` keep every line from both sides; in
     `specs/README.md` keep one row per spec in number order. A scripted resolution drops the whole
     `|||||||` base section (diff3). No conflict marker reaches a pushed commit.
  9. The plan's last package moves the spec status (D14).
  ```
  (d) Line 64: replace `a CHANGELOG line (rule 8)` with `one CHANGELOG line per wave PR, written by the integrator (rule 8)`.
- **Done-when:** `f=.claude/agents/README.md && ! grep -q 'up to three subagents' $f && test "$(grep -c 'at most five agents' $f)" = 2 && ! grep -q 'on every builder SHA' $f && test "$(grep -c 'one .docs(changelog). commit' $f)" = 1 && grep -q 'MERGED, and not before' $f && test "$(grep -c 'diff3' $f)" = 1 && grep -q 'specs/0050-build-handoff-pipeline.md' $f && test "$(awk '/^## Build handoff/{s=1;next} /^## /{s=0} s' $f | wc -l)" -le 34 && ! LC_ALL=C grep -nP '[^\x00-\x7F]' $f && typos $f` exits 0.
- **Red-first:** run it before the edit: it exits 1 at the first leg (`up to three subagents` is on line 24 [verified]). After green, put `up to three subagents` back in a scratch copy and watch the first leg fail; then delete `, and not before` and watch the MERGED leg fail. Restore.

### P2: persona edits (wave 1)
- **Persona:** `uas-engineer`.
- **Files, all exist [verified]:** `.claude/agents/uas-builder.md` (line 3 description; lines 16 and 17), `.claude/agents/uas-k8s-builder.md` (line 3; lines 16 and 17), `.claude/agents/uas-ebpf-builder.md` (lines 16 and 17), `.claude/agents/uas-engineer.md` (line 50), `.claude/agents/uas-planner.md` (line 41), `.claude/agents/uas-critic.md` (line 40; the scope item at line 50).
- **Change:** (a) In the three builders: delete `plus the CHANGELOG line` from the description (builder, k8s-builder) and from the Boundaries paragraph, and delete the `plus one line under ## [Unreleased] ... (AGENTS.md rule 8)` clause; add one sentence to Boundaries: ``You never edit `CHANGELOG.md`: the integrator writes the wave's one line (spec 0050 D6).`` (b) Engineer line 50: `under about 800 changed lines` becomes `target under 400 changed lines, split above 800 (spec 0050 D3)`. (c) Planner line 41: after `under 400 changed lines` add `; a package above 800 is split (spec 0050 D3)`. (d) Critic line 40: replace `Rule 8: a CHANGELOG line when user-visible.` with ``Rule 8: the wave PR carries exactly one `docs(changelog)` commit from the integrator; a builder commit that edits `CHANGELOG.md`, a push or an opened PR is a scope defect (spec 0050 D6, REQ-0050-03).``; at the scope item add `A per-SHA pass is required only for unsafe, sandbox, pool kernel calls and crates/uas-ebpf (spec 0050 D8).` Keep every file at 40 to 110 body lines.
- **Done-when:** `test "$(for f in uas-builder uas-k8s-builder uas-ebpf-builder; do p=.claude/agents/$f.md; test "$(grep -c 'never edit .CHANGELOG.md.' $p)" = 1 || echo "RED $f missing"; grep -Fq 'CHANGELOG line' $p && echo "RED $f old clause"; grep -Fq 'under `## [Unreleased]`' $p && echo "RED $f old clause"; done | wc -l)" = 0 && grep -q 'split above 800' .claude/agents/uas-engineer.md && grep -q 'split above 800\|above 800 is split' .claude/agents/uas-planner.md && grep -q 'docs(changelog)' .claude/agents/uas-critic.md && test "$(for p in .claude/agents/uas-*.md; do n=$(awk '/^---$/{d++;next} d>=2{c++} END{print c}' $p); [ "$n" -ge 40 ] && [ "$n" -le 110 ] || echo "RED $p $n"; done | wc -l)" = 0 && ! LC_ALL=C grep -nP '[^\x00-\x7F]' .claude/agents/*.md && typos .claude/agents` exits 0.
- **Red-first:** before the edit the first `test` prints eight `RED` lines [verified, run at `8d80119`] and fails. After green, restore the old clause in a scratch copy of `uas-k8s-builder.md` and watch `RED uas-k8s-builder old clause` print.

### P3: plans README and the first status move (wave 1)
- **Persona:** `uas-engineer`.
- **Files, all exist [verified]:** `engineering/plans/README.md` (8 lines; rewrite), `specs/0050-build-handoff-pipeline.md` (frontmatter line 2, body line 12), `specs/README.md` (the 0050 row).
- **Change:** README keeps line 1 (the CC-BY-4.0 tag) and holds, in about 30 lines: the pointer to spec 0050; the plan file name; the required sections in order (header with spec, status, base sha, planner, date and the three tiers; `Findings the builders must know`; `Plan items to packages`; `Requirement coverage`; `Waves` with a "Disjoint because" column; one `### P<n>: <title> (wave <k>[, needs ...])` block per package with Persona, Files, Change, Done-when and Red-first, or the word BLOCKED with the spec line that blocks it; `Open`); the D13 rule (a landed package heading gains `(DONE <date>, #<PR>)` and the file stays as the record); the D14 rule (the plan's last package is the status move; `in-progress` with the first landed wave, `complete` when the plan is done and the spec's Test command exits 0, frontmatter, body line and registry row in one commit). Spec 0050: `status: in-progress` in the frontmatter, `in-progress` on body line 12, and `in-progress` in its registry row.
- **Done-when:** `f=engineering/plans/README.md && test "$(for k in 'Findings the builders must know' 'Plan items to packages' 'Requirement coverage' 'Disjoint because' 'Persona' 'Done-when' 'Red-first' 'BLOCKED' 'DONE <date>' 'status move' 'specs/0050-build-handoff-pipeline.md'; do grep -Fq "$k" $f || echo "RED $k"; done | wc -l)" = 0 && head -1 $f | grep -qx '<!-- SPDX-License-Identifier: CC-BY-4.0 -->' && grep -qx 'status: in-progress' specs/0050-build-handoff-pipeline.md && sed -n 12p specs/0050-build-handoff-pipeline.md | grep -q 'Status\*\*: in-progress' && grep '(0050-build-handoff-pipeline.md)' specs/README.md | grep -q in-progress && make spec-check JOBS=2 && make oss-check JOBS=2 && typos engineering specs/0050-build-handoff-pipeline.md` exits 0.
- **Red-first:** before the edit the keyword loop prints eleven `RED` lines [verified, run at `8d80119`] and the `grep -qx 'status: in-progress'` leg fails. After green, delete `status move` from a scratch copy and watch `RED status move` print.

### P4: status moves owed today (wave 2, needs P3 landed)
- **Persona:** `uas-builder`: it runs each Test command and records the result in its verdict.
- **Files, all exist [verified]:** `specs/0040-...md`, `specs/0041-...md`, `specs/0042-...md`, `specs/0043-...md`, `specs/0048-...md` (frontmatter `status:` and body line 12 each), `specs/README.md` (rows 47, 48, 49, 50 and 55 [verified, `grep -n "(004[0-3]-\|(0048-" specs/README.md`]), and `engineering/plans/0040-...md`, `0041`, `0042`, `0043`, `0048` (package headings).
- **Change:** apply the moves below in one commit per spec, subject `docs(spec-<n>): status <new>`. A move to `complete` needs plan done and Test exit 0 in the builder's own run (D14); if the run is red, leave `in-progress`, report PARTIAL and paste the red line. Record exit code and any `skipped:` line for every Test command (finding 3).

  | Spec | Now (frontmatter, body, registry) | Plan done (D13) | Test command | Move |
  |---|---|---|---|---|
  | spec 0040 | in-progress, in-progress, planned | yes, P1 to P6; P7, P8 BLOCKED | `cargo nextest run -p uas-pool reconcile -j 2` and `-p uas-engine lost_slot`: exit 0 at `8d80119` [verified]; the cgroup steps skip off a delegated tree | registry to `in-progress`; stays `in-progress` (finding 5) |
  | spec 0041 | planned, planned, planned | yes, P1 to P7; P8, P9 BLOCKED | `cargo nextest run -p uas-pool limits -j 2` and `-p uas-domain outcome`: exit 0 [verified]; the three `limits_live_*` cases skipped | all three copies to `in-progress`; stays there |
  | spec 0042 | in-progress, in-progress, in-progress | yes, P1 to P8 | the spec's Test block: `./target/debug/uas --version`, `uas nonsense` exits 2, `cargo nextest run -p uas-cli conventions -j 2` (6 passed [verified]); the builder also runs the `uas-ebpfd` rows after `cargo build -p uas-ebpf -j 2` and the dirty-tree `make release-notes` row | all three copies to `complete` if every row is green; tick the six Plan boxes |
  | spec 0043 | in-progress, in-progress, planned | yes, P1 to P5 | `cargo test -p uas-sandbox -j 2 --test escapes` and `cargo nextest run -p uas-sandbox seccomp -j 2`: exit 0 [verified]; the network rows skip off a netns host | all three copies to `complete`; rewrite the fifth Plan box (the `x32_syscall` row is on `development` since commit 651fb70 [verified, `git branch -r --contains`]) to `Landed in PR #155` |
  | spec 0048 | planned, planned, planned | no: P13, P17 BLOCKED, later packages open | none run: the spec's chain stops at its first red leg [reported, spec Test] | all three copies to `in-progress` |

  D13 suffixes: add `(DONE <merge date>, #<PR>)` to the heading of each landed package, the date from `gh pr view <n> --json mergedAt`. Plan 0040: P1, P2, P3 #138; P4, P5, P6 #140. Plan 0041: P1, P2, P4 #147; P3 #148; P5 #150; P6 #154; P7 #157. Plan 0042: P1, P3, P7 #129; P4 #130; P5, P6 #131; P8 #132 (replace `BLOCKED` in its heading); P2 #119. Plan 0043: P1, P5 #136; P2 #139; P3 #141; P4 #142. Plan 0048: derive the packages of #152, #153, #156 and #159 from `gh pr view <n> --json commits` and the `spec 0048 REQ-` references in the subjects; P12 already has its suffix. Replace `BLOCKED` in the headings of the landed packages whose headings still carry it (plan 0043 P1 and P2, plan 0042 P8) with the suffix; leave a heading whose work has not landed (plan 0040 P7, P8; plan 0041 P8, P9; plan 0048 P13, P17) as it is. Plan 0043: the fifth Plan box of spec 0043 is `[x]` already and still says the x32 row is "not yet on `development`"; rewrite that clause to `Landed in PR #155`. Spec 0043 stays `complete` because no open BLOCKED package remains.
- **Done-when:** `test "$(for s in 0040 0041 0042 0043 0048; do a=$(sed -n 's/^status: //p' specs/$s-*.md | head -1); b=$(sed -n '12s/^> \*\*Status\*\*: \([a-z-]*\),.*/\1/p' specs/$s-*.md); c=$(grep "($s-" specs/README.md | awk -F'|' '{gsub(/ /,"",$4); print $4}'); case $s in 0042|0043) w=complete;; *) w=in-progress;; esac; [ "$a $b $c" = "$w $w $w" ] || echo "RED $s: $a $b $c"; done | wc -l)" = 0 && test "$(grep -c '^### P.*(DONE 2026' engineering/plans/0040-*.md)" = 6 && test "$(grep -c '^### P.*(DONE 2026' engineering/plans/0041-*.md)" = 7 && test "$(grep -c '^### P.*(DONE 2026' engineering/plans/0042-*.md)" = 8 && test "$(grep -c '^### P.*(DONE 2026' engineering/plans/0043-*.md)" = 5 && test "$(grep '^### P.*BLOCKED' engineering/plans/0040-*.md engineering/plans/0041-*.md | grep -c DONE)" = 0 && test "$(for f in specs/0*.md; do s=$(basename $f .md); a=$(sed -n 's/^status: //p' $f | head -1); b=$(grep "($s.md)" specs/README.md | awk -F'|' '{gsub(/ /,"",$4); print $4}'); [ "$a" = "$b" ] || echo $s; done | wc -l)" = 0 && test "$(grep -c '^### P.*BLOCKED' engineering/plans/0042-*.md engineering/plans/0043-*.md | awk -F: '{n+=$2} END{print n}')" = 0 && test "$(awk '/^## Plan/,/^## Test/' specs/0042-*.md | grep -c '^- \[ \]')" = 0 && test "$(awk '/^## Plan/,/^## Test/' specs/0043-*.md | grep -c '^- \[ \]')" = 0 && ! grep -q 'not yet on' specs/0043-*.md && make spec-check hygiene JOBS=2` exits 0. The last status loop is spec Test row 3. If the builder leaves 0042 at `in-progress` because a Test row is red, the first `test` leg stays red by design and the verdict is PARTIAL.
- **Red-first:** before the edit the first leg prints five `RED` lines and the row 3 loop prints `0040` and `0043` [verified, run at `8d80119`]. After green, set the 0043 registry cell back to `planned` in a scratch copy and watch both loops print `0043`; restore.

### P5: spec 0050 complete (wave 3, BLOCKED on the plan 0052 package that builds R13; needs P1 to P4 landed)
- **BLOCKED on:** spec 0052 line 86 (R13 "not built") and REQ-0052-15 [reported]. Unblocks when plan 0052's R13 package has landed on `development`, which the first done-when leg proves.
- **Persona:** `uas-builder`: it runs the spec's Test script and records the result.
- **Files, all exist [verified]:** `specs/0050-build-handoff-pipeline.md` (frontmatter, body line 12, the four Plan boxes), `specs/README.md` (the 0050 row), `engineering/plans/0050-build-handoff-pipeline.md` (this file: the D13 suffix on P1 to P5).
- **Change:** run the Test script of spec 0050 verbatim from the repository root with `gh` signed in and paste rows 1 to 10 into the verdict. Tick the Plan boxes P1, P2 and P3. Rewrite the P4 box to `moved to spec 0052 (plan 0052)`. Set `complete` in the three copies. Add the D13 suffix to P1 to P5 in this file. If any row reads RED, stop with PARTIAL.
- **Done-when:** `grep -q 'R13' xtask/src/spec_check.rs && make spec-check JOBS=2 && grep -qx 'status: complete' specs/0050-build-handoff-pipeline.md && sed -n 12p specs/0050-build-handoff-pipeline.md | grep -q 'Status\*\*: complete' && grep '(0050-build-handoff-pipeline.md)' specs/README.md | grep -q complete && test "$(awk '/^## Plan/,/^## Test/' specs/0050-build-handoff-pipeline.md | grep -c '^- \[ \]')" = 0 && test "$(grep -c '^### P.*(DONE 2026' engineering/plans/0050-*.md)" = 5 && make spec-check hygiene JOBS=2` exits 0. Plus the Test script itself: rows 3, 5 and 7 print no RED, row 4 prints `4 required checks ok`.
- **Red-first:** the first leg is red until R13 is built (`grep -c R13 xtask/src/spec_check.rs` prints 0 at `2a3d442` [verified]). After R13 lands, set the 0050 registry cell back to `in-progress` in a scratch copy and watch `make spec-check` fail on the parity rule, which proves the leg runs the check. Also, the `status: complete` leg is red before the edit. After green, tick one box back to `[ ]` in a scratch copy and watch the Plan-box leg fail.

## Open

- **Spec 0052 timing:** P5 waits on plan 0052's R13 package, which does not exist yet (finding 11). Until then spec 0050 stays `in-progress`; waves 1 and 2 are not held by it.
- **Spec amendments this plan needs [asserted]:** (1) D13 and D14 should say `complete` requires every package landed or dropped by a ruling, not "every unblocked package" (finding 5); until then this plan keeps specs with a BLOCKED package at `in-progress`. (2) D14 and Test row 3 should name the body status line and compare all three copies (finding 1), and F10 should list 0041 and 0048 (finding 2). (3) The Plan's D6 item should list `uas-critic.md` (finding 6). (4) The Plan's P1 and P2 name `uas-builder` for Markdown edits; this plan uses `uas-engineer` as the integrator directed.
- **Foreman persona:** `uas-foreman.md` lines 17, 32 and 36 keep "every builder SHA" and "at most three builders". Spec 0050 governs in-lane waves only. A pane-gang decision, not mine.
- **Not run:** the `make release-notes` dirty-tree row and the `uas-ebpfd` rows of spec 0042's Test block, and the live cgroup and network rows of specs 0040, 0041 and 0043 (skipped on this host, finding 3). P4 runs the first two; the live rows need a delegated cgroup tree or `make ebpf-test`.
- **Dates:** the DONE dates in P4 come from `gh pr view`; I did not read `mergedAt` for #138 to #157 individually.
