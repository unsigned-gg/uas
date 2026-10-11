<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plan: 0056-agent-roster-and-tooling

Spec: `specs/0056-agent-roster-and-tooling.md` (status planned). Base: `origin/development` `1406339`. Planner: `uas-planner`, 2026-10-11.
Every path below was read in a worktree cut from that base, and both fenced Test commands of the spec were run there.
Tiers: `verified` = I ran or read it at the path or command named; `reported` = the spec says it; `asserted` = my judgement.

## Findings the builders must know

1. **Both spec commands behave as the spec says [verified]:** the first fenced block (the `tooling-check` body) prints `OK` and exits 0 on the base. The second (the `hygiene` gap check) prints `missing oss-check`, `missing spdx-check`, `missing hook-test`. Extract either with `awk -v n=1 '/^```bash/{c++;p=1;next} /^```/{p=0} p&&c==n' specs/0056-agent-roster-and-tooling.md` (n=2 for the gap check).
2. **The block ports to a Makefile recipe with one change [verified]:** I wrote it into a scratch Makefile with `SHELL := bash`, `.SHELLFLAGS := -euo pipefail -c` and `.ONESHELL:` (the repo's own settings), doubled every `$` to `$$`, indented with a tab, and replaced the last line `[ "$bad" = 0 ] && echo OK` with `exit $$bad`. `make -s` exited 0 with no output on the real tree. The block is 48 lines, not "about 40" (D6); that is fine. Keep a final `echo "tooling-check: OK"` after the exit test so a pass is visible. Run directly under `bash -euo pipefail` it also exits 0 five times in a row.
3. **A `grep -q` fed by `echo` is a flake risk under `pipefail` [verified, Makefile comment on `smoke`]:** the repo already records a 1-in-4 SIGPIPE failure of exactly this shape. The block has four such lines (`echo "$ph" | grep -qx`, `echo "$tbl" | grep -qF`, `echo "$t" | grep -qE`). The inputs are small and the five runs above did not flake, but write them as `grep -q ... <<< "$x"` in the recipe. The semantics are identical, so the planted tests are unchanged.
4. **`hygiene` will have nine members, not eight [verified + reasoned]:** `Makefile` line 49 has eight today (`typos machete spec-check toolchain-check pin-check oss-check spdx-check hook-test`). D5 adds `tooling-check`. The spec's P3 and D9 say "the Makefile's 8", and the gap check reads the live `hygiene:` line. So the `AGENTS.md` and spec 0002 rows must list nine, and they land in the same wave as the gate, not before it. P5 below does that.
5. **`text-check` is not a `hygiene` member and must stay out [verified, Makefile comment above `text-check`]:** it needs a base ref that a push has not. The gap check does not ask for it. Do not add it to the rows.
6. **The planted-defect tests have no home in the spec [verified]:** spec P1 says "one planted-defect test per row" and the Test section says to copy the tree with `git archive HEAD`. `git archive HEAD` copies the committed tree, so a builder with uncommitted edits would test the old Makefile. The `hook-test` target runs `.claude/hooks/tests/test-*.sh`, and any extra script there runs too (Makefile `hook-test` recipe, spec 0051 REQ-0051-13 [reported]). So P1 puts the tests in a new `.claude/hooks/tests/test-tooling-check.sh`: they then run inside `hygiene` for free and no extra target is needed. Shell files must carry `SPDX-License-Identifier: Apache-2.0 OR MIT` in the first two lines (`spdx-check`; the existing tests show the shape).
7. **What the recipe reads [verified, by reading the block]:** `Makefile`, `.claude/agents/`, `.claude/skills/` (symlinks), `.agents/skills/`, `.saga/teams.toml`, `specs/` (the owner check lists `specs/NNNN-*.md`) and `specs/0056-agent-roster-and-tooling.md` itself, for the roster and the Make surface tables. A scratch copy for a plant needs exactly those five paths. The recipe finds the Make table by the header text `| Targets | Owner`; renaming that header makes every target fail `no row`, which is loud, not silent.
8. **Concurrent Makefile edits [verified]:** the 0053 plan (P6) also edits `Makefile` (the `text-check` target exists already) and `ci.yml`. Merge with merge commits and keep both sides. P1 adds its target after `hook-test` and P4 touches only the `hygiene:` line, so conflicts should be line-local.
9. **The persona files need no edit [verified]:** Test block one prints `OK`, which covers frontmatter keys, body lines (58 to 77), ASCII, heading order, `Forbidden`, the 15-line cap, tools, haiku-has-no-write, a README row, a roster row and the exact tools text of each of the nine files. The model column of the roster matches each file's `model:` line (I read all nine). No persona edit is needed, so the package list has none.
10. **The Make-table owner column is already complete [verified]:** the same block checks every target has a row, every owner cell is `This` or names spec numbers that exist, and passes. No package edits the table. P1 enforces it from now on, and the one new target (`tooling-check`) already has its row.
11. **Stale label outside the spec's reach [verified]:** `.github/workflows/ci.yml` line 84 names the step "make hygiene (typos, machete, spec-check, toolchain-check)", four of nine. D9 does not list it. Not in a package (see Open).
12. **The linter's banned-string list does not exist anywhere [verified]:** `find` for `agents.test.sh` under the paas checkout (depth 3) finds nothing; its `tests` directory holds five fixture directories. This matches F5. P7 is BLOCKED on it.

## Plan items to packages

| Spec 0056 Plan item | Package |
|---|---|
| P1 `tooling-check` recipe with planted tests, in `.PHONY` and in `hygiene` | P1 (recipe, `.PHONY`, tests, not in `hygiene`), then P4 (the gate) |
| P2 linter step in `.claude/agents/README.md` | P2 |
| P3 `hygiene` rows and the two amendments | P5 |
| P4 status moves | P3 (`in-progress`), P6 (`complete`) |
| Not in the spec's Plan: the optional banned-string variable (D6) | P7, BLOCKED |

## Requirement coverage

| Requirement | Closed by |
|---|---|
| REQ-0056-01, 02, 03, 04 | P1 (the persona checks and their plants) |
| REQ-0056-05 | P1 (README row, roster row, tools text) |
| REQ-0056-06 | P1 |
| REQ-0056-07 | P2 (process rule: no command; the reviewer reads the PR body for a `TOTALS:` line) |
| REQ-0056-08 | P1 (target exists), P4 (member of `hygiene`) |
| REQ-0056-09 | P1 |
| REQ-0056-10 | P1 |
| REQ-0056-11 | P1 (the same check seen from the PR side: a PR that adds a target without its row cannot go green once P4 lands) |
| REQ-0056-12 | P5 |
| REQ-0056-13 | P1 (already true; the check keeps it true) |

13 of 13 requirements are closed by a package. REQ-0056-07 has no failing command and stays at evidence tier `asserted` until a PR changes a persona file and carries the line.

## Waves

| Wave | Packages | Disjoint because | State after the wave |
|---|---|---|---|
| first | P1, P2, P3 | P1: `Makefile`, `.claude/hooks/tests/test-tooling-check.sh` (new). P2: `.claude/agents/README.md`. P3: `specs/0056-agent-roster-and-tooling.md`, `specs/README.md` | `make tooling-check` exists, passes on the tree and is tested by plants inside `hook-test`. It is not yet in `hygiene`. The linter step is documented. The spec reads `in-progress` |
| second | P4, P5 | P4: `Makefile` (the `hygiene:` line only). P5: `AGENTS.md`, `specs/0002-verification-gates-and-evidence.md`, `specs/0052-spec-check-rule-set.md` | `hygiene` runs the check; the three docs name nine members; the gap check prints nothing |
| third | P6 | `specs/0056-agent-roster-and-tooling.md`, `specs/README.md` | Spec reads `complete` |
| later | P7 | not builder work; BLOCKED | none |

Ordering for "`hygiene` never lands red": the first wave lands the recipe and its plants on the real tree with the check outside `hygiene`. The integrator waits for that PR to merge and for `development` to be green, then dispatches the second wave. P4 adds the member only after the recipe has run green on the merged tree. P1 cannot make `hygiene` red on its own because `hook-test` already runs its test script and the script passes on the tree (it is the first live proof). One standing risk: after P4, any open PR that edits a persona tool line, adds a target or adds a skill without its companion row goes red on `hygiene` until it merges `development`. The integrator checks `gh pr list --base development` for such PRs before dispatching wave two [asserted: I did not list open PRs].

Each of P1, P4 and P5 changes a tracked file that CI reads, so each needs a CHANGELOG line. The integrator writes it as one `docs(changelog)` commit per wave PR (spec 0050 D6). Builders write none.

## Packages

### P1: `tooling-check` recipe and its planted tests (wave first)
- **Persona:** `uas-builder` (a shell recipe and a shell test; no Rust).
- **Files:** `Makefile` (exists, 13.8K; `.PHONY` is lines 15 to 17, `hook-test` follows `hygiene`), `.claude/hooks/tests/test-tooling-check.sh` (new).
- **Change:** (a) Add `tooling-check` to `.PHONY` and define it after `hook-test`, with the help text `## Persona files, skills, Make targets and team personas parity (spec 0056)` and a comment line naming spec 0056 D4 to D6. The recipe is the spec's first fenced block, ported as finding 2 describes, with `<<<` in place of the three `echo | grep -q` lines (finding 3). Do NOT add it to the `hygiene:` line; P4 does. (b) Write the test script in the shape of `test-lint-on-edit.sh` (shebang, SPDX line, `set -uo pipefail`, `mktemp -d` and a trap). A helper copies `Makefile .claude .agents .saga specs` from the working tree into a fresh scratch directory with `cp -a`, applies one plant, runs `make -s -C "$S" tooling-check`, and asserts a non-zero exit and the expected `FAIL ...` substring in the output. One control: the unplanted copy exits 0. The plants are the rows of the spec Test table for REQ-0056-01 to 06, 09, 10 and 13 (every plant the table lists; the table gives the planted edit and the expected text). REQ-0056-07 and 11 have no plant. The `hygiene` membership plant belongs to P4. The script prints `ok` or `RED: <row>` per plant and exits 1 on any red.
- **Done-when:** `make tooling-check JOBS=2 && bash .claude/hooks/tests/test-tooling-check.sh >/dev/null && ! make -n hygiene JOBS=2 | grep -q 'FAIL frontmatter key' && test "$(bash .claude/hooks/tests/test-tooling-check.sh | grep -c '^ok')" = 24` exits 0. The `! make -n` leg proves the check is not yet a `hygiene` member. The count leg guards against a plant that silently stopped running: the spec Test table lists 23 plants (REQ-01: 3, 02: 3, 03: 2, 04: 3, 05: 2, 06: 1, 09: 3, 10: 4, 13: 2) plus the one control, so 24 `ok` lines. If the builder splits or merges a plant, the number changes in the commit body and in this command together.
- **Red-first:** run `make tooling-check` before the Makefile edit: `make: *** No rule to make target 'tooling-check'`. After green, break it twice: delete the `Forbidden` check from the recipe and watch the heading-order plant row print `RED`; then delete the `.PHONY` entry for the new target and watch the control row fail with `FAIL not in .PHONY: tooling-check`. Restore both.

### P2: record the linter step in the agents README (wave first)
- **Persona:** `uas-engineer` (text).
- **Files:** `.claude/agents/README.md` (exists; the `## Lint` section is the last, lines 73 to 79).
- **Change:** extend `## Lint` to say, in short sentences: (a) `make tooling-check` runs rules E1 to E6 of spec 0056 on every persona, skill and Make target, as part of `make hygiene`; (b) `prompt-linter` (global, no shell, so CI cannot run it) covers E7 and runs on request; (c) a PR that changes a file under `.claude/agents/` carries the linter's `TOTALS:` line in its body, one line per changed file (REQ-0056-07), and the reviewer reads the PR body for it; (d) one link to `specs/0056-agent-roster-and-tooling.md`. Do not change the table: its rows are checked by `tooling-check` and the roster. Add no new `##` heading (the existing section is enough). ASCII only.
- **Done-when:** `test "$(git diff --name-only origin/development)" = .claude/agents/README.md && grep -q 'TOTALS' .claude/agents/README.md && grep -q 'tooling-check' .claude/agents/README.md && grep -q 'specs/0056-agent-roster-and-tooling' .claude/agents/README.md && ! LC_ALL=C grep -qP '[^\x00-\x7F]' .claude/agents/README.md && make typos oss-check JOBS=2` exits 0. The first leg proves nothing else changed; the ASCII leg and `oss-check` guard the file.
- **Red-first:** the three `grep -q` legs fail before the edit (`grep -c TOTALS` prints 0 [asserted: `git grep -c TOTALS -- .claude/agents/README.md` prints nothing at the base; builder confirms]). After green, remove the `TOTALS` sentence in a scratch copy and watch the second leg fail; restore.

### P3: spec status to `in-progress` (wave first)
- **Persona:** `uas-engineer`.
- **Files:** `specs/0056-agent-roster-and-tooling.md` (frontmatter `status:` line 2 and the body line `> **Status**: planned`), `specs/README.md` (the 0056 row, line 61).
- **Change:** `planned` to `in-progress` in all three places in one commit, per spec 0050 D14. Change no other line.
- **Done-when:** `grep -qx 'status: in-progress' specs/0056-agent-roster-and-tooling.md && grep -q '\*\*Status\*\*: in-progress' specs/0056-agent-roster-and-tooling.md && grep -qF '| [0056](0056-agent-roster-and-tooling.md) | Agent roster and tooling surface | in-progress | medium |' specs/README.md && test "$(git diff --name-only origin/development | sort | tr '\n' ' ')" = 'specs/0056-agent-roster-and-tooling.md specs/README.md ' && make spec-check JOBS=2` exits 0.
- **Red-first:** each `grep` leg fails on the base (the file says `planned`). After green, revert the registry row alone in a scratch copy and watch the third leg fail.

### P4: make `tooling-check` a `hygiene` member (wave second, needs P1 merged and green)
- **Persona:** `uas-builder`.
- **Files:** `Makefile` (exists; the `hygiene:` line, line 49 on the base, and its `##` text). No other line.
- **Change:** append `tooling-check` as the last prerequisite of `hygiene` and extend its help text to name it. Do not touch the recipe.
- **Done-when:** `sed -n 's/^hygiene: \(.*\) ##.*/\1/p' Makefile | grep -qw tooling-check && make -n hygiene JOBS=2 | grep -q 'FAIL frontmatter key' && make hygiene JOBS=2` exits 0. The `make -n` leg is REQ-0056-08's plant read from the other side: the recipe text appears under `hygiene`. The last leg runs the whole gate, including P1's test script, on the merged tree.
- **Red-first:** the first two legs fail before the edit. After green, remove `tooling-check` from the line in a scratch copy and watch the `make -n` leg fail (the spec's REQ-0056-08 plant); then plant `owner: x` in a persona file in a scratch copy and watch `make hygiene` exit non-zero with `FAIL frontmatter key owner`. Restore.

### P5: correct the `hygiene` rows and the two amendments (wave second, needs P1 merged; lands with P4)
- **Persona:** `uas-engineer` (text and spec amendments).
- **Files:** `AGENTS.md` (row at line 56), `specs/0002-verification-gates-and-evidence.md` (row at line 40, note at lines 104 and 105), `specs/0052-spec-check-rule-set.md` (note at lines 228 and 229). All exist.
- **Change:** (a) `AGENTS.md` line 56 and spec 0002 line 40 list all nine members in the Makefile's order: `typos machete spec-check toolchain-check pin-check oss-check spdx-check hook-test tooling-check` (backticks per the row's existing style; keep spec 0002's link to spec 0052 for the `spec-check` rules). Nine, not the eight the spec's D9 says: finding 4. (b) Spec 0002 lines 104 and 105: the "Amended" note says spec 0052 Notes own the drift; change it to say spec 0056 D9 owns it and the row is corrected (amendment 1 of 2, dated 2026-10-11). (c) Spec 0052 lines 228 and 229: the note says "Not fixed here; a spec 0002 amendment"; change it to say spec 0056 D9 took the fix over and that it landed (amendment 2 of 2). These are the two spec amendments D9 and the spec Notes require. Edit no other line of either spec. Cite no new requirement ID.
- **Done-when:** `test -z "$(for m in $(sed -n 's/^hygiene: \(.*\) ##.*/\1/p' Makefile); do grep '`make hygiene`' AGENTS.md | grep -qF "\`$m\`" || echo "missing $m"; done)" && grep -q 'spec 0056' specs/0002-verification-gates-and-evidence.md && grep -q 'spec 0056' specs/0052-spec-check-rule-set.md && test "$(git diff --name-only origin/development | sort | tr '\n' ' ')" = 'AGENTS.md specs/0002-verification-gates-and-evidence.md specs/0052-spec-check-rule-set.md ' && make spec-check typos oss-check JOBS=2` exits 0. The first leg is the spec's gap check (the second fenced block) wrapped so that any `missing` line fails it. Run it from the merged tree where the `hygiene:` line already holds `tooling-check` (after P4), or P5 will not see the ninth member.
- **Red-first:** the first leg prints three `missing` lines on the base (finding 1), and after P4 alone it prints four, with `missing tooling-check` added. After green, drop `hook-test` from the `AGENTS.md` row in a scratch copy and watch it print `missing hook-test`; restore.

### P6: spec status to `complete` (wave third, needs P4 and P5 merged)
- **Persona:** `uas-engineer`.
- **Files:** `specs/0056-agent-roster-and-tooling.md`, `specs/README.md`.
- **Change:** `in-progress` to `complete` in the frontmatter, the body line and the registry row, in one commit (spec 0050 D14). Tick the four Plan checkboxes in the spec. Change no other line.
- **Done-when:** `awk -v n=1 '/^```bash/{c++;p=1;next} /^```/{p=0} p&&c==n' specs/0056-agent-roster-and-tooling.md | bash | grep -qx OK && test -z "$(awk -v n=2 '/^```bash/{c++;p=1;next} /^```/{p=0} p&&c==n' specs/0056-agent-roster-and-tooling.md | bash)" && grep -qx 'status: complete' specs/0056-agent-roster-and-tooling.md && grep -qF '| [0056](0056-agent-roster-and-tooling.md) | Agent roster and tooling surface | complete | medium |' specs/README.md && make hygiene JOBS=2` exits 0, run on `development` after P4 and P5 merged. This is the spec's own Test command (D14 of spec 0050): both fenced blocks, the first printing `OK` and the second printing nothing.
- **Red-first:** with the status still `in-progress`, the third leg fails. Re-run the second block on the base before P5 and see its three `missing` lines, so the leg is shown able to go red.

### P7: banned-string variable for the recipe (BLOCKED)
- **BLOCKED on the paas lane [reported: spec F5, D6, Notes "Open, for another lane"; verified: finding 12].** The linter's check F6 reads banned strings from `tests/agents.test.sh`, and that file is in neither this repo nor the paas checkout. No list exists to copy, and a list invented here would be a rule nobody ruled.
- **Unblocks when:** the paas lane publishes the file or a list of strings. Then a `uas-builder` package adds them to `tooling-check` as a `BANNED` variable (D6) with one plant; its Done-when would be `make tooling-check` exits 0 on the tree and exits non-zero on a scratch copy with one banned string planted in a persona file.
- **Not blocking:** nothing else in this plan waits on it. The spec's Notes say the same.

## Open

- **The `ci.yml` step label (finding 11):** line 84 names four of nine members. Spec 0056 D9 does not list the workflow, and `ci.yml` is edited by the 0053 plan as well. I left it out of every package. A one-line follow-up after the 0053 job lands would fix it; the spec's D9 should list it or the integrator should accept the label as stale.
- **Spec changes the build needs (for the integrator to rule on):** D9 and spec P3 say "the Makefile's 8"; after D5 it is nine (finding 4). The spec's Test section says to copy the tree with `git archive HEAD`; the tests need the working tree (finding 6). The spec's P1 does not say where the planted tests live. None blocks a package: P1 and P5 carry the corrected facts.
- **REQ-0056-07 stays a process rule:** no command can fail on a missing `TOTALS:` line. A follow-up could add an HTML comment to `.github/pull_request_template.md`; I did not, because the 0053 plan edits that file and the 0053 check counts its sections.
- **Open pull requests that edit personas, skills or the Makefile** could go red once P4 lands. I did not list them.
- I did not run any package's Done-when to green: the plan edits no code. The two spec Test blocks and the Makefile port (findings 1 to 3) were run on the base worktree, which is removed after the commit.
