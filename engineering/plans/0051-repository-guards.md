<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plan: 0051-repository-guards

Spec: `specs/0051-repository-guards.md` (status planned). Base: `origin/development` `2a3d442` (the plan was first cut at `8d80119`; the two merges since touch only specs 0052 and 0053, the CHANGELOG and the spec index, none of the files this plan governs).
Planner: `uas-planner`, 2026-10-11. Every path below was read in a worktree on that base, and the checks quoted as verified were re-run there after the rebase.
Tiers: `verified` = I ran or read it at the path named; `reported` = the spec says it; `asserted` = my judgement.

## Findings the builders must know

1. **The spec's concurrency test row cannot print 1 [verified]:** `grep -c 'cancel-in-progress: ${{ github.event_name == .pull_request. }}' .github/workflows/ci.yml` prints `0` at this base, because grep reads `${{` as a pattern, not text. With `grep -cF` and the quote written out it prints `1`; the control (the line removed) prints `0`. The standing checks below use the `-F` form.
2. **Line numbers moved [verified at `2a3d442`]:** the sentence "Subjects follow Conventional Commits" in spec 0003 is on line 46, not 45. Spec 0005 lists the `ci.yml` jobs on line 82. Builders match by text, not by line.
3. **The hook docs go stale and the spec's Plan does not list them [verified]:** `.claude/README.md` line 15 says the guard denies `main` or `master`, and lines 20 to 26 give a loop for the tests. `.claude/settings.json` line 24 repeats the "main/master" description. P1 and P3 fix them.
4. **A label added after the PR opens never reaches a re-run [asserted, from how GitHub re-runs a job: it replays the original event payload]:** a fork-gate that reads `github.event.pull_request.labels` stays red after the maintainer labels the PR. The job must read the labels from the API at run time. Adding `labeled` to the `pull_request` types would work but re-runs every job on every label, and `labeler.yml` labels every PR. P5 reads the API and the maintainer re-runs the one job.
5. **The label alone is not bound to a head SHA, and Christian ruled on it [reported: AskUserQuestion, lane w25:p1, 2026-10-11 about 04:10Z, "Label plus head SHA"]:** the gate passes only when the label is present and a maintainer comment names the PR's current head SHA; a new push turns it red. This overrides the label-only text of D8 and REQ-0051-15, which P6 amends in spec 0051. P5 builds the ruled form.
6. **No workflow here uses a hosted runner [verified]:** `grep -rn ubuntu .github/workflows` prints nothing. Every job runs in the self-hosted group `ent-default`. Whether the organisation has hosted minutes is `asserted` unknown. A required job that never gets a runner leaves every PR pending, so P7 waits for one green run first (the pattern of spec 0005 D2 for `ebpf-live`).
7. **Making `fork-gate` required changes three texts together [verified]:** the branch protection (Christian), the spec 0003 protection table, and `want=` in spec 0050's Test row 4 (`secrets,rust / cargo,cargo-deny,hygiene,ebpf-live`, spec 0050 line 177). Until the protection changes, the table row says "no" so that row 4 stays true. P6 writes the row that way.
8. **D7 can block builders [asserted]:** after P4 a commit on a host whose PATH lacks gitleaks fails. Here `gitleaks` resolves from a mise directory on the interactive PATH. The integrator confirms that builder shells have it, or the builder commits with `LEFTHOOK=0` and says why in the verdict (D2 allows it).
9. **ci-gate already fails closed when `timeout` is missing [verified by reading, bash semantics]:** `timeout: command not found` exits 127, the `if !` branch runs, and the push is denied with that text. D3 needs only the 124 and 137 message. GNU `timeout` signals its whole process group, so a child of `make` dies with it (I ran `timeout -k 2 1 sh -c 'sleep 37.5 & wait'`; the sleep was gone).
10. **The spec's REQ-0051-12 row runs a real workspace test [verified by reading `lefthook-affected-cargo.sh` lines 49 to 54]:** staging `Cargo.lock` execs `cargo test --workspace`. The standing check puts a stub `cargo` first on PATH.
11. **Two guard gaps beside D5 [asserted from lines 7 and 8]:** `git push origin` (remote only) and `git push origin HEAD` push the current branch, and the new bare-push rule should cover the first. P1 includes the remote-only row. `HEAD` is Open.
12. **`ci.yml` line 2 is stale [verified]:** it lists four required checks, not five. P5 rewrites the line to name `fork-gate`.

## Plan items to packages

| Spec 0051 Plan item | Package |
|---|---|
| guard-main-push fixes (D5), test rows first | P1 |
| ci-gate `timeout 840` (D3), slow-make row | P2 |
| `hook-test` target in `hygiene` (D6); one red CI run | P3 (target); the red CI run is the integrator's push, not a builder act |
| Amend spec 0003 line 45 (D4) | P6 |
| `AGENTS.md` line 44 (D9) | P4 |
| `lefthook.yml` comment naming this spec owner of the regex (D4) | P4 |
| gitleaks fails closed with an install hint (D7) | P4 |
| `fork-gate` job (D8), spec 0003 table row | P5 (job), P6 (table), P7 (required status, DONE 2026-10-11) |
| Test F6 in a live session, record the result | P8 (BLOCKED, manual) |
| Bypass logging | none: D2 builds none. No package adds logging |

## Requirement coverage

| Requirement | Closed by |
|---|---|
| REQ-0051-02, 03 | P1 |
| REQ-0051-04 | holds today (test row 10); P1 keeps it and adds a `-C` variant |
| REQ-0051-06 | P2 |
| REQ-0051-11 | P4 |
| REQ-0051-13 | P3 |
| REQ-0051-15 | P5 (the job), P6 (the table), P7 (the required status, BLOCKED) |
| REQ-0051-01, 05, 07, 08, 09, 10, 12, 14 | hold today; no package. The standing checks below prove each and carry a control |

6 of 15 requirements are closed by a package (02, 03, 06, 11, 13, and 15 in part). 9 hold today with a command that can fail. REQ-0051-15 completes only at P7, which is BLOCKED. No requirement is left without a command.

## Standing checks for the requirements that hold today

Run from the repository root of any worktree. Today (verified at `2a3d442`) the output reads as the `want` words say; each control goes red when its text is damaged.

```bash
set -u
R=$(pwd); c=.github/workflows/ci.yml; t=$(mktemp -d)
re=$(sed -n "s/.*grep -qE '\(.*\)' *\\\\\$/\1/p" lefthook.yml)
acc=0; for s in 'docs(specs): x' 'feat!: x' 'Merge pull request #1' 'Revert "feat: x"'; do printf '%s\n' "$s" | grep -qE "$re" && acc=$((acc+1)); done
rej=0; for s in wip 'Feat: x' 'release: 1.0' 'fixup! feat: x' Merge; do printf '%s\n' "$s" | grep -qE "$re" || rej=$((rej+1)); done
echo "08/09 accept $acc of 4, reject $rej of 5; control (Merge removed) $(printf 'Merge pull request #1\n' | grep -qE "${re//|Merge .+/}" && echo RED || echo rejects)"
g='head.repo.full_name == github.repository'; k="cancel-in-progress: \${{ github.event_name == 'pull_request' }}"
echo "14 guards $(grep -c "$g" $c) want 6; control $(awk -v g="$g" 'index($0,g)&&!d{d=1;next}{print}' $c | grep -c "$g") want 5"
echo "14 concurrency $(grep -cF "$k" $c) want 1; control $(grep -vF "$k" $c | grep -cF "$k") want 0"
printf '#!/bin/sh\necho "cargo $*"\n' > $t/cargo; chmod +x $t/cargo
git init -q $t/r; cd $t/r; git config user.email a@b.c; git config user.name t; mkdir scripts specs; cp $R/scripts/lefthook-affected-cargo.sh scripts/
echo x > specs/a.md; git add specs; echo "12 docs only: $(PATH=$t:$PATH bash scripts/lefthook-affected-cargo.sh --staged test 2>&1 | grep -c 'no crate changes') want 1"
git reset -q; echo x > Cargo.lock; git add Cargo.lock; echo "12 lockfile: $(PATH=$t:$PATH bash scripts/lefthook-affected-cargo.sh --staged test 2>&1 | grep -c 'full workspace') want 1"
cd $R; for x in test-guard-main-push test-ci-gate test-lint-on-edit; do bash .claude/hooks/tests/$x.sh | tail -1; done
gh api repos/unsigned-gg/uas/branches/development/protection --jq '.required_status_checks.contexts|join(",")'
```

Rows 05, 07 and 04 are the three hook test scripts in the last loop (today `14 passed`, `8 passed`, `10 passed`). Row 01 is the last line: today `secrets,rust / cargo,cargo-deny,hygiene,ebpf-live`. Its control is spec 0050 Test row 4, which prints `4 RED` for any other list. REQ-0051-10 is the third line of P4's script. The 14-guards row stays at 6 after P5, because the new job carries no such guard.

## Waves

| Wave | Packages | Disjoint because | State after the wave |
|---|---|---|---|
| 1 | P1, P2, P3 | P1: `guard-main-push.sh`, its test, `settings.json`. P2: `ci-gate.sh`, its test. P3: `Makefile`, `.claude/README.md` | Guards fixed, hook tests in `hygiene` |
| 2 | P4, P5, P6 | P4: `lefthook.yml`, `AGENTS.md`. P5: `ci.yml`. P6: `specs/0003`, `specs/0005` | gitleaks fails closed, `fork-gate` exists, specs carry the new texts |
| later | P7, P8 | BLOCKED | none |

P3 runs in wave 1 although it runs the other two scripts: each builder's worktree holds the base versions of the other files, and all three scripts are green there.

## Packages

### P1: guard-main-push (wave 1) (DONE 2026-10-11, #172)
- **Persona:** `uas-builder`.
- **Files, all exist [verified, `git ls-files`]:** `.claude/hooks/guard-main-push.sh` (match on lines 7 and 8, deny text on line 9, header on line 3), `.claude/hooks/tests/test-guard-main-push.sh` (rows on lines 30 to 39), `.claude/settings.json` (the `description` on line 24 only).
- **Change, tests first:** line 36 of the test, `t allow feat 'git push origin development' 'push development'`, encodes the behaviour D5 removes. Flip it to `t deny feat 'git push origin development' 'push development'`. Then add nine rows after line 39 in the existing `t <deny|allow> <branch> <command> <description>` form. Deny: `feat` `git -C /x push origin main`; `feat` `git push --force origin development`; `development` `git push`; `development` `git -C . push`; `development` `git push origin`. Allow: `feat` `git push origin feat/main-menu`; `feat` `git -C /x push origin feat/x`; `development` `git push origin feat/x`; `feat` `git -C /x push origin development # allow-direct-push`. The suite then holds 10 + 9 = 19 rows, and the flipped row is one of the 19.
- **Change, hook:** treat a push as ci-gate does (`ci-gate.sh` line 8: `git`, optional option words including `-C dir`, then `push` as a whole word). Deny when a word after `push` names `main`, `master` or `development` as a whole ref name, plain, with a `refs/heads/` prefix, after a `+` or after a `:` (so `HEAD:main` still denies). A name that only contains `main`, such as `feat/main-menu` or `main-menu`, is not a match. Deny a push with no refspec (bare, or the remote name alone) when the checked-out branch is `main` or `development`; when `-C dir` is present, read the branch of `dir`, else of the working directory. Keep the `# allow-direct-push` early exit and the single deny JSON. Say `main`, `master` or `development` in the deny text. Update the header comment on line 3 and the `settings.json` description to name `development`.
- **Done-when:** `bash .claude/hooks/tests/test-guard-main-push.sh | tail -1 | grep -qx '19 passed, 0 failed' && bash .claude/hooks/tests/test-guard-main-push.sh >/dev/null && jq -e . .claude/settings.json >/dev/null` exits 0. These are the Test rows for REQ-0051-02, 03 and 04.
- **Red-first:** flip line 36 and add the nine rows, then run the suite before the hook edit: it must print seven `FAIL` lines (the flipped `push development` row, the five new deny rows, and the `feat/main-menu` allow row) and exit 1; the other three new rows and the nine unchanged old rows stay green. The `FAIL` set is read from lines 7 and 8 for the `development` and remote-only rows [asserted]; the `-C main`, `development` and `feat/main-menu` outcomes I ran [verified, spec F3]. After the fix, restore the old line 7 in a scratch copy of the hook and watch at least one of the seven rows fail again.

### P2: ci-gate timeout (wave 1) (DONE 2026-10-11, #172)
- **Persona:** `uas-builder`.
- **Files, all exist [verified]:** `.claude/hooks/ci-gate.sh` (lines 10 to 13), `.claude/hooks/tests/test-ci-gate.sh` (stub on lines 10 to 15, rows on lines 36 to 60).
- **Change, tests first:** extend the stub `make` so it runs `sleep "${STUB_MAKE_SLEEP:-0}"` before it exits. Add three assertions: a push with `CI_GATE_TIMEOUT=1`, `STUB_MAKE_SLEEP=5` and `STUB_MAKE_RC=0` prints a deny whose reason contains the word `timeout`; the hook returns in under 4 seconds (measure with `date +%s`); a push with `CI_GATE_TIMEOUT=1` and no sleep prints nothing (the control). The suite then holds 17 assertions.
- **Change, hook:** run `timeout -k 10 "${CI_GATE_TIMEOUT:-840}" make check` and keep its exit code (the current `if ! out=$(...)` loses it). Exit 124 or 137 denies with a reason that names the timeout and its seconds; any other non-zero exit denies as today with the last 15 lines. A missing `timeout` binary exits 127 and denies by the same branch (finding 9). The default 840 s keeps the hook under the 900 s `settings.json` limit.
- **Done-when:** `bash .claude/hooks/tests/test-ci-gate.sh | tail -1 | grep -qx '17 passed, 0 failed' && bash .claude/hooks/tests/test-ci-gate.sh >/dev/null` exits 0. This is the Test row for REQ-0051-05 and 06.
- **Red-first:** run the new rows before the hook edit: the deny assertion and the under-4-seconds assertion fail and the run takes at least 5 seconds; the control stays green. After the fix, change `-k 10 "${CI_GATE_TIMEOUT:-840}"` to `0` in a scratch copy and watch the deny assertion fail.

### P3: hook tests join hygiene (wave 1) (DONE 2026-10-11, #172)
- **Persona:** `uas-builder`.
- **Files, all exist [verified]:** `Makefile` (`.PHONY` lines 15 to 17, `hygiene` line 49), `.claude/README.md` (the Hooks rows on lines 15 and 16, the Tests section on lines 20 to 26).
- **Change:** a target `hook-test: ## Run the Claude Code hook tests` that loops over `.claude/hooks/tests/test-*.sh` with `bash "$$t" || exit 1`, listed in `.PHONY` and as a prerequisite of `hygiene`. In the README, say that the guard denies `main`, `master` and `development`, that ci-gate stops at 840 seconds (`CI_GATE_TIMEOUT` lowers it for tests), and that the tests run with `make hook-test` and inside `make hygiene`. Do not touch `ci.yml`: the `hygiene` job already runs `make hygiene` (line 84).
- **Done-when:** `make hook-test >/dev/null && test "$(make -n hygiene | grep -c 'hooks/tests')" -ge 1 && make hygiene -o typos -o machete -o spec-check -o toolchain-check -o pin-check -o oss-check JOBS=2 >/dev/null` exits 0. The last leg runs `hygiene` with its other prerequisites marked old, so only `hook-test` runs.
- **Red-first:** before the edit `make -n hygiene | grep -c 'hooks/tests'` prints `0` (spec F2). After the edit, flip the expected word on one row of `test-guard-main-push.sh` (for example the `push to main` row from `deny` to `allow`) and run the last leg: it must exit 2 [verified on a scratch edit of this base]. Restore the row.
- **Integrator step, not a builder act:** the spec asks for one red run in CI. The integrator pushes a throwaway commit that breaks one row, reads the `hygiene` job red, and removes it. That is a push and stays with the integrator.

### P4: gitleaks fails closed, regex owner comment, AGENTS.md (wave 2) (DONE 2026-10-11, #176)
- **Persona:** `uas-builder`.
- **Files, all exist [verified]:** `lefthook.yml` (gitleaks command on lines 15 to 23, the regex on line 9), `AGENTS.md` (line 44, item 11).
- **Change, lefthook:** in the missing-binary branch print to stderr that gitleaks is not installed, name the install step with the version CI pins (8.30.1, `ci.yml` line 37) and the release URL `https://github.com/gitleaks/gitleaks/releases/tag/v8.30.1`, then `exit 1`. Rewrite the comment on lines 16 and 17 (it says only a missing binary is tolerated). Add a comment above the `conventional` command: spec 0051 owns the subject regex. Leave the regex itself unchanged. ASCII only.
- **Change, AGENTS.md:** item 11 reads "Enforced: the lefthook pre-commit stage and the CI `secrets` job run gitleaks." (finding in spec F13: `make` does not.)
- **Done-when:** the script below, then the standing check for rows 08 and 09. It prints `rc=0` for the control, a non-zero `rc` plus one or more hint lines for the missing binary, and a non-zero `rc` for the staged key. Then `grep -cF '`make` and lefthook run gitleaks' AGENTS.md` and `grep -cF 'pre-commit stage and the CI `secrets` job run gitleaks' AGENTS.md` print `0` and `1`; today they print `1` and `0` [verified].

```bash
set -u
R=$(pwd); t=$(mktemp -d); git init -q $t/r; cd $t/r; git config user.email a@b.c; git config user.name t
cp "$R/lefthook.yml" .; mkdir $t/nb; ln -s "$(command -v lefthook)" $t/nb/lefthook
lefthook run pre-commit --force --commands gitleaks >/dev/null 2>&1; echo "control, gitleaks present, empty stage: rc=$? want 0"
out=$(PATH=$t/nb:/usr/bin:/bin lefthook run pre-commit --force --commands gitleaks 2>&1); rc=$?
echo "REQ-11, gitleaks off PATH: rc=$rc want non-zero; hint lines: $(printf '%s' "$out" | grep -c 'gitleaks/releases') want 1 or more"
printf 'aws_access_key_id = "%s%s"\nkey="%s%s"\n' AKIAIOSFODNN7 EXAMPLF ghp_ abcdefghijklmnopqrstuvwxyz0123456789 > leak.txt; git add leak.txt
lefthook run pre-commit --force --commands gitleaks >/dev/null 2>&1; echo "REQ-10, staged fake key: rc=$? want non-zero"
```
- **Red-first:** run the script on the unedited tree: the REQ-11 line prints `rc=0` and `0` hint lines [verified at this base; the edited copy printed `rc=1` and `1`]. After the edit, delete the `exit 1` in a scratch copy and the REQ-11 line goes back to `rc=0`. For the AGENTS.md legs, restore the old sentence in a scratch copy and the pair reads `1` and `0` again.

### P5: the fork-gate job (wave 2) (DONE 2026-10-11, #176)
- **Persona:** `uas-builder`.
- **Files:** `.github/workflows/ci.yml` (exists; header comment on line 2, new job after `ebpf-live`).
- **Form chosen: a maintainer comment, not a mirror branch [reported ruling: label plus head SHA, recorded in a comment the job reads, or a mirror branch].** The comment needs one read call and no push. A mirror branch needs a push by a maintainer and a ref lookup, and it runs the fork's code on self-hosted runners, which is the thing the guard exists to stop. The comment is written by a maintainer with `gh pr comment <N> --body "fork-ci-approved: <40-character head sha>"`. The body is exactly that one line (trailing white space allowed). The job accepts a comment only when `author_association` is `OWNER` or `MEMBER`, so the fork author cannot approve their own PR.
- **Change:** a job `fork-gate` with `name: fork-gate`, no `if:`, `runs-on: ubuntu-24.04`, `timeout-minutes: 5`, `permissions: {contents: read, pull-requests: read}` and one step. The step takes `GH_TOKEN` (`github.token`), `EVENT`, `HEAD_REPO` (`github.event.pull_request.head.repo.full_name`), `BASE_REPO` (`github.repository`) and `PR` (`github.event.pull_request.number`) from the environment and runs, under `set -euo pipefail`: exit 0 when `EVENT` is not `pull_request` or `HEAD_REPO` equals `BASE_REPO`. Otherwise read three things from the API, each into a variable first: the label names (`issues/$PR/labels`), the current head SHA (`pulls/$PR`, field `.head.sha`; never the event payload, which a re-run replays stale), and the comments (`issues/$PR/comments`, filtered by `jq` to maintainer comments whose trimmed body equals `fork-ci-approved: <current head sha>`). Pass only when the label `fork-ci-approved` is present (`grep -xF ... >/dev/null`, not `grep -q`, and not a pipe from `gh`, so no SIGPIPE under `pipefail`) and the filtered output is non-empty. Else print an `::error::` line that names the exact comment to post and exit 1. A failed API call fails the job (closed). A deleted fork (empty `HEAD_REPO`) is a fork. A new push changes the head SHA, so the old comment no longer matches and the gate is red again; a maintainer re-runs the one job after commenting. Do not write the text `head.repo.full_name == github.repository` in the job (it would raise the guard count above 6). Do not add `labeled` to the triggers (finding 4). Rewrite the header comment on line 2 to list the required checks and say `fork-gate` becomes one once public. This job uses no action.
- **What this does not do, stated plainly:**
  - Until P7 lands, a fork PR with no label gets a red `fork-gate` that does not block the merge. The five required checks skip on a fork PR and GitHub counts a skipped job as passed (spec F14, asserted), so the F14 hole stays open until P7 makes `fork-gate` required.
  - Nothing in P5 gives a fork PR secrets or runs its code. The job runs on the `pull_request` event, whose token for a fork is read-only and holds no repository secrets. It uses no action, checks out nothing, and reads only labels, the head SHA and comments through the API.
- **Done-when:** the script below prints `0 1 1 1 0 1 1 0 1 1` on its first line and a count of at least 1 on its second, and the standing checks keep the guard count at 6 and the concurrency count at 1.

```bash
set -u
c=.github/workflows/ci.yml; d=$(mktemp -d); H=aaaa1111; L=fork-ci-approved
run=$(yq '.jobs."fork-gate".steps[0].run' $c); mkdir $d/stub
cat > $d/stub/gh <<'EOS'
#!/bin/sh
case "$*" in
  *labels*) printf '%s' "$FAKE_LABELS" | jq -Rn '[inputs|select(length>0)|{name:.}]' -R ;;
  *comments*) printf '%s' "$FAKE_COMMENTS" ;;
  *pulls*) printf '{"head":{"sha":"%s"}}' "$FAKE_HEAD" ;;
esac
EOS
chmod +x $d/stub/gh
ok="[{\"author_association\":\"MEMBER\",\"body\":\"$L: $H\n\"}]"
old="[{\"author_association\":\"MEMBER\",\"body\":\"$L: bbbb2222\"}]"
none="[{\"author_association\":\"NONE\",\"body\":\"$L: $H\"}]"
g() { env PATH="$d/stub:$PATH" EVENT="$1" HEAD_REPO="$2" BASE_REPO=o/r PR=7 FAKE_LABELS="$3" FAKE_COMMENTS="$4" FAKE_HEAD=$H bash -c "$run" >/dev/null 2>&1; echo -n "$? "; }
g push '' '' '[]'; g pull_request f/r '' '[]'; g pull_request f/r $L '[]'; g pull_request f/r '' "$ok"; g pull_request f/r $L "$ok"
g pull_request f/r $L "$old"; g pull_request f/r $L "$none"; g pull_request o/r '' '[]'; g pull_request '' '' '[]'; g pull_request f/r $L-not "$ok"; echo
grep -c 'fork-gate' $c
```
  The cases are, in order: a push; a fork PR with no label and no comment; the label alone; the comment alone; the label and a matching comment; the label and a comment for an older SHA (the new-push case); the label and a matching comment from a non-maintainer; a same-repository PR; a deleted fork with nothing; a label that only starts like the real one. The expected line is `0 1 1 1 0 1 1 0 1 1`. I ran this script against a draft of the job in a scratch copy of `ci.yml` and it printed that line [verified]. Today `yq` returns `null`, the first line is wrong, and `grep -c 'fork-gate'` prints `0` (spec F14).
- **Red-first:** run the script before the edit and watch it fail. After the edit, change `grep -xF` to `grep -F` in a scratch copy and the last case flips from `1` to `0`. Change the comment filter to ignore the SHA and the old-SHA case flips from `1` to `0`. Remove the job in a scratch copy and the grep count returns to `0`.

### P6: specs 0003 and 0005 (wave 2) (DONE 2026-10-11, #176)
- **Persona:** `uas-engineer` (a spec amendment).
- **Files, all exist [verified]:** `specs/0003-branch-model-and-promotion.md` (line 46 and the protection table, `ebpf-live` row on line 64), `specs/0005-ci-cache-and-runners.md` (line 82), `specs/0051-repository-guards.md` (D8, REQ-0051-15 and its Test row).
- **Change:** line 46 becomes "Commits are signed. Subjects follow the Conventional Commits regex in spec 0051; Merge and Revert subjects stay allowed." Add a table row "Required check `fork-gate` | no, until plan 0048 P13 | no, until plan 0048 P13" after the `ebpf-live` row; it says "no" on purpose so spec 0050 Test row 4 stays true (finding 7). Add `fork-gate` to the `ci.yml` job list on spec 0005 line 82. Amend spec 0051 D8, REQ-0051-15 and the REQ-0051-15 Test row to the ruled form: the gate passes only with the label and a maintainer comment `fork-ci-approved: <head sha>` that equals the PR's current head SHA; the test row names the case table of P5. Record the ruling (lane w25:p1, 2026-10-11 about 04:10Z, label "Label plus head SHA"). Also replace the concurrency Test row with the fixed-string form (finding 1). The spec says the integrator commits the spec 0003 amendment on its own, so the integrator may split this package into one commit per spec.
- **Done-when:** `grep -c 'regex in spec 0051' specs/0003-branch-model-and-promotion.md` prints `1`, `for f in specs/0003-branch-model-and-promotion.md specs/0005-ci-cache-and-runners.md; do grep -c 'fork-gate' $f; done` prints `1` then `1`, `grep -c 'Label plus head SHA' specs/0051-repository-guards.md` prints `1`, and `make spec-check JOBS=2` exits 0 with no new error.
- **Red-first:** the greps print `0` before the edit. After it, delete the new table row in a scratch copy and the loop prints `0` for spec 0003. Revert the D8 wording in a scratch copy and the `Label plus head SHA` grep prints `0`.

### P7: fork-gate becomes a required check (DONE 2026-10-11, Christian's act)
- **Done 2026-10-11:** Christian ruled "Add fork-gate to the required checks now" (AskUserQuestion, lane w25:p1, about 05:25Z) and the integrating lane applied it to `development`. `gh api repos/unsigned-gg/uas/branches/development/protection --jq '.required_status_checks.contexts'` read back six contexts ending in `fork-gate` on 2026-10-11. Spec 0003 and spec 0050 row 4 were amended the same day.
- **Was BLOCKED:** branch protection is Christian's (spec 0003; the repo setting is not a file). Spec 0051 D8 and plan 0048 P13 set the time: before the public cut. It also waits for one green `fork-gate` run on `development` (finding 6), and the same sitting updates `want=` in spec 0050 Test row 4 and the two `no, until plan 0048 P13` cells in spec 0003.
- **Done-when, when it unblocks:** `gh api repos/unsigned-gg/uas/branches/development/protection --jq '.required_status_checks.contexts|join(",")'` prints a list holding `fork-gate`, and the `want=` string in spec 0050 line 177 is the same list. Today it prints `secrets,rust / cargo,cargo-deny,hygiene,ebpf-live`.
- **Red-first:** the command's output lacks `fork-gate` until the change.
- **Also needed:** a maintainer creates the label `fork-ci-approved` in the repository (a repository setting, not a file). Until P7 the F14 hole is open: a fork PR gets a red `fork-gate` that does not block the merge (see P5). Plan 0048 P13 does not name `fork-gate` as a prerequisite today (`grep -n fork-gate engineering/plans/0048-licence-and-publication.md` prints nothing); the integrator adds the line.

### P8: test F6 in a live session (BLOCKED, manual)
- **BLOCKED:** needs a live Claude Code session, which a builder does not have. Spec Plan, last item: set a hook `timeout` of 1 with a `sleep 5` hook in a throwaway settings file and record whether the tool call runs. The integrator or Christian runs it and the result goes into spec 0051 F6 as an amendment (`uas-engineer`).
- **Done-when:** F6 in the spec reads `[verified]` or `[refuted]` with the date and the observed behaviour, and `grep -c 'F6 \[asserted\]' specs/0051-repository-guards.md` prints `0`. Today it prints `1`.
- **Red-first:** the grep prints `1` today. D3 holds whatever the result is, so no package waits on this.

## Open

- None on the fork-gate form: Christian ruled label plus head SHA (finding 5). P6 amends spec 0051 to match.
- Whether `git push origin HEAD` while on `development` should deny (finding 11). Not in the spec; no row added.
- Whether the organisation has hosted runner minutes (finding 6). P7's "one green run first" is the test.
- The label name `fork-ci-approved` is my choice; the spec only says "a label" [asserted]. Christian can rename it before P5.
- Who installs `prepare-commit-msg` (spec Notes) is untouched: not read beyond the spec.
- Not run: a real fork PR (the repository is private) and `make hygiene` whole (typos and machete not run here). P3's last leg ran with those prerequisites marked old.
