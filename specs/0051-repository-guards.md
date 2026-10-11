---
status: planned
created: 2026-10-11
tags: [hooks, lefthook, guards, ci, gitleaks, conventional-commits]
priority: high
depends_on: [0002-verification-gates-and-evidence, 0003-branch-model-and-promotion, 0005-ci-cache-and-runners, 0048-licence-and-publication]
---

# 0051-repository-guards

> **Status**: planned, **Priority**: high, **Created**: 2026-10-11

## Overview

Spec 0003 says "repository guards" reject a direct push and says subjects are Conventional
Commits. It names no guard. The guards are three Claude Code hooks, three lefthook stages and
five CI jobs, written in four places. This spec names each, says what blocks and what advises,
records each bypass, and fixes three gaps found while reading them. the operator ruled on
2026-10-11 about 03:10Z that this spec is drafted [reported: AskUserQuestion in lane w25:p1,
label "Repository guards: hooks, push gates, lefthook"]. Content decisions are the integrating
lane's unless marked proposed.

### Facts (read 2026-10-11 at `e0e903f`)

- **F1 [verified]:** `bash .claude/hooks/tests/test-guard-main-push.sh`, `test-ci-gate.sh` and
  `test-lint-on-edit.sh` print `10 passed, 0 failed`, `14 passed, 0 failed` and `8 passed, 0 failed`.
- **F2 [verified]:** `git grep -n "hooks/tests" -- Makefile .github moon.yml lefthook.yml` prints
  nothing. No CI job and no Make target runs the hook tests.
- **F3 [verified]:** Feeding `guard-main-push.sh` the JSON for each command: `git push origin main`,
  `git push origin HEAD:main` and `git push origin HEAD:refs/heads/main` deny. `git -C /x push origin
  main`, `git push origin development` and `git push --force origin development` print nothing.
  `git push origin feat/main-menu` denies, a false positive.
- **F4 [verified]:** `ci-gate.sh` lines 7 to 8 match `git -C dir push`; `guard-main-push.sh` line 8
  matches only `git +push`. The two hooks disagree on what a push looks like.
- **F5 [verified]:** `ci-gate.sh` line 4 reads `SKIP_CI_GATE` from its own environment. A prefix
  typed inside the pushed command does not reach it. `lint-on-edit.sh` line 5 reads `SKIP_LINT` the
  same way. `settings.json` sets `timeout` 5 on the guard, 900 on ci-gate, 30 on lint-on-edit.
- **F6 [asserted]:** Claude Code treats a hook that exceeds `timeout` as a non-blocking error and
  runs the tool call. Not run here: a timeout test needs a live session. If true, ci-gate fails
  open at 900 s, not closed. `ci-gate.sh` has no timeout of its own.
- **F7 [verified]:** `lefthook.yml` line 9 accepts a first line matching
  `^((feat|fix|perf|refactor|docs|test|ci|build|infra|security|style|chore)(\(.+\))?!?: .+|Merge .+|Revert ".+")`.
  Piping subjects through that `grep -qE`: `docs(specs): x`, `feat!: x`, `Merge pull request #1`
  and `Revert "feat: x"` accept. `wip`, `Feat: x`, `release: 1.0`, `fixup! feat: x` and a bare
  `Merge` reject. Spec 0003 line 45 states only "Conventional Commits", so the regex is wider.
- **F8 [verified]:** `lefthook run pre-commit --force --commands gitleaks` prints `no leaks found`
  with gitleaks on PATH. With both gitleaks directories off PATH it prints `gitleaks not installed -
  CI will enforce` and exits 0 (lefthook.yml lines 17 to 22).
- **F9 [verified]:** `scripts/lefthook-affected-cargo.sh` maps changed files to `crates/<name>` and
  runs `cargo test` or `clippy` with `-j 2` for those packages. A change to `Cargo.*`,
  `rust-toolchain*`, `.github/`, `.cargo/`, `Makefile`, `lefthook.yml`, `scripts/` or `xtask/` runs
  the whole workspace. Docs and specs run nothing. With no base ref it runs the whole workspace.
- **F10 [verified]:** lefthook stages: `commit-msg` (conventional); `pre-commit` (gitleaks and
  affected-tests, in parallel); `pre-push` (affected clippy, then affected tests). The shims for all
  three are present in the shared hooks directory. `LEFTHOOK=0` is named in line 2.
- **F11 [verified]:** `ci.yml` has six jobs: `secrets`, `rust` (calls `_rust.yml`), `cargo-deny`, `hygiene`,
  `moon-affected` and `ebpf-live`. Spec 0003's table requires five on
  `development` (`secrets`, `rust / cargo`, `cargo-deny`, `hygiene`, `ebpf-live`) and four on `main`
  (no `ebpf-live`). `moon-affected` is informational (comment at line 88).
- **F12 [verified]:** Lines 28, 48, 57, 70, 90 and 132 skip each job when the PR head repository is
  not this repository. Lines 15 to 17 set the group `ci-<ref>` and cancel in progress only on
  `pull_request`. `codeql.yml` line 29 has the same guard.
- **F13 [verified]:** `git grep gitleaks -- Makefile` prints nothing, yet `AGENTS.md` line 44 says
  "`make` and lefthook run gitleaks". Only lefthook and the CI `secrets` job run it.
- **F14 [asserted]:** GitHub reports a job skipped by `if:` as success to a required check. If so,
  a fork PR passes all five required checks with no code run. Spec 0005 line 116 records the guard
  only. Not run: the repository is private and has no fork.

## Design

Two layers. The local layer (hooks and lefthook) gives a fast no and is bypassable on purpose. The
authority is branch protection plus the CI checks (spec 0003). A local gate never replaces a CI
check, and CI never trusts that a local gate ran.

| Gate | Where | Effect | Bypass |
|---|---|---|---|
| guard-main-push | PreToolUse, Bash | blocks (agent sessions only) | `# allow-direct-push` in the command |
| ci-gate | PreToolUse, Bash | blocks a push when `make check` fails | `SKIP_CI_GATE=1` in the session env |
| lint-on-edit | PostToolUse, Edit or Write | advisory, rustfmt of one `.rs` file | `SKIP_LINT=1` |
| lefthook commit-msg | git | blocks a bad subject | `LEFTHOOK=0` |
| lefthook gitleaks | git pre-commit | blocks a finding; skips a missing binary | `LEFTHOOK=0` |
| lefthook affected test and clippy | git pre-commit, pre-push | blocks | `LEFTHOOK=0` |

### Decisions

| ID | Decision | Class |
|---|---|---|
| D1 | Every row above blocks except lint-on-edit, which is advisory and exits 0 always. CI and branch protection stay the authority; this spec adds no required check. | lane-decided |
| D2 | Bypasses are not logged locally and no logging is built. The push that carries a bypassed change meets CI, and the override text sits in the transcript. `# allow-direct-push` is for the integrator and states why in the report. `SKIP_CI_GATE` and `LEFTHOOK=0` are for any lane with the reason in the PR body. No lane sets `SKIP_CI_GATE` as a habit. | lane-decided |
| D3 | ci-gate wraps `make check` in `timeout 840` and denies on exit 124, so the 900 s hook limit is never the first to fire. This makes the gate fail closed whatever F6 proves. | lane-decided |
| D4 | This spec owns the subject regex. Spec 0003 line 45 is amended to say "subjects follow the regex in spec 0051" (Plan). Merge and Revert subjects stay allowed: merge commits are the branch model. | lane-decided |
| D5 | guard-main-push gains three fixes: it matches `git -C dir push` like ci-gate, it denies a push whose target is `development` as well as `main` and `master`, and it matches the target as a whole ref name so `feat/main-menu` passes. | lane-decided |
| D6 | The hook tests become a Make target `hook-test`, listed in `hygiene`, so the CI `hygiene` job runs them. The job's image already runs `jq` (ci.yml line 120). | lane-decided |
| D7 | The gitleaks tolerance stays while the repository is private, because the CI `secrets` job scans every reachable commit. Before the fresh public repository exists (spec 0048 D9) a missing binary fails the hook and names the install command. | proposed, for the operator |
| D8 | Once public, a fork PR gets no self-hosted job (F12 stays). A required job `fork-gate` on a hosted runner fails a fork PR until a maintainer pushes the head to a branch in this repository, then CI runs on that branch. This closes F14. | proposed, for the operator |
| D9 | `AGENTS.md` line 44 is corrected to name lefthook and the CI `secrets` job, not `make` (F13). | lane-decided |

### Requirements (EARS)

- **[REQ-0051-01]** (Ubiquitous): The repository shall treat branch protection and the CI checks of spec 0003 as the authority, and every local gate shall be bypassable without weakening them.
- **[REQ-0051-02]** (Event-driven): WHEN a Bash command pushes to `main`, `master` or `development`, in any `git` option form including `-C dir`, the guard-main-push hook shall deny it.
- **[REQ-0051-03]** (Unwanted): IF a push names only a branch whose name contains `main` as a substring, such as `feat/main-menu`, THEN the guard-main-push hook shall allow it.
- **[REQ-0051-04]** (Event-driven): WHEN the command carries `# allow-direct-push`, the guard-main-push hook shall allow it and print nothing.
- **[REQ-0051-05]** (Event-driven): WHEN a Bash command runs `git push` in any option form and `SKIP_CI_GATE` is not `1`, the ci-gate hook shall run `make check` and deny the push when it exits non-zero.
- **[REQ-0051-06]** (Unwanted): IF `make check` runs longer than 840 seconds, THEN the ci-gate hook shall deny the push and name the timeout.
- **[REQ-0051-07]** (Ubiquitous): The lint-on-edit hook shall never block, and shall exit 0 whatever rustfmt returns.
- **[REQ-0051-08]** (Unwanted): IF a commit's first line does not match the subject regex in `lefthook.yml`, THEN the commit-msg stage shall reject the commit.
- **[REQ-0051-09]** (Ubiquitous): The subject regex shall accept the types `feat fix perf refactor docs test ci build infra security style chore`, an optional scope, an optional `!`, and git's own `Merge` and `Revert` subjects.
- **[REQ-0051-10]** (Unwanted): IF a staged change contains a secret and gitleaks is installed, THEN the pre-commit stage shall exit non-zero.
- **[REQ-0051-11]** (State-driven): WHILE the repository is private, the pre-commit stage shall skip the scan with a printed notice when gitleaks is not installed.
- **[REQ-0051-12]** (Event-driven): WHEN files change under `crates/<name>/`, the affected-crate script shall run cargo with `-j 2` for those packages only, and for the whole workspace when a workspace-level path changes or no base ref exists.
- **[REQ-0051-13]** (Ubiquitous): The `hygiene` target shall run the three hook test scripts, and a failing script shall fail the target.
- **[REQ-0051-14]** (Unwanted): IF a pull request comes from a fork, THEN the CI workflow shall run no self-hosted job on it.
- **[REQ-0051-15]** (Optional): WHERE the repository is public, a required `fork-gate` job shall fail a fork PR until its head is pushed to a branch in this repository.

## Plan

- [ ] Fix `guard-main-push.sh` for D5 and add test rows for `-C`, `development` and `feat/main-menu` first, red before the fix.
- [ ] Add `timeout 840` to `ci-gate.sh` (D3) and a test row with a stub `make` that sleeps past a lowered limit.
- [ ] Add a `hook-test` Make target and list it in `hygiene` (D6). Confirm one red run in CI by breaking a row.
- [ ] Amend spec 0003 line 45 to point at this spec's regex (D4). Done in a separate commit by the integrator; this commit leaves spec 0003 alone.
- [ ] Correct `AGENTS.md` line 44 (D9).
- [ ] Add a `lefthook.yml` comment that points to this spec as the owner of the subject regex.
- [ ] After the operator rules D7 and D8: change the gitleaks branch to `exit 1`; add `fork-gate` to `ci.yml` and to the spec 0003 table.
- [ ] Test F6 in a live session with `timeout` set to 1 and a `sleep 5` hook, and record the result here.

## Test

Rows marked "red today" fail on the current tree and pass after the Plan item lands.

| Requirement | Negative control |
|---|---|
| REQ-0051-02 | Planted `git push origin main` denies (green today). Red today: `git -C x push origin main` and `git push origin development` print nothing (F3). |
| REQ-0051-03 | Red today: `git push origin feat/main-menu` denies (F3). Control: `git push origin feat/x` allows. |
| REQ-0051-04 | `git push origin main # allow-direct-push` prints nothing (test row 10). Remove the marker and it denies. |
| REQ-0051-05 | Stub `make` exiting 1: the hook prints a deny carrying the stub output. With `SKIP_CI_GATE=1` the stub is not reached (test-ci-gate.sh). Control: `git status` never reaches it. |
| REQ-0051-06 | Red today: a stub `make` that sleeps past the limit produces no deny (F6). |
| REQ-0051-07 | A stub `rustfmt` exiting 1 leaves the hook exit code 0 (test-lint-on-edit.sh). |
| REQ-0051-08, 09 | The F7 list: four accept, five reject. Delete `Merge .+` from a scratch copy of the regex and `Merge pull request #1` flips to reject. |
| REQ-0051-10 | A scratch repo with a staged fake key: `lefthook run pre-commit --force --commands gitleaks` exits non-zero. Control: an empty stage prints `no leaks found`. |
| REQ-0051-11 | Both gitleaks directories off PATH in a scratch shell: exit 0 and the notice (F8). Red the day D7 lands. |
| REQ-0051-12 | Stage a `specs/` file only: the script prints `no crate changes`. Stage `Cargo.lock`: it prints `full workspace`. |
| REQ-0051-13 | Red today: F2. After D6, break one test row and `make hygiene` exits non-zero. |
| REQ-0051-14, 15 | Read `ci.yml`: each job carries the `head.repo.full_name` guard (F12). Not exercisable until a fork exists. F14 and D8 are the open question. |

```bash
for t in test-guard-main-push test-ci-gate test-lint-on-edit; do bash .claude/hooks/tests/$t.sh | tail -1; done
re='^((feat|fix|perf|refactor|docs|test|ci|build|infra|security|style|chore)(\(.+\))?!?: .+|Merge .+|Revert ".+")'
printf 'docs(specs): x\n' | grep -qE "$re" && echo accept-ok
printf 'wip\n' | grep -qE "$re" || echo reject-ok
lefthook run pre-commit --force --commands gitleaks | tail -2
make spec-check JOBS=2 | tail -1
```

## Notes

- The brief said ci-gate "fails closed on its 900 s timeout". Reading shows no such code, and F6
  says Claude Code likely lets the call through. D3 closes it in the script.
- The brief called the regex wider than spec 0003; F7 confirms it, and D4 records it.
- Open for other lanes: who installs `prepare-commit-msg`. It is a lefthook shim and does nothing
  here; not verified beyond its header.
- Rulings needed from the operator: D7 and D8. Neither blocks the lane-decided Plan items.
- The guards stop honest mistakes, not a determined bypass. Branch protection is the control that
  holds (spec 0003).
