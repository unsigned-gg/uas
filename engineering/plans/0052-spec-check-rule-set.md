<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plan: 0052-spec-check-rule-set

Spec: `specs/0052-spec-check-rule-set.md` (status planned). Base: `origin/development` `2a3d442`.
Planner: `uas-planner`, 2026-10-11. Every path below was read in a worktree cut from that base.
Tiers: `verified` = I ran or read it at the path named; `reported` = the spec says it; `asserted` = my judgement.

## Findings the builders must know

1. **Two specs mismatch, not three [verified]:** a bash loop comparing `status:` in each `specs/0*.md` frontmatter with the third cell of its `specs/README.md` row prints exactly two lines at `2a3d442`: 0040 (frontmatter `in-progress`, README `planned`) and 0043 (same). Spec 0041 is `planned` in both, so it is not an R13 mismatch (it lags spec 0050 D14, a different rule). Control: the loop prints nothing for the 50 specs that agree. Plan 0050 (branch `origin/plan/spec-0050`, its finding 1) also moves the statuses of 0040 to 0043 and 0048, in three places: frontmatter, body line 12 (`> **Status**:`) and the README row. R13 compares only the first and the last, so the body line stays unchecked.
2. **The keyword fix alone clears nothing [verified by a throwaway script that mirrors the checker, not by the checker]:** with `is_ears_prefix` fixed for `**[REQ-NNNN-NN]** (Pattern):`, the legacy `### Requirements (EARS)` blocks of 19 of the 52 specs still hold 46 bullets that draw the R09 warning. They start with `Every`, `Each`, `After`, `A change`, or a code span (the checker strips code spans before it looks), for example 0033 REQ-01 and 0052 REQ-01. Plus 1 R08 warning: spec 0014 line 57, `it may read`. R07 would raise 0 (every legacy spec has a block with `shall`; only 0024, which is modern, has none). 557 lines with `shall` in all. The spec's "about 290" and "one modal word" were estimates; 46 and 1 are the numbers. 46 one-line rewordings is small enough for one PR (P6). The throwaway script is not the checker: P4 replaces my count with the checker's own, and P6 works from the checker's output, not from my line list.
3. **R12 over plans is nearly inert today [verified]:** the six plan files hold zero relative markdown links (a script with the checker's link logic found 0 targets). Plans cite files in code spans, which R12 skips. P7 lands green, and also protects almost nothing until plan authors use links. The spec's rationale (D7) assumes links.
4. **R14 would warn once on today's plans [verified by script]:** 55 packages in plans 0040 to 0049, 54 hold all four labels or BLOCKED. The one that warns is 0048 P12, a package marked `DONE`. The rule has no DONE exemption; P9 adds one, and P10 states it in the spec (finding for the spec, below).
5. **`repository_specs_pass_check` runs the checker on the real tree [verified, `spec_check.rs` line 1021]:** once R12 over plans and R13 exist, `make test` fails for any branch whose spec status and registry row disagree. So P7 (R13) cannot land before the registry is clean (P2b or plan 0050 P4).
6. **The unit-test README helper has no Status column [verified, `readme()` near line 640]:** it writes `| [name](name) | t |`. R13 needs a `Status` header cell. P7 changes the helper to four columns (`planned` in both fixtures) and makes a missing `Status` header an error, so deleting the column cannot silence R13.
7. **Builders do not write CHANGELOG lines [reported, spec 0050 D6]:** D10 asks for a CHANGELOG line with each rule change. The integrator writes it; no package edits `CHANGELOG.md`.
8. **Nothing to change in `Makefile`, `ci.yml` or `xtask/src/main.rs` [verified]:** `make spec-check` runs `cargo xtask spec-check` (Makefile lines 64 and 65), `make hygiene` includes it, CI runs `make hygiene JOBS=10` (lines 83 and 84). Plans are read by the same command, so CI needs no step. `main.rs` takes the command name only; that is REQ-0052-01's existing behaviour.
9. **Single file, so single-package waves [verified]:** P1, P3, P4, P7 and P9 all edit `xtask/src/spec_check.rs`. Their waves differ. Spec text packages ride beside them in the same wave because they touch other files. An integrator who stacks the five on one branch loses nothing.

## Plan items to packages

| Spec 0052 Plan item | Package |
|---|---|
| Package 1: header comment, pointer, rule-change line | P1 |
| Package 1: point 0001 line 65 and 0002 line 40 at the spec | P2 |
| Package 2: five missing tests | P3 |
| Package 3: keyword parser fix, legacy checks as warnings | P4 |
| Package 3: clear the repository's warnings | P6 (with P5 for spec 0052's own two bullets) |
| Package 3: R07 to an error after a clean week | P11 (BLOCKED) |
| Package 4: R12 over plans | P7 |
| Package 4: R13 status parity | P7, with the registry from P2b or plan 0050 P4 |
| Package 4: amend spec 0050's P4 box | none: plan 0050 P5 owns it (its finding 11) |
| Package 4: R14 plan shape as a warning | P9 |
| D10 amendment of the tables with each rule change | P5, P8, P10 |
| R14 to an error after a clean week | P12 (BLOCKED) |

## Requirement coverage

| Requirement | Closed by |
|---|---|
| REQ-0052-01, 02, 04, 06, 07, 08, 10, 11, 12 | no package: existing behaviour with existing tests; the gate in every wave re-runs them (Test rows 1, 2, 4, 6 to 8, 10 to 12 by hand) |
| REQ-0052-03, 05, 09 | P3 (tests), P4 (the legacy extension and the parser fix) |
| REQ-0052-13 | P1 (the line in the header), P5, P8, P10 (the tables) |
| REQ-0052-14 | P1 |
| REQ-0052-15 | P2b, P7, P8 |
| REQ-0052-16 | P9, P10 |
| REQ-0052-17 | P7, P8 |

8 of 17 requirements are closed by a package (03, 05, 09, 13, 14, 15, 16, 17); the other nine are existing behaviour that no package changes. P11 and P12 close nothing until a date is set.

## Waves

| Wave | Packages | Disjoint because | State after the wave |
|---|---|---|---|
| 1 | P1, P2, P2b | P1: `xtask/src/spec_check.rs`. P2: `specs/0001`, `specs/0002`. P2b: `specs/README.md` | Header points at the spec; registry clean |
| 2 | P3 | `xtask/src/spec_check.rs` | Five checks have a test |
| 3 | P4, P5 | P4: `xtask/src/spec_check.rs`. P5: `specs/0052` | Legacy blocks draw warnings; about 49 warnings print |
| 4 | P6, P7, P8 | P6: 18 other specs. P7: `xtask/src/spec_check.rs`. P8: `specs/0052` | Warnings back to the two length warnings; R12 over plans and R13 live |
| 5 | P9, P10 | P9: `xtask/src/spec_check.rs`. P10: `specs/0052` | R14 live as a warning |

No check lands red against the tree at its wave: P4 only warns; P7's two errors find 0 broken plan links and 0 mismatches once P2b lands; P9 only warns (one warning on 0048 P12 until its DONE exemption is in, which P9 includes). Run order matters only inside a wave if the integrator stacks branches.

## Packages

### P1: header, comment numbers, rule-change line (wave 1)
- **Persona:** `uas-builder`.
- **Files, all exist [verified]:** `xtask/src/spec_check.rs` (header lines 2 to 19; the `// Rule 1:` comment line 88; the doc comments at lines 564 and 580).
- **Change:** replace the numbered list in the header with a pointer to spec 0052 and twelve lines `//! R01 file names` to `//! R12 relative links`, one per rule name, using the spec's IDs. Add the procedure line `//! Changing a rule: amend the spec 0052 tables, add a unit test that fails without the change.` Change the three `Rule N:` comments to `R01`, `R12`, `R11` in that order (the old `Rule 8` is links, now R12; the old `Rule 7` is the README, now R11). No code change.
- **Done-when:** `test "$(grep -c 'Rule [0-9]*:' xtask/src/spec_check.rs)" = 0 && test "$(grep -c '^//! R[0-9][0-9] ' xtask/src/spec_check.rs)" = 12 && test "$(grep -c 'unit test that fails without the change' xtask/src/spec_check.rs)" = 1 && test "$(grep -c 'spec 0052' xtask/src/spec_check.rs)" -ge 1 && make fmt-check clippy JOBS=2 && cargo nextest run -p xtask -j 2` exits 0. This is Test row REQ-0052-14 plus the twelve-line and procedure checks.
- **Red-first:** the first `test` prints 3 at `2a3d442` and fails. After the edit, put one `Rule 8:` back and watch it fail again; restore.

### P2: point specs 0001 and 0002 at the rule set (wave 1)
- **Persona:** `uas-engineer`.
- **Files, all exist [verified]:** `specs/0001-cas-architecture-and-crate-layout.md` line 65 (`xtask/` also exists, it holds `spec-check`), `specs/0002-verification-gates-and-evidence.md` line 40 (the `make hygiene` row). Keep each file's line count: 0001 and 0002 are far under 300.
- **Change:** add to each line a link `[spec 0052](0052-spec-check-rule-set.md)` as the owner of the rule list. Do not touch the `make hygiene` target list in 0002 (spec 0052 Notes: not fixed here).
- **Done-when:** `test "$(grep -l '0052-spec-check-rule-set' specs/0001-*.md specs/0002-*.md | wc -l)" = 2 && make spec-check JOBS=2` exits 0. The link must resolve, so R12 backs the second leg.
- **Red-first:** the first leg counts 0 before the edit. After it, break the link target in one file (`0052-spec-check-rule-sett.md`) and watch `make spec-check` print `broken link` and exit 1; restore.

### P2b: registry rows for 0040 and 0043 (wave 1, conditional)
- **Persona:** `uas-engineer`.
- **Precheck:** run the loop in the done-when. If it prints 0 lines, plan 0050 P4 has landed; skip this package and say so. Plan 0050 P4 owns the full status move (all three copies).
- **Files, exist [verified]:** `specs/README.md` rows 0040 and 0043 (Status cell, third column).
- **Change:** set both Status cells to `in-progress`, the value the frontmatter holds [asserted: the waves landed, so the frontmatter is the truth; the third copy, body line 12, is plan 0050's].
- **Done-when:** `bash -c 'for f in specs/0*.md; do s=$(awk "NR>1&&/^---/{exit} /^status:/{print \$2}" "$f"); r=$(grep -F "($(basename "$f"))" specs/README.md | awk -F"|" "{gsub(/ /,\"\",\$4);print \$4}"); [ "$s" = "$r" ] || echo "$f"; done | wc -l'` prints `0`, then `make spec-check JOBS=2` exits 0.
- **Red-first:** the loop prints 2 before the edit. After it, set one cell back and watch it print 1; restore.

### P3: the five missing tests (wave 2)
- **Persona:** `uas-builder`.
- **Files, exist [verified]:** `xtask/src/spec_check.rs`, test module only (after line 686). Use the `Tmp`, `spec` and `readme` helpers already there.
- **Change:** five tests named `keyword_warning_on_non_ears_bullet`, `duplicate_key_fails`, `unclosed_frontmatter_fails`, `list_and_scalar_type_errors` and `missing_h1_fails`. Each asserts the exact message stem from spec 0052's table (R09, R02, R02, R02, R04), and the first asserts the warning text, not only the count.
- **Done-when:** `test "$(cargo nextest run -p xtask --color never -j 2 -E 'test(/keyword_warning_on|duplicate_key_fails|unclosed_frontmatter|list_and_scalar|missing_h1_fails/)' 2>&1 | grep -c '^ *PASS ')" = 5 && cargo nextest run -p xtask -j 2` exits 0. The filter style and the PASS count were run on two existing tests and printed 2 [verified].
- **Red-first:** for each test, comment out the `report.errors.push` (or warnings push) it covers and watch that test, and only that test, fail; restore all five. Spec Test rows REQ-0052-03, 05 and 09 name the plants.

### P4: keyword parser fix and the legacy EARS block (wave 3)
- **Persona:** `uas-builder`.
- **Files, exist [verified]:** `xtask/src/spec_check.rs` (`is_ears_prefix` near line 402, `check_ears_requirements` line 437, `check_spec` line 280, tests).
- **Change, parser:** after the bold label is stripped, also strip a leading parenthesis `(Pattern)` and an optional `:`. The bullet `- **[REQ-0099-01]** (Ubiquitous): The system shall run.` then draws no R09 warning; `- **[REQ-0099-01]** (Ubiquitous): Systems shall run.` still does.
- **Change, legacy:** in a spec without an H2 `Requirements`, find the H3 `### Requirements (EARS)` outside fences, take the lines up to the next H2 or H3, and call the same function with a `legacy` flag. In legacy, R07's missing-`shall` result is a warning, R08 and R09 are as in modern. A legacy spec without the H3 draws nothing. Message stems stay as spec 0052's table shows.
- **Report in the verdict:** the checker's own count of the new warnings by rule (mine are 46 R09 and 1 R08, from a script).
- **Done-when:** `cargo nextest run -p xtask --color never -j 2 -E 'test(/house_form|legacy_ears/)' && make spec-check JOBS=2 && test "$(cargo xtask spec-check 2>/dev/null | grep -c '^warning:')" -ge 3` exits 0. The `-ge 3` leg is red before the change (2 length warnings) and goes green once the legacy blocks speak. The unit tests plant: the house bullet in a modern tree (no warning), `Systems shall run.` and `will` for `shall` and `may` in a legacy tree (a warning each, exit 0), and a backticked `may` (none).
- **Red-first:** run the tests before the parser edit: the house-form test fails. Revert only the legacy call and watch the legacy tests fail.

### P5: spec 0052 amendment for P4, and its own two bullets (wave 3)
- **Persona:** `uas-engineer`.
- **Files, exist:** `specs/0052-spec-check-rule-set.md` only. The file is 215 lines.
- **Change:** R07, R08 and R09 Mode becomes `all (legacy: block, warning)`; the sentence "R07 to R09 skip fenced lines" gains the legacy block; replace F6's "about 290" with the checker's count from P4's verdict; rewrite REQ-0052-01 and REQ-0052-13 so they open with `The` (the two bullets in this file that R09 will warn on, finding 2).
- **Done-when:** `make spec-check JOBS=2 2>&1 | grep '^warning: specs/0052' | wc -l` prints `0`, and `make spec-check JOBS=2` exits 0.
- **Red-first:** after P4 landed, the count is 2 before the edit.

### P6: clear the legacy warnings in the other specs (wave 4)
- **Persona:** `uas-engineer`.
- **Files, exist [verified]:** the specs the checker names. At `2a3d442` the mirror script lists 0009, 0014, 0019, 0021, 0022, 0023, 0025, 0026, 0033, 0039, 0042, 0044, 0045, 0047, 0048, 0049, 0050, 0053 (18 files; 0052 is P5). Re-derive the list from `cargo xtask spec-check` output at dispatch; some of these have open spec PRs, so fetch first.
- **Change:** reword only the opening words of each flagged bullet to begin with The, When, While, If or Where, and the `may` in 0014 line 57 to `can`. No change of meaning, no change of requirement ID. About 47 lines change, 94 in the diff.
- **Done-when:** `test "$(make spec-check JOBS=2 2>&1 | grep '^warning:' | grep -vc 'lines is over')" = 0 && make spec-check JOBS=2` exits 0. Only the length warnings (0004, 0025) may remain.
- **Red-first:** the count is 47 before the edit (as measured by the checker in P4; re-measure).

### P7: R12 over plans and R13 status parity (wave 4)
- **Persona:** `uas-builder`.
- **Needs:** P2b done or plan 0050 P4 landed (finding 5).
- **Files, exist [verified]:** `xtask/src/spec_check.rs` (`check` line 69, `check_links` line 565, `check_spec`, `check_readme` line 581, the `readme()` helper).
- **Change, R12:** after the README, for each `engineering/plans/*.md` call `check_links` with `root/engineering/plans` as the base. A missing directory is skipped. Messages read `engineering/plans/<file>:<line>: broken link `<target>``.
- **Change, R13:** `check_spec` also returns the status and its line; `check_readme` reads the `Status` header cell, and for each spec row compares. Message: `specs/<file>:<line>: status `in-progress` differs from the specs/README.md row `planned``. A README table with no `Status` header is an error `specs/README.md:<line>: no Status column`. Update the `readme()` helper to four columns.
- **Done-when:** `cargo nextest run -p xtask --color never -j 2 -E 'test(/plan_links|status_parity/)' && make spec-check JOBS=2` exits 0. Tests plant: `[x](nope.md)` in a plan (error), `[x](../../specs/0001-cas-architecture-and-crate-layout.md)` (passes), a README row `in-progress` against frontmatter `planned` (error naming both), a README with no `Status` column (error), and the unplanted tree (exit 0 with the existing tests).
- **Red-first:** run both new tests before the code; they fail. With R13 in place, revert P2b's rows and watch `make spec-check` print the two lines and exit 1 (the same two the loop names).

### P8: spec 0052 amendment for R12 over plans and R13 (wave 4)
- **Persona:** `uas-engineer`.
- **Files, exist:** `specs/0052-spec-check-rule-set.md`.
- **Change:** table R13 gets Mode `README`, severity error, the message of P7; R12 gets "and `engineering/plans/*.md`"; add the "no `Status` column" message; add a Notes line that body line 12 is unchecked (finding 1) and that plans cite files in code spans (finding 3). Keep the file under 300 lines.
- **Done-when:** `make spec-check JOBS=2` exits 0 and `grep -c 'differs from the specs/README.md row' specs/0052-spec-check-rule-set.md` prints at least 1.
- **Red-first:** the grep prints 0 before the edit.

### P9: R14 plan package shape as a warning (wave 5)
- **Persona:** `uas-builder`.
- **Files, exist:** `xtask/src/spec_check.rs`.
- **Change:** for each `engineering/plans/*.md` other than `README.md`, split at `### P<n>` headings outside fences. A package passes if its section holds the word BLOCKED, or the word DONE in its heading, or all four labels `**Persona`, `**Files`, `**Done-when`, `**Red-first` (case-insensitive, text may follow inside the bold). Otherwise: `warning: engineering/plans/<file>:<line>: package P12 lacks Done-when`, one per package naming the first missing label. Spec 0052 R14 says "the spec line that blocks it"; the check looks for the word, as spec 0050 Test row 1 does [asserted].
- **Done-when:** `cargo nextest run -p xtask --color never -j 2 -E 'test(/plan_shape/)' && test "$(cargo xtask spec-check 2>/dev/null | grep -c 'engineering/plans')" = 0` exits 0. Tests plant a package without Done-when (a warning), the same with BLOCKED (none), a DONE heading (none). The second leg proves today's six plans stay clean.
- **Red-first:** the plant test fails before the code. Strip `Done-when` from a copy of plan 0049 and run the checker on the copy: it warns once per package.

### P10: spec 0052 amendment for R14 (wave 5)
- **Persona:** `uas-engineer`.
- **Files, exist:** `specs/0052-spec-check-rule-set.md`.
- **Change:** R14 row becomes built with the message of P9 and the DONE exemption; D12 and REQ-0052-16 gain the DONE clause.
- **Done-when:** `make spec-check JOBS=2` exits 0 and `grep -c 'DONE' specs/0052-spec-check-rule-set.md` prints at least 1.
- **Red-first:** the grep prints 0 before the edit.

### P11: promote R07 to an error in legacy (BLOCKED)
- BLOCKED on spec 0052 Plan, Package 3, third box (line 157) and D5 step 3: the lane sets the date after one clean week. The week starts when P6 lands with zero non-length warnings. Unblocks with the date. Then one `uas-builder` package flips R07's legacy severity, with a test that plants `will` for `shall` and expects exit 1.

### P12: promote R14 to an error (BLOCKED)
- BLOCKED on spec 0052 D12 (line 120): "an error on a date the lane sets after one clean week". Same shape as P11.

## Open

- **Registry versus plan 0050 P4:** if plan 0050 P4 lands first, P2b is skipped; if P2b lands first, plan 0050 P4 edits the README rows again, harmlessly.
- **Counts:** 46 and 1 come from a script that mirrors the checker; P4's verdict replaces them.
- **I ran no package.** Paths, line numbers and counts were read at `2a3d442`; the P3 filter style was run on existing tests only.
