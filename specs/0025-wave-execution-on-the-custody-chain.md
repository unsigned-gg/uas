---
status: planned
created: 2026-10-10
tags: [engine, wave, custody, blackwall, saga, cpm]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0008-soma-cortex-saga-reverie-integration, 0009-saga-compensation-runner-and-durability, 0020-build-job-cap, 0022-custody-landing, 0023-test-time-compute-and-verifiable-reward, 0024-estate-saga-cpm-and-agent-sprint-wave-strategy]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0025-wave-execution-on-the-custody-chain
**HELD 2026-10-10 (the operator, in session, lane w25:p1): wave execution is a saga domain; uas implements no more of it. The landed code stays as is, unarmed; the migration shape is to be ruled.**
> **Moves to saga, 2026-10-11 (spec 0049 D6, lane-decided: integrating lane w25:p1):** saga spec 0040 (`cerebral-work/saga` `specs/0040-cpm-and-wave-execution.md`, PR 133, 51e4df8) restates the admission, not-ready, halt and compensation rows under uas spec 0049. Each moved rule here is superseded there. The HELD line above stays as the operator's words.

## Overview

Roadmap item from the operator, 2026-10-10, lane w25:p1: "implement cpm and agent waves with deltadb/blackwall and saga".
Spec 0024 (landed in b409386) plans waves: `WavePlan`, `WaveCoordinator`, the roles Probe, Builder, Critic and MainSession, and `uas flow --wave`. It executes nothing on the custody chain.
This spec makes each wave step run on that chain. A Builder is a jailed blackwall run. A Critic is a `uas-critic` run. A Probe is a command. A MainSession step is a saga gate. Saga drives one run per wave. `uas-engine` owns the plan, the custody calls, the halt and the compensation.

Facts, 2026-10-10. Source facts from `unsigned-gg/blackwall@3d98dc1` are `reported`: that checkout is dirty (`run.rs` has 6 local lines).

- **F1 [verified, read `crates/uas-engine/src/wave.rs`]:** `validate_verdict` checks only the 15-line cap; nothing parses a verdict.
  `classify_step` gives Builder and Critic `ModelTier::Pro`. `uas flow --wave` runs each step's `verify_cmd` in-process (`crates/uas-cli/src/main.rs:568-640`).
- **F2 [verified, read `crates/uas-engine/src/landing.rs`]:** `land` runs the base check, `reconcile` and `pr` in one call, never with `--draft`.
  Its input needs a slot id, a `run_hash`, an `AuditStatus` and a `cpu_cap`. A wave run has no UAS slot, so none has an honest value.
- **F3 [reported, `crates/blackwall-cli/src/run.rs:815-825,884,898`]:** `start` prints on stderr `run <model> (ok|err): hash <12 chars> (new|dedup)`, then
  `changeset: <12 chars> (<n> entries)` and `world commit: <12 chars>`. `run.rs` has no `println!`, so nothing goes to stdout. Each line starts with a check mark (U+2713), so a parse matches substrings. [verified, P9c run 0dbb7f02bf2d, stderr as recorded] `start` printed `-> running haiku...`, `(ok): hash 0dbb7f02bf2d (new)`, `op trace: 3 delta(s)`, `changeset: 4abbf3bebe21 (1 entries)`, `world commit: 22749f85c3ad` and `otel: exported 4 span(s)`, exit 0, and the soma lane names `start --json` the stable surface [reported, soma lane, CER-2787]. The record shows no `run <model>` text and no check mark, and stdout was not teed, so those parts stay `reported`.
- **F4 [reported, `reconcile.rs:151`, `pr.rs:149`, `settle.rs:60`, `report.rs:437`]:** `settle`, `reconcile`, `pr` and `changeset` resolve a hash prefix.
  `show` does not (`report.rs:420-425`). Its `RunRecord` JSON (`blackwall-store/src/record.rs:210`) has no base and no changeset hash.
- **F5 [reported, `run.rs:309-321`]:** `init` writes the git HEAD to `.blackwall/init-head` [verified, P9c step 3: `init-head` and `git rev-parse HEAD` both 642c3f292d45].
  A `start` whose HEAD differs only warns. [reported, `blackwall reconcile --help` text]
  `reconcile` refuses a stale root unless given `--force`. No run has shown either.
- **F6 [verified, `blackwall <cmd> --help` in an empty dir]:** `start` takes `--task`,
  `--provider`, `--model` and `--parent`, and no persona. `show`, `list` and `changeset`
  have no JSON flag. `serve` only inspects the store.
- **F7 [verified, `claude --help`]:** `claude -p` and `--agent <name>` exist. [reported,
  `.claude/agents/README.md`] `--model` overrides a persona's model. [verified, 2026-10-10]
  `--allowed-tools` is variadic: `claude -p --allowed-tools Read 'Reply PONG' </dev/null`
  exits 1, "Input must be provided". The prompt on stdin printed PONG, exit 0.
- **F8 [verified, `saga run --help`, `saga gate approve --help`, `saga flows`]:**
  `saga run <flow> --param k=v --dry-run`; `saga gate approve <run> <task>`. Here
  `saga flows` listed 21 flows: 3 from `.saga/flows/`, 18 from outside the repo. A dry-run
  of `estate-custody-rollout` printed `inputs` equal to `deps` and no `verify_cmd`. [verified, run 20261010-140148-cas-cpm-wave-w0] `saga run` exits 0 when every task fails, so only `run.json` holds the result.
- **F9 [reported, lane-package flow header, architect w10:pC]:** the shipped step kinds are
  impl, test, review, gate and note. Saga copies `verify_cmd` and `cwd` raw, so `{{params}}`
  is not substituted there. [reported, `vendor/saga/contracts/flow.rs`, saga@1e4be86] a step
  has `agent`, `cwd` and an optional `persona`.
- **F10 [verified, read `.claude/agents/uas-critic.md`]:** the critic's first line is
  `VERDICT: DEFECTS` or `VERDICT: NO DEFECTS`. It says NO DEFECTS only after it ran the
  done-when command and the gates itself.
- **F11 [verified, probe on rustc 1.94.1]:** `std::fs::File::lock` works on the pinned
  toolchain. A host lock needs no new dependency.
- **F12 [verified, read `blackwall.toml` and `tasks/*/task.toml`, 2026-10-10]:** `blackwall.toml`
  sets `sonnet`; `tasks/wave-smoke-build/task.toml` sets `sonnet` since P14 (PR #96; it set `haiku` before) [verified, `git show origin/development:tasks/wave-smoke-build/task.toml` at 2c6a557] (D6). the operator's rule of 2026-10-10 sets builders to sonnet, estate-wide, and supersedes his haiku rule of that day [reported,
  `.claude/agents/README.md` l.8 at eeb66de; mesh 4c2e9f04]. [reported, `run.rs:489-503`] a `task.toml` `model` overwrites
  the CLI `--model` with no condition. Whether `blackwall.toml` beats it is unknown. [verified, P9c run 0dbb7f02bf2d and `blackwall list`]
  the `running` line says `haiku`, and `blackwall list` shows `haiku` for all five P9c runs.
- **F13 [reported, DeltaDB design surface 3.1, draft, w19:p1]:** the cluster loop is
  cortex-worker, reflexd, escapement, blackwall-serve. `fork_from_delta` is not built.
- **F14 [verified, read `.claude/settings.json`]:** the project allows `Bash(cargo *)`,
  `Bash(make *)`, `Bash(git fetch *)` and `Bash(git rebase *)`. A `claude -p` critic cannot
  answer a prompt, so it needs `--allowed-tools` [asserted]. [verified, P9c run 20261010-141552-cas-cpm-wave-w1] the critic step ran and passed. Whether it ran `make check` is not provable: the record keeps the outcome, not the verdict lines (CER-2789).
- **F15 to F17 [reported, saga lane w1P:p2E, mesh 50f2d67a, code reading; no flow run for F16 and F17]:**
  F15: an omitted `agent` means auto-classify. **Retracted 2026-10-10 by the live run (P9c, our error in reading): saga d0d6b9e fails a step with no `agent` as "unassigned agent" in milliseconds (bifrost executor.rs:667); with no `agent` the run fails [verified, `saga run uas-cpm-wave-w0`, run 20261010-140148]; `agent = "auto"` parses with owner `auto` [verified, `saga run --dry-run` on the regenerated flows]; with `agent = "auto"` saga runs the agent, then `verify_cmd` [verified, run 20261010-141346-cas-cpm-wave-w0: both steps done, actors `agy` and `claude`], fixed in PR #81.** Every step of a kind other than gate runs the
  agent, then `verify_cmd`. So each non-gate step spends an agent run. F16: steps pass no
  values; `inputs` only order the DAG. One run shares one worktree. F17: a failed step
  cancels only the queued steps that depend on it. Running and independent siblings finish.
  Saga has no halt-all and no compensation.
- **F18 [verified, read `git show d0d6b9e:crates/bifrost/src/executor.rs` lines
  1112-1152]:** on saga main, a gate waits 600 s, hardcoded. A timeout returns a failed
  step, "heimdal gate timed out". [reported, saga lane] development 35ee74a reads
  `SAGA_HEIMDAL_GATE_TIMEOUT_SECS` and denies on timeout. [verified,
  `git merge-base --is-ancestor 35ee74a HEAD` exits 1] the saga checkout at d0d6b9e lacks
  35ee74a. [verified, `ls -l`] the installed `saga` links to its `target/release/saga`,
  built two minutes after d0d6b9e. That the binary holds d0d6b9e is an inference [heuristic].
- **F19 [verified, read `crates/uas-engine/src/compensator.rs`, `wave.rs`]:** Fixed by CER-2783 (PR fix/compensation-continue-on-failure, 2026-10-10): compensation continues past a failed compensator and spec 0009 names the field `details`.
  `Compensator::compensate` returns receipts in a `Result`. The field is `details`, not
  0009's `reason`. `CompositeCompensator` and `WaveCoordinator::rollback` stop at the first
  `Err`, against spec 0009. `BranchPruneCompensator` runs no git. F20 [verified,
  `grep -rn EVALUATOR_PATHS crates/` is empty; `evaluator_touched` matches] the spec 0023
  path list is not built.
- **F21 [verified, 2026-10-10T14:46Z, `gh` 2.93.0, commands as named]:** `gh pr checks 83 --required --json name,state,bucket` printed the five required checks of `development` (`secrets`, `rust / cargo`, `cargo-deny`, `hygiene`, `ebpf-live`), each `SUCCESS`. `gh pr checks --help`: `bucket` is pass, fail, pending, skipping or cancel; exit 8 means pending. `gh api repos/unsigned-gg/uas/branches/development/protection`: 0 required approvals, no required signatures, `enforce_admins` true; the repo allows merge commits only. `gh pr merge --help` lists `--merge` and `--match-head-commit SHA`.
  The five checks are jobs of `.github/workflows/ci.yml` [verified, read]. Over 86 green PR runs of that workflow (`gh run list --workflow CI --event pull_request --limit 100`, created to updated time): p50 49 s, p90 149 s, max 247 s. `git log --format=%G?` prints `G` for a commit signed here and `E` for a GitHub merge commit. Every wave receipt is `NotAudited` (`wave_exec.rs:538`) [verified, read].
- **F22 [reported, saga lane w1P:p2E, mesh 8365c486, code reading at saga d0d6b9e]:** a step `verify_cmd` has no time bound. `run_verify` (bifrost `executor.rs:1058`) blocks on `Command::output()` and kills nothing; a flow has no step timeout. [verified, 2026-10-10, GNU coreutils `timeout` 9.4] `timeout 1 bash -c 'sleep 7 & wait'` exits 124 and the grandchild `sleep` is gone; a control `sleep` started with no `timeout` lives on. [reported, saga@d0d6b9e:`.saga/flows/self-improve.toml` header] the `land` step kind is not built.
- **F23 [verified, 2026-10-10, read `gh api repos/unsigned-gg/soma/contents/blackwall/crates/blackwall-cli/src/merge_gate.rs?ref=ec076f6`; soma main is `ec076f6`, the installed build (spec 0022 F13)]:** `blackwall merge-gate <pr>` runs `gh pr checks <pr>`, not `--required`, and `gh pr view --json state,mergeable`. It ignores a skipped check, and it reads an empty check list with `MERGEABLE` as green (l.164-190, 249-270). `--watch` polls every 30 s; `--timeout` (default 1800 s) bounds only the pending rounds (l.324-349). Armed by `[merge] automerge = true` in the `blackwall.toml` of its cwd (`chain.rs:145`), or by the environment twin `BLACKWALL_AUTOMERGE=true`, which beats the file (`config.rs:183,212-213`), it runs `gh pr merge <pr> --squash --delete-branch`, with no head pin (l.423-427). Unarmed, a green PR prints `ready: <pr>` on stdout and exits 0; a merged PR exits 0. Red, closed, conflicting, timeout and `gh` errors return an error [reported: `main.rs` returns it, so the exit is non-zero]. `unsigned-gg/blackwall` main `3d98dc1` has the same merge line. [heuristic, `grep -a -c` on the installed binary] `--squash` counts 1, `match-head-commit` 0. The repo `blackwall.toml` sets `automerge = false` for this reason (l.20-24).

## Design

### Requirements (EARS)

- **Ubiquitous:** The `WaveExecutor` shall dispatch each step by its role (table).
- **Ubiquitous:** The executor shall run `blackwall`, `git`, `gh` and `claude` as subprocesses
  with fixed argv. It shall not link their crates.
- **Ubiquitous:** No `uas` code shall read PR checks or run `gh pr merge` (D15).
- **Ubiquitous:** Every step shall emit one `WorkflowEvent::WaveStep` (D5).
- **Ubiquitous:** The executor shall hold one host lock around each call that can build:
  reconcile, a critic run and a Probe (D8).
- **Ubiquitous:** The executor shall send the critic prompt on stdin (F7).
- **Ubiquitous:** The executor shall never approve a saga gate.
- **Event-driven:** WHEN a Builder run settles, the executor shall reconcile it with
  `--base <wave base>`. It shall open no PR then.
- **Event-driven:** WHEN a Builder reconciles, the executor shall diff the base against the
  reconciled head (D17).
- **Event-driven:** WHEN a Critic passes, the executor shall record one `LandingReceipt` per
  Builder it depends on.
- **Event-driven:** WHEN a step halts, the executor shall write a `Halted` record. Then it
  compensates the passed records, newest first.
- **State-driven:** WHILE the state directory holds a `Halted` record, the executor shall
  start no step, no reconcile, no critic run, no PR and no `blackwall merge-gate`.
- **Unwanted behavior:** IF the plan fails an admission row, THEN the executor shall refuse
  the plan before its first subprocess call.
- **Unwanted behavior:** IF a step is not ready, THEN `uas wave step` shall exit 2. It
  writes no record of its own and compensates nothing.
- **Unwanted behavior:** IF a step meets a halt row, THEN the step shall halt with that
  row's reason.
- **Unwanted behavior:** IF a compensation action fails, THEN the executor shall record a
  receipt with `success: false` and continue (spec 0009).
- **Unwanted behavior:** IF a `blackwall/<run>` branch exists on the remote, THEN
  compensation shall keep it and list it.
- **Unwanted behavior:** IF compensation runs, THEN it shall not discard, release or delete
  a blackwall run, or close a PR.
- **Optional feature:** WHERE `UAS_WAVE_LIVE=1` is set, the live smoke shall run the real
  tools. It fails, not skips, when a tool or variable is missing.
- **Optional feature:** WHERE `open_prs` is set (D16), the executor shall open the PR of each
  Builder whose Critic passed. Its body carries the receipt (spec 0022 D11).
- **Optional feature:** WHERE the source plan sets `[flow] merge_on_green` (D15), `uas wave flow` shall emit after each Critic one merge step per Builder it depends on. Its `verify_cmd` runs `blackwall merge-gate` through `uas wave merge-gate`, under `timeout`.
- **Event-driven:** WHEN a Builder's readiness meets an upstream Builder with a PR and no merge commit, the executor shall pre-read that PR's `mergeCommit`, bounded by `timeout` (D15). It records a 40-hex value on that Builder record and its receipt (D15).
- **Unwanted behavior:** IF `uas wave merge-gate` finds a `Halted` record, a plan that fails admission (`MergeWithoutOpenPrs`), no PR, a commit not signed `G`, no `.blackwall/init-head`, a `blackwall.toml` that does not parse with `[merge] automerge = false`, or a repo not given by `--repo` or `UAS_WAVE_REPO`, THEN it shall exit 1 and run no `blackwall`.

### Admission, readiness and halt

| Class | Case | Result |
|---|---|---|
| admission | a Builder has not exactly one Critic that directly depends on it | refuse (D10) |
| admission | a Builder has no `tasks/<id>/prompt.md` | refuse |
| admission | a Builder's `task.toml` lacks `model = "sonnet"` (F12, D6) | refuse |
| admission | a Probe has no `verify_cmd` | refuse |
| admission | a Probe depends on a Builder | refuse (D18) |
| admission | `wave-<step id>` is not a valid `SlotId` | refuse |
| admission | the source plan sets `[flow] merge_on_green` and `open_prs` is off (D15, D16) | refuse (`MergeWithoutOpenPrs`) |
| not ready | `<repo>/.blackwall/init-head` is absent | exit 2 |
| not ready | `git rev-parse HEAD` differs from `init-head` | exit 2 |
| not ready | a dependency that is not MainSession has no `Passed` record | exit 2 |
| not ready | a Builder step whose deps' closure holds a Builder that is not merged: with a PR on its record, no merge commit after the D15 pre-read, or one that is not an ancestor of the base; with no PR (D16 off), a head that is not an ancestor of the base (D7, lane decision w25:p1, 2026-10-10). The step names the PR. A Critic never waits on a merge: its own Builder's PR opens only after it passes. | exit 2 |
| not ready | the step's own `Passed` record names another base than the current `init-head`, or no base: the record is stale, so the step prints why and exits 2 with no new record (lane decision w25:p1, 2026-10-10; the operator may override) | exit 2 |
| halt | `start` prints no `(ok)` run line (F3) | `BuilderRunFailed` |
| halt | `start` prints no `changeset:` line (F3) | `BuilderRunFailed` |
| halt | reconcile does not return `Reconciled` | `VerifyFailed`, `NoVerifyGate` or `StaleBase` |
| halt | the base-to-head diff touches a D17 halt path | `EvaluatorTouched` |
| halt | the verdict's first line is `VERDICT: DEFECTS` | `CriticDefects` |
| halt | another first line, over 15 lines, or a non-zero exit | `CriticUnparsed` |
| halt | a Probe exits non-zero | `ProbeFailed` |
| halt | WHERE `open_prs` (D16), `open_pr` returns Err after a Critic pass. D4 applies to the branch: pushed stays, unpushed is deleted | `PrFailed` |
| halt | a `Halted` record appears after the step started | `WaveHalted` |
| halt | the executor itself fails before an outcome (a missing tool on PATH, an I/O or record error). The step writes a `Halted` record so siblings and later waves stop (F17), prints the reason on stderr, and `uas wave step` exits 3. It runs no compensation: the fault is in the environment, not the work, and the operator re-runs the step once it is fixed. WHERE the state records themselves cannot be read or written, the step exits 3 with the reason only and no record: the halt writer reads the same directory (lane decisions w25:p1, 2026-10-10; the operator may override) | `ExecutorError` |

### Dispatch by role

| Role | Persona (D6) | Subprocess argv, in order | Pass when |
|---|---|---|---|
| Builder | `uas-builder` | 1. `blackwall start --task <id> --provider claude --model sonnet`. 2. `blackwall settle <run> select`. 3. Under the lock, `git merge-base --is-ancestor <base> <dev>`, then `blackwall reconcile <run> --base <base> --branch blackwall/<run>`. 4. `git rev-parse blackwall/<run>`. 5. `git diff --name-only -z --no-renames <base> <head>`, split on NUL (D17). | an `(ok)` line, a changeset line, `Reconciled`, no evaluator path |
| Critic | `uas-critic` | Per Builder dependency, under the lock: 1. `git worktree add --detach <scratch>/<critic>-<builder> <head>`. 2. `git -C <tree> checkout --no-overlay <base> -- .claude`. 3. In the tree, `claude -p --strict-mcp-config --agent uas-critic --allowed-tools Read Grep Glob 'Bash(git diff *)' 'Bash(git log *)' 'Bash(git show *)' 'Bash(make *)' 'Bash(cargo *)'` (F14). The prompt goes on stdin, with no `--model`. 4. `git worktree remove --force`, on every exit path. 5. On a pass and WHERE `open_prs` (D16), `open_pr`: `blackwall pr <run> --base development --body -`. Stdin is the spec 0022 D11 fenced JSON of the Builder record's reconciled receipt. | F10 first line NO DEFECTS, 15 lines or fewer, exit 0 |
| Probe | none | Under the lock: 1. `git worktree add --detach <scratch>/<probe> <base>`. 2. The step's `verify_cmd`, in that tree. 3. `git worktree remove --force`. | exit 0 |
| MainSession | none | none: the saga gate blocks until `saga gate approve` | saga approves |

`<run>` is the 12-character prefix from the `start` line (D12). `<scratch>`
lies outside the custody checkout, because `init` copies the tree [reported,
custody-run skill, step 2]. The critic prompt names the base, the branch, the
head, the reconcile receipt and `tasks/<builder id>/prompt.md`. A Critic's
`<base>` is the base in its Builder's record. D17 halts on any `.claude/` change
first; the base checkout guards a defect in that diff. Overlay mode would keep a
head-only `.claude/settings.json` [verified, 2026-10-10, git 2.43 scratch repo].

### Compensation

`WaveCompensator` implements the spec 0024 `Compensator` trait (F19). It holds
the step records and never returns `Err`. The executor calls it once per passed
record, newest first, and keeps every `CompensationReceipt`. It does not use
`CompositeCompensator` or `rollback`. For a Builder record, in order:

1. The record has a PR number: keep everything.
2. `git ls-remote --exit-code --heads origin blackwall/<run>` exits 0: a `pr`
   may have failed after its push. Keep the branch and list it.
3. It exits 2, no such ref: `git branch -D blackwall/<run>`. [verified, 2026-10-10,
   git 2.43: absent ref 2, present ref 0, bad remote 128]
4. Any other exit, or a failed `branch -D`: keep the branch, `success: false`.

A Critic or Probe record leaves no state. The halt report lists the runs, PRs
and kept branches for the operator.

### The saga flows

`uas wave flow --from <source plan> --out-dir .saga/flows` writes one flow per non-empty
wave, `uas-cpm-wave-w<i>.toml`, with `<i>` the plan's wave index, so a wave emptied by a
moved gate gets no file (D2). A literal `verify_cmd` cannot carry a wave parameter (F9, F16). Each flow holds:

- one step per `WaveStep` of the wave: impl (Builder), review (Critic) or test
  (Probe). Its `verify_cmd` is the literal argv (plus `--max-concurrency N` WHERE the plan was generated with N != 0, so the flow regenerates as written) `["uas", "wave", "step",
  "--flow", "<source>", "--step", "<id>"]`. It carries `agent = "auto"`, the
  value saga auto-classifies (F15, as retracted). The prompt tells that agent to do nothing else (D2).
- a gate step (`heimdal:approval`) for each MainSession step with a direct
  dependent in the wave. Those dependents depend on it. A MainSession step
  with no dependent stays in its own wave's flow.
- WHERE the source plan sets `[flow] merge_on_green = true` (D15): after each Critic, one test step per Builder it depends on, id `merge-<builder id>`, `deps = ["<critic id>"]`, `agent = "auto"` (`land` is not built, F22). Its `verify_cmd` is `["timeout", "-k", "30", "<merge_wait + 60>", "uas", "wave", "merge-gate", "--flow", "<source>", "--builder", "<builder id>"]`, plus `--max-concurrency N` as above. `uas wave flow` refuses a plan that already holds such an id.
- `deps` that name only steps in the same flow. `uas wave step` checks deps on
  earlier waves from their records.
- `[limits] max_in_flight`, the plan's `max_concurrency`. No gate sits between waves.

Between two runs, the wave's open PRs merge as D15 rules: through the merge step's `blackwall merge-gate` when armed, else by the lane. Before a wave with a
dependent Builder, the integrator runs `git fetch`, moves the custody checkout
HEAD to the new development head and runs `blackwall init .`; no gate waits on CI (F18).

`uas wave step` re-plans the source (ties break on step id) and runs one step. Its record
goes to `--state-dir`, else `UAS_WAVE_STATE_DIR`, else
`${XDG_STATE_HOME:-$HOME/.local/state}/uas/wave/<plan>/`, outside the repo, because `init`
copies the tree. All wave runs of one plan share it. The step finds the custody checkout
from `--repo`, else `UAS_WAVE_REPO`, else its cwd. Saga may run `verify_cmd` in a run
worktree with no `init-head` (F16); the step then exits 2. A `Passed` record on the current
base exits 0 and runs nothing, so a saga retry is safe. Any `Halted` record makes the step
exit 1 and run nothing. Only a person removes one.

`uas wave merge-gate` finds the state directory as `uas wave step` does. It takes the custody checkout only from `--repo` or `UAS_WAVE_REPO`, never from its cwd, because saga runs `verify_cmd` in a run worktree (F16) that the shim agent writes first (D2). The emitted argv has no `--repo`, so the saga run sets `UAS_WAVE_REPO`: `run_verify` removes only `SAGA_MOCK_HARNESS`, so the step inherits the run env [verified, read saga@70f852e `crates/bifrost/src/executor.rs` l.1058-1071]. A saga run without it fails every merge step closed (exit 1). It reads the Builder's `Passed` record. Any `Halted` record, a plan that fails admission (`MergeWithoutOpenPrs`), no PR on the record, no `.blackwall/init-head` in the checkout, a checkout `blackwall.toml` that does not parse with `[merge] automerge = false`, or a `git log --format=%G? <base>..<head>` that prints no line or a line other than `G` makes it exit 1 and run nothing. Else it removes `BLACKWALL_AUTOMERGE` from the environment and execs `blackwall merge-gate <pr> --watch --timeout <merge_wait>` with the custody checkout as cwd. The exit is merge-gate's, or 124 from the outer `timeout` (F22). It reads no check and writes no record. The `automerge = false` guard holds while the installed merge-gate squashes with no head pin (F23; EST-558 [reported, mesh 40481bdc]). The PR that arms merge-gate also lifts that guard; from then on the reviewed `blackwall.toml` is the only switch (D15).

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | `WaveExecutor` in `uas-engine` dispatches by role (table above). A Builder is a jailed `blackwall start` and `settle select`, then the landed `LandingActivity`. Amendment: `LandingActivity` splits into `reconcile` and `open_pr`, and `land` keeps calling both. `open_pr` takes the reconciled receipt and writes it as the spec 0022 D11 body. The PR opens only after the Critic passes (F10). | Caller's default for the operator's roadmap item, lane w25:p1, 2026-10-10; amendment uas-engineer, 2026-10-10 (the operator can override) |
| D2 | Saga drives one run per wave, through the generated `uas-cpm-wave-w<i>.toml`. Saga owns the run journal and the gates. `uas-engine` owns the plan, the custody calls, the halt and the compensation, because saga has neither (F17). Each step is an agent shim whose `verify_cmd` runs `uas wave step`. The shim spends one agent run per step, Probes included (F15). | Caller's default, lane w25:p1, 2026-10-10; one run per wave and the shim cost, uas-engineer, 2026-10-10 (the operator can override) The shim agent runs before `verify_cmd` in a writable worktree, so the relative plan path is agent-writable: CER-2788. |
| D3 | Process boundary: subprocesses with exact argv, as `LandingActivity` does. Every exit and output contract is `asserted` or `reported` until the P9c live run records it. | Caller's default, lane w25:p1, 2026-10-10 |
| D4 | Halt and compensate as the requirements and the Compensation section say. Blackwall runs stay for inspection. PRs and pushed branches stay; the halt report lists them. | Caller's default, lane w25:p1, 2026-10-10 |
| D5 | Evidence: a new `WorkflowEvent::WaveStep` per step, with role, persona, run, changeset, base, branch, head, PR and outcome. A merge commit goes on the Builder record and its receipt, not on an event (D15). The wave result is one `LandingReceipt` per Builder, plus the halt reason and the compensation receipts. | Caller's default, lane w25:p1, 2026-10-10 |
| D6 | Models by persona. Builder `uas-builder` runs as `--model sonnet`, because `start` takes no persona (F6). Its `task.toml` must say `model = "sonnet"`, because that file beats the flag (F12). Critic `uas-critic` runs through `--agent` with no `--model`, so its opus line holds (F7). The executor does not dispatch `uas-foreman`. Probe and MainSession use no model. D6 supersedes 0024's `ModelTier::Pro` for dispatch. | the operator's model rule, 2026-10-10 (sonnet, estate-wide; it supersedes his haiku rule of that day); mapping lane w25:p1, 2026-10-10; a wave Builder is a builder, so sonnet: lane reading w25:p1, 2026-10-10, mesh 4c2e9f04 (the operator may override) |
| D7 | Base per wave: `.blackwall/init-head`, with HEAD equal to it (F5). A Builder waits until every upstream Builder in its deps' closure has merged: its recorded merge commit (D15 pre-read) is an ancestor of the base. Merge commits make each reconciled head an ancestor of the next base. The wait falls between saga runs, so no gate waits on CI (F18). No `--parent` stacking. | uas-engineer, 2026-10-10 (the operator can override) |
| D8 | One host-wide lock file (`File::lock`, F11) serializes reconcile, critic runs and Probes. So at most one `make check JOBS=2` runs at a time (rule 2, spec 0020). `start` runs may overlap up to `max_concurrency`. | uas-engineer, 2026-10-10 |
| D9 | A wave Builder has no UAS slot (F2). Its receipt sets `slot_id = wave-<step id>`, `audit = NotAudited` (a new `AuditStatus` variant), `run_hash` empty and `cpu_cap = "uncapped"`. This extends 0022 D4 to runs with no audit at all. **Ruled: `NotAudited` lands, flagged.** The receipt and the PR body say so, and the planner warns. A `NotAudited` wave PR may be merged by the D15 merge step with the flag in its body (D19). | the operator, 2026-10-10, in session (lane w25:p1 interview); lane default uas-engineer |
| D10 | Plan admission checks the admission rows before any subprocess runs. One Critic per Builder means one PR opener per Builder. | uas-engineer, 2026-10-10 |
| D11 | The blackwall task name is the step id: `tasks/<step id>/`. | uas-engineer, 2026-10-10 |
| D12 | Receipts carry the 12-character run and changeset prefixes that `start` prints (F3). The other subcommands resolve prefixes (F4); `show` does not (Notes). [verified, P9c runs 0dbb7f02bf2d, 20261010-141346-cas-cpm-wave-w0 and 20261010-141552-cas-cpm-wave-w1] `settle` and `reconcile` took the 12-character run id, the records carry run 47c34f0dd060 and changeset 4abbf3bebe21, reconcile reached head 42560966 on `blackwall/47c34f0dd060`, and no PR opened (D16). Full hashes wait for `start --json` (CER-2787). | uas-engineer, 2026-10-10 |
| D13 | The critic reviews in a transient detached worktree at the reconciled head. It runs the gates (F10) and leaves HEAD in the custody checkout alone (F5). | uas-engineer, 2026-10-10 |
| D14 | First live run: the local `blackwall` CLI on ceres, or blackwall-serve in the cluster through cortex-worker, reflexd and escapement (F13). Lane proposal: local, because the 1.2.0 `serve` only inspects (F6). **Ruled: the local CLI on ceres.** P9c ran on it (runs 20261010-141346-cas-cpm-wave-w0 and 20261010-141552-cas-cpm-wave-w1); a blackwall-serve run needs its own brief when `serve` executes. | the operator, 2026-10-10, in session (lane w25:p1 interview); proposal lane w25:p1 |
| D15 | Merging a wave's PRs. **Ruled: uas implements no merge flow; merging runs through saga and blackwall.** WHERE the source plan sets `[flow] merge_on_green = true`, `uas wave flow` emits after each Critic one merge step per Builder (The saga flows). A literal `verify_cmd` cannot carry the PR number (F9, F16), so the step runs `uas wave merge-gate`. That resolver reads the PR from the Builder record, checks signatures and execs `blackwall merge-gate <pr> --watch --timeout <merge_wait>` in the custody checkout. An outer `timeout -k 30 <merge_wait + 60>` bounds the step, because saga bounds none (F22). `merge_wait` is `[flow] merge_wait_secs`, default 900 s, about 3.6 times the 247 s max in F21. Each merge step is a shim and spends one agent run (D2). The executor never polls checks and never merges. **Pre-read:** a Builder's readiness runs `timeout -k 5 30 gh pr view <n> --json mergeCommit --jq .mergeCommit.oid` in the custody checkout for each upstream Builder in its deps' closure with a PR and no merge commit. Forty lowercase hex characters go, under the host lock, to that Builder record and its receipt (`LandingFacts.merge_commit`, spec 0022). Exit 125, 126 or 127 from `timeout` (it failed, `gh` is not executable, `gh` is not found) is an `ExecutorError` (exit 3), like any missing tool [verified, coreutils 9.4]. Empty output, `null`, a timeout (124, or 137 after the kill) or any other non-zero exit records nothing: the step is not ready (exit 2) and never blocks, because saga bounds no `verify_cmd` (F22). "Merged" means a recorded merge commit that is an ancestor of the base (D7). **Arming:** merge-gate merges only WHERE `blackwall.toml` sets `[merge] automerge = true`; the resolver strips the `BLACKWALL_AUTOMERGE` twin (F23), so a reviewed file is the only switch. It stays `false`: an armed merge-gate at soma `ec076f6` runs `gh pr merge --squash --delete-branch` with no head pin (F23). The integrator sets it `true`, in a PR that also lifts the resolver's `automerge = false` guard (The saga flows), only after a blackwall build merges with `--merge --match-head-commit <head>` and reads an empty check list as pending. Until then a passing merge step prints `ready: <pr>`; because F23 reads an empty check list as green, `ready` can print before any check registers, so the lane confirms `gh pr checks <pr> --required` on the exact head before it merges (F21 branch protection is the backstop). The lane then merges by hand with `gh pr merge <n> --merge --match-head-commit <head>`. The pre-read records either kind of merge. **Failure:** a red, closed or conflicting PR, a merge-gate timeout (exit 124, or 137 after the kill), no PR, an unsigned commit or any other resolver refusal (The saga flows) fails the merge step in saga. Saga cancels only its dependents (F17). The step writes no `Halted` record and compensates nothing. The next Builder step that needs that PR reads no merge commit, exits 2 and names the PR. **NEVER:** a merge on red (merge-gate exits non-zero on red; protection requires the five checks, F21); an unsigned commit (the resolver refuses); a merge with no `--match-head-commit`; a squash (both wait on the blackwall change, so `automerge` stays `false`). The executor merge of P12a is retired and reverted (P13a). | the operator, 2026-10-10, in session (lane w25:p1 interview): "uas must not implement its own merge flow; merging runs through saga and blackwall". It supersedes his earlier D15 ruling that the executor may merge on green. Reason recorded with it: saga owns the run journal and gates, blackwall owns landing, and uas owns custody calls, halts and records. The resolver, the env strip, the pre-read, the base check, the defaults and `automerge = false`: lane decisions w25:p1, 2026-10-10; the operator may override. The resolver guards (repo only from `--repo` or `UAS_WAVE_REPO`; no init-head; `automerge = false` until the arming PR): lane decision w25:p1, 2026-10-10, mesh 40481bdc. The 30 s pre-read bound: uas-engineer, 2026-10-10, foreman request mesh 1377f5e6; the operator may override **Interim ruled (the operator, 2026-10-10, in session, lane w25:p1 interview): merge-gate runs unarmed and reports `ready: <pr>` on green; the lane merges by hand with `--merge --match-head-commit`; arm it only once an installed blackwall merges that way.** |
| D16 | `open_prs` is off by default, so a wave run stops after the critic verdict and opens no PR. `UAS_WAVE_LIVE_PR=1` sets it for the live smoke. `UAS_WAVE_OPEN_PRS=1` sets it for `uas wave step`, because a literal `verify_cmd` carries no flag. The integrator, or the D15 merge step when armed, then lands each PR; the integrator closes the rest. | uas-engineer, 2026-10-10 |
| D17 | Before a critic, the executor runs `git diff --name-only -z --no-renames <base> <head>` and splits the output on NUL. Without `-z`, git C-quotes a non-ASCII path, so no pattern matches it. Without `--no-renames`, it lists only a rename's destination, so a renamed-away `Makefile` is not seen [verified, 2026-10-10, git 2.43 scratch repo]. The halt set: a path under `.claude/`; `.mcp.json`; `**/CLAUDE.md`, `**/CLAUDE.local.md` and `**/AGENTS.md`; and the spec 0023 D4 and D6 lists. A `**/` pattern matches at the root too. The rule files are in the set because `claude -p` loads the root `CLAUDE.md` of its cwd. That file names `AGENTS.md` as the only rule source. It also loads a nested `CLAUDE.md` when the critic reads that directory [reported, critic-0025-3]. So a Builder could rewrite its own rubric. `claude -p` starts a project `.mcp.json` server from its cwd with no approval [reported, critic-0025-4]. The Critic argv sets `--strict-mcp-config`, which ignores all other MCP configurations [verified, 2026-10-10, `claude --help`, 2.1.296]. Spec 0023 D6 lists every `Cargo.toml` and `build.rs`, and `crates/uas-ebpf/ebpf/**`, fail closed. `build. `crates/uas-ebpf/ebpf/**` is in the set because `build.rs` trusts the object and its `.sha256` as a pair; the reason and its evidence are in spec 0023 D6. Rebuilding that object is kernel work, never a wave Builder step. So a Builder that touches one of these paths halts the wave, and the executor compensates its siblings' passed records. A dependency add is not wave Builder work, because rule 12 needs a spec 0004 pin first. A match sets `evaluator_touched` and halts the step. The executor rebuilds the receipt with `LandingReceipt::new`, so the tier is `reported`. The critic never loads a persona, settings, rule file, MCP server or gate from the changeset. The branch stays; a person may open its PR at `reported` (0023 D3). | uas-engineer, 2026-10-10; halt set widened per critic-0025-3 and critic-0025-4 (`.mcp.json`, `--strict-mcp-config`, 0023 D6), lane w25:p1, 2026-10-10; `crates/uas-ebpf/ebpf/**` via 0023 D6, lane decision w25:p1, 2026-10-10; narrows the wave path of the operator's 0023 D3 (the operator can override) |
| D18 | A Probe runs at the wave base in a transient worktree under the lock. It checks the base, not a Builder's output; the Critic checks that. | uas-engineer, 2026-10-10 |
| D19 | Every wave receipt is `NotAudited` (F21). **Ruled: a `NotAudited` wave PR merges, flagged in the body.** The PR already carries a Critic pass and green CI; the exclusion of D9 and D15 is withdrawn, so there is no `MergeNotAudited` halt. It holds under the D15 redirect: `blackwall pr` opened the PR with the D11 receipt body, so the flag is in the PR that merge-gate or the lane merges. | the operator, 2026-10-10, in session (lane w25:p1 interview); raised by lane w25:p1 |

## Plan

- [x] P9a, `uas-engine` (plus `uas-domain`), `uas-builder`, briefed on haiku; landed in PRs #75 and #77. It adds `AuditStatus::NotAudited`,
  and `EVALUATOR_PATHS` with `is_evaluator_path` (the spec 0023 plan item). It splits
  `LandingActivity` into `reconcile` and `open_pr`, with `land` unchanged. `open_pr` reuses
  P5c's `--body -` code. `WaveExecutor` gets admission, dispatch, the D17 diff, the verdict
  parse, the host lock, `WorkflowEvent::WaveStep` and `WaveCompensator`. Tests use fake
  `blackwall`, `git` and `claude` scripts, red first. It starts after P5b and P5c (spec 0022
  D7, D9 to D11) land: all three edit `landing.rs`.
- [x] P9b, `uas-cli`, `uas-builder`, briefed on haiku; landed in PR #78. It adds `uas wave flow` and `uas wave step` with the state directory, and the source plan
  `.saga/plans/uas-wave-smoke.toml`. It generates `.saga/flows/uas-cpm-wave-w*.toml`
  and adds `tasks/wave-smoke-build/`. Tests: a round-trip and a drift test.
- [x] P9c, integrator on ceres, once D14 is ruled: `blackwall doctor`,
  `blackwall init .` in a clean checkout at `origin/development`, the live smoke,
  then the saga runs `uas-cpm-wave-w0` and `uas-cpm-wave-w1`. Record every
  output and exit contract, then amend D12 and F3 to F5. Done 2026-10-10 at 642c3f2: smoke run 7878209325ad, saga runs 20261010-141346-cas-cpm-wave-w0 and 20261010-141552-cas-cpm-wave-w1. Not shown: the F5 stale root, and whether `init` keeps earlier runs.
- [x] the operator ruled D9, D14 and D15 on 2026-10-10. D19 ruled 2026-10-10: a `NotAudited` wave PR merges, flagged. D15 re-ruled the same day: merging runs through saga and blackwall, so P12a and P12b are retired.
- [x] P13a, integrator: revert P12a (PR #89, merged 5f182db). It removes the executor merge, its halts and tests, and `merge_commit` from `uas-domain` and the event.
- [x] P13b, landed in PR #95 (dc288d6), `uas-builder` sonnet: one package over `uas-engine` and `uas-cli` plus the one `uas-domain` field (the caller asked for one package; lane w25:p1, 2026-10-10). It adds D15's plan keys, the admission row, the merge step in `uas wave flow`, `uas wave merge-gate` and the readiness pre-read with `merge_commit`. Tests go red first against a fake `blackwall` and a fake `gh` that fails on `pr merge` and `pr checks`.
- [x] P14 (CER-2796), landed in PR #96 (e6c570d), `uas-builder` sonnet: the D6 default model and `tasks/wave-smoke-build/task.toml` move to sonnet, with the test fixtures.
- [x] P15 (CER-2797), after P14, `uas-builder` sonnet: wrap the readiness pre-read in `timeout -k 5 30` and map its exits as D15 Pre-read says. Red-first tests for 124, 127 and a 40-hex pass. Ticked 2026-10-11 on reading `tests/wave_exec.rs` (`wave_preread_timeout_records_nothing` line 559, `wave_preread_gh_missing_is_executor_error` 583, `wave_preread_bounded_pass` 602), not on a run.

## Test

```bash
make spec-check
cargo nextest run -p uas-domain --build-jobs 2 landing_receipt
cargo nextest run -p uas-domain --build-jobs 2 evaluator_path
cargo nextest run -p uas-engine --build-jobs 2 landing
cargo nextest run -p uas-engine --build-jobs 2 --test wave_exec
cargo nextest run -p uas-cli --build-jobs 2 --test wave_flow
```

A pass means every test above is green with the fakes. The live smoke (P9c)
runs on ceres only, after `blackwall init .`, with `L` the clean checkout:

```bash
UAS_WAVE_LIVE=1 UAS_WAVE_REPO=$L UAS_WAVE_PLAN=$L/.saga/plans/uas-wave-smoke.toml \
  cargo nextest run -p uas-engine --build-jobs 2 --test wave_exec --run-ignored only live_smoke
```

## Notes

- Rejected: stacking a dependent Builder on its upstream world with `start --parent`. Its whole-file blob would carry the upstream edits onto a base without them: the revert trap in the custody rebasing SOP.
- Rejected: opening the PR in the Builder step, as `land` does. It would precede the critic.
- Rejected: the executor polls the checks and runs `gh pr merge` (P12a, landed 8315a6b). the operator's 2026-10-10 ruling gives merging to saga and blackwall (D15).
- Rejected: one saga run with a gate between waves; it would wait on CI and the merge, past the 600 s window (F18).
- P9c settled neither unknown. `blackwall show 7878209325ad` returned `NotFound`, though `list` prints that 12-character id [verified, run 7878209325ad], so whether `show` prints the base (custody-run skill against F4) is still open. `init` ran once, before any run, so whether it keeps earlier runs (D4) is still open.
- Under `cargo nextest` the smoke's reconcile halted `VerifyFailed`, yet the smoke exited 0 because it passes on any outcome, while the same run reconciled outside nextest [verified, runs 7878209325ad and 0dbb7f02bf2d]; the cause is not proven [heuristic: the inherited nextest environment], and the receipt keeps no reconcile stderr (CER-2789).
- Amends spec 0022 D1 and D9 (its D12 logs it), and adds `LandingFacts.merge_commit`, set only by the D15 pre-read (its amendment log). P9a builds 0023's `EVALUATOR_PATHS`.
- Out of scope: the `?` at `wave.rs:421` and `compensator.rs:339` in `crates/uas-engine/src` [verified at b149d73] stops compensation at the first `Err`, against spec 0009 (F19). Ticket CER-2783 is the fix package, not P9a.
- Open with the saga lane: a command-only step kind (D2 cost), the `verify_cmd`
  cwd, and when 35ee74a ships. Saga bounds no `verify_cmd` (F22), so D15 puts `timeout` in the argv. Open with the blackwall owners: overlapping
  `start` runs on one store, a full-hash or JSON `start` output, and a merge-gate that merges with `--merge --match-head-commit <head>` and reads an empty check list as pending (F23, D15).
