---
status: planned
created: 2026-10-10
tags: [engine, wave, custody, blackwall, saga, cpm]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0008-soma-cortex-saga-reverie-integration, 0009-saga-compensation-runner-and-durability, 0020-build-job-cap, 0022-custody-landing, 0023-test-time-compute-and-verifiable-reward, 0024-estate-saga-cpm-and-agent-sprint-wave-strategy]
---

# 0025-wave-execution-on-the-custody-chain

## Overview

Roadmap item from Christian, 2026-10-10, lane w25:p1: "implement cpm and agent waves with deltadb/blackwall and saga".
Spec 0024 (landed in b409386) plans waves: `WavePlan`, `WaveCoordinator`, the roles Probe, Builder, Critic and MainSession, and `cas flow --wave`. It executes nothing on the custody chain.
This spec makes each wave step run on that chain. A Builder is a jailed blackwall run. A Critic is a `cas-critic` run. A Probe is a command. A MainSession step is a saga gate. Saga drives one run per wave. `cas-engine` owns the plan, the custody calls, the halt and the compensation.

Facts, 2026-10-10. Source facts from `unsigned-gg/blackwall@3d98dc1` are `reported`: that checkout is dirty (`run.rs` has 6 local lines).

- **F1 [verified, read `crates/cas-engine/src/wave.rs`]:** `validate_verdict` checks only the 15-line cap; nothing parses a verdict.
  `classify_step` gives Builder and Critic `ModelTier::Pro`. `cas flow --wave` runs each step's `verify_cmd` in-process (`crates/cas-cli/src/main.rs:568-640`).
- **F2 [verified, read `crates/cas-engine/src/landing.rs`]:** `land` runs the base check, `reconcile` and `pr` in one call, never with `--draft`.
  Its input needs a slot id, a `run_hash`, an `AuditStatus` and a `cpu_cap`. A wave run has no CAS slot, so none has an honest value.
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
- **F10 [verified, read `.claude/agents/cas-critic.md`]:** the critic's first line is
  `VERDICT: DEFECTS` or `VERDICT: NO DEFECTS`. It says NO DEFECTS only after it ran the
  done-when command and the gates itself.
- **F11 [verified, probe on rustc 1.94.1]:** `std::fs::File::lock` works on the pinned
  toolchain. A host lock needs no new dependency.
- **F12 [verified, read `blackwall.toml` and `tasks/*/task.toml`, 2026-10-10]:** `blackwall.toml`
  sets `sonnet`; `tasks/wave-smoke-build/task.toml` sets `haiku`. Christian's rule of 2026-10-10 sets builders to haiku [reported,
  `.claude/agents/README.md`]. [reported, `run.rs:489-503`] a `task.toml` `model` overwrites
  the CLI `--model` with no condition. Whether `blackwall.toml` beats it is unknown. [verified, P9c run 0dbb7f02bf2d and `blackwall list`]
  the `running` line says `haiku`, and `blackwall list` shows `haiku` for all five P9c runs.
- **F13 [reported, DeltaDB design surface 3.1, draft, w19:p1]:** the cluster loop is
  cortex-worker, reflexd, escapement, blackwall-serve. `fork_from_delta` is not built.
- **F14 [verified, read `.claude/settings.json`]:** the project allows `Bash(cargo *)`,
  `Bash(make *)`, `Bash(git fetch *)` and `Bash(git rebase *)`. A `claude -p` critic cannot
  answer a prompt, so it needs `--allowed-tools` [asserted]. [verified, P9c run 20261010-141552-cas-cpm-wave-w1] the critic step ran and passed. Whether it ran `make check` is not provable: the record keeps the outcome, not the verdict lines (CER-2789).
- **F15 to F17 [reported, saga lane w1P:p2E, mesh 50f2d67a, code reading; no flow run for F16 and F17]:**
  F15: an omitted `agent` means auto-classify. **Retracted 2026-10-10 by the live run (P9c, our error in reading): saga d0d6b9e fails a step with no `agent` as "unassigned agent" in milliseconds (bifrost executor.rs:667); with no `agent` the run fails [verified, `saga run cas-cpm-wave-w0`, run 20261010-140148]; `agent = "auto"` parses with owner `auto` [verified, `saga run --dry-run` on the regenerated flows]; with `agent = "auto"` saga runs the agent, then `verify_cmd` [verified, run 20261010-141346-cas-cpm-wave-w0: both steps done, actors `agy` and `claude`], fixed in PR #81.** Every step of a kind other than gate runs the
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
- **F19 [verified, read `crates/cas-engine/src/compensator.rs`, `wave.rs`]:** Fixed by CER-2783 (PR fix/compensation-continue-on-failure, 2026-10-10): compensation continues past a failed compensator and spec 0009 names the field `details`.
  `Compensator::compensate` returns receipts in a `Result`. The field is `details`, not
  0009's `reason`. `CompositeCompensator` and `WaveCoordinator::rollback` stop at the first
  `Err`, against spec 0009. `BranchPruneCompensator` runs no git. F20 [verified,
  `grep -rn EVALUATOR_PATHS crates/` is empty; `evaluator_touched` matches] the spec 0023
  path list is not built.
- **F21 [verified, 2026-10-10T14:46Z, `gh` 2.93.0, commands as named]:** `gh pr checks 83 --required --json name,state,bucket` printed the five required checks of `development` (`secrets`, `rust / cargo`, `cargo-deny`, `hygiene`, `ebpf-live`), each `SUCCESS`. `gh pr checks --help`: `bucket` is pass, fail, pending, skipping or cancel; exit 8 means pending. `gh api repos/cerebral-work/cas-substrate/branches/development/protection`: 0 required approvals, no required signatures, `enforce_admins` true; the repo allows merge commits only. `gh pr merge --help` lists `--merge` and `--match-head-commit SHA`.
  The five checks are jobs of `.github/workflows/ci.yml` [verified, read]. Over 86 green PR runs of that workflow (`gh run list --workflow CI --event pull_request --limit 100`, created to updated time): p50 49 s, p90 149 s, max 247 s. `git log --format=%G?` prints `G` for a commit signed here and `E` for a GitHub merge commit. Every wave receipt is `NotAudited` (`wave_exec.rs:538`) [verified, read].

## Design

### Requirements (EARS)

- **Ubiquitous:** The `WaveExecutor` shall dispatch each step by its role (table).
- **Ubiquitous:** The executor shall run `blackwall`, `git` and `claude` as subprocesses
  with fixed argv. It shall not link their crates.
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
  start no step, no reconcile, no critic run, no PR and no merge.
- **Unwanted behavior:** IF the plan fails an admission row, THEN the executor shall refuse
  the plan before its first subprocess call.
- **Unwanted behavior:** IF a step is not ready, THEN `cas wave step` shall exit 2. It
  writes no record and compensates nothing.
- **Unwanted behavior:** IF a step meets a halt row, THEN the step shall halt with that
  row's reason.
- **Unwanted behavior:** IF a compensation action fails, THEN the executor shall record a
  receipt with `success: false` and continue (spec 0009).
- **Unwanted behavior:** IF a `blackwall/<run>` branch exists on the remote, THEN
  compensation shall keep it and list it.
- **Unwanted behavior:** IF compensation runs, THEN it shall not discard, release or delete
  a blackwall run, or close a PR.
- **Optional feature:** WHERE `CAS_WAVE_LIVE=1` is set, the live smoke shall run the real
  tools. It fails, not skips, when a tool or variable is missing.
- **Optional feature:** WHERE `open_prs` is set (D16), the executor shall open the PR of each
  Builder whose Critic passed. Its body carries the receipt (spec 0022 D11).
- **Optional feature:** WHERE `merge_on_green` is set (D15), with an open PR and no merge recorded, the executor shall poll that PR's required checks outside the host lock, in bounded rounds with a sleep between rounds.
- **Event-driven:** WHEN every required check is `SUCCESS` on the recorded head, the executor shall run `gh pr merge <n> --merge --match-head-commit <head>`, read the merge commit back, and record it on the receipt, the record and the event.
- **Unwanted behavior:** IF a commit is unsigned, a check is red, the rounds run out, the PR head moves or the merge fails, THEN the step shall halt with the D15 reason and merge nothing; before merging and on an empty read-back it shall read `mergeCommit`, and record it WHEN set.

### Admission, readiness and halt

| Class | Case | Result |
|---|---|---|
| admission | a Builder has not exactly one Critic that directly depends on it | refuse (D10) |
| admission | a Builder has no `tasks/<id>/prompt.md` | refuse |
| admission | a Builder's `task.toml` lacks `model = "haiku"` (F12) | refuse |
| admission | a Probe has no `verify_cmd` | refuse |
| admission | a Probe depends on a Builder | refuse (D18) |
| admission | `wave-<step id>` is not a valid `SlotId` | refuse |
| admission | `merge_on_green` is set and `open_prs` is not (D15) | refuse (`MergeWithoutOpenPrs`) |
| not ready | `<repo>/.blackwall/init-head` is absent | exit 2 |
| not ready | `git rev-parse HEAD` differs from `init-head` | exit 2 |
| not ready | a dependency that is not MainSession has no `Passed` record | exit 2 |
| not ready | a Builder step whose deps' closure holds an unmerged Builder (D7). A Critic never waits on a merge: its own Builder's PR opens only after it passes. | exit 2 |
| not ready | the step's own `Passed` record names another base than the current `init-head`, or no base: the record is stale, so the step prints why and exits 2 with no new record (lane decision w25:p1, 2026-10-10; Christian may override) | exit 2 |
| halt | `start` prints no `(ok)` run line (F3) | `BuilderRunFailed` |
| halt | `start` prints no `changeset:` line (F3) | `BuilderRunFailed` |
| halt | reconcile does not return `Reconciled` | `VerifyFailed`, `NoVerifyGate` or `StaleBase` |
| halt | the base-to-head diff touches a D17 halt path | `EvaluatorTouched` |
| halt | the verdict's first line is `VERDICT: DEFECTS` | `CriticDefects` |
| halt | another first line, over 15 lines, or a non-zero exit | `CriticUnparsed` |
| halt | a Probe exits non-zero | `ProbeFailed` |
| halt | WHERE `open_prs` (D16), `open_pr` returns Err after a Critic pass. D4 applies to the branch: pushed stays, unpushed is deleted | `PrFailed` |
| halt | a `Halted` record appears after the step started, a merge wait round included | `WaveHalted` |
| halt | WHERE `merge_on_green` (D15), checked in this order: `git log --format=%G? <base>..<head>` prints no line or a line other than `G`; `headRefOid` differs from the record head; a required check is red; every round is spent with no green | `MergeUnsigned`, `HeadMismatch`, `ChecksRed`, `ChecksTimeout` |
| halt | WHERE `merge_on_green`: `gh pr merge` exits non-zero, or the read-back prints no merge commit | `MergeFailed` |
| halt | the executor itself fails before an outcome (a missing tool on PATH, an I/O or record error). The step writes a `Halted` record so siblings and later waves stop (F17), prints the reason on stderr, and `cas wave step` exits 3. It runs no compensation: the fault is in the environment, not the work, and the operator re-runs the step once it is fixed. WHERE the state records themselves cannot be read or written, the step exits 3 with the reason only and no record: the halt writer reads the same directory (lane decisions w25:p1, 2026-10-10; Christian may override) | `ExecutorError` |

### Dispatch by role

| Role | Persona (D6) | Subprocess argv, in order | Pass when |
|---|---|---|---|
| Builder | `cas-builder` | 1. `blackwall start --task <id> --provider claude --model haiku`. 2. `blackwall settle <run> select`. 3. Under the lock, `git merge-base --is-ancestor <base> <dev>`, then `blackwall reconcile <run> --base <base> --branch blackwall/<run>`. 4. `git rev-parse blackwall/<run>`. 5. `git diff --name-only -z --no-renames <base> <head>`, split on NUL (D17). | an `(ok)` line, a changeset line, `Reconciled`, no evaluator path |
| Critic | `cas-critic` | Per Builder dependency, under the lock: 1. `git worktree add --detach <scratch>/<critic>-<builder> <head>`. 2. `git -C <tree> checkout --no-overlay <base> -- .claude`. 3. In the tree, `claude -p --strict-mcp-config --agent cas-critic --allowed-tools Read Grep Glob 'Bash(git diff *)' 'Bash(git log *)' 'Bash(git show *)' 'Bash(make *)' 'Bash(cargo *)'` (F14). The prompt goes on stdin, with no `--model`. 4. `git worktree remove --force`, on every exit path. 5. On a pass and WHERE `open_prs` (D16), `open_pr`: `blackwall pr <run> --base development --body -`. Stdin is the spec 0022 D11 fenced JSON of the Builder record's reconciled receipt. 6. WHERE `merge_on_green` and no merge is recorded yet (D15): `git log --format=%G? <base>..<head>`; then per round, outside the lock, `gh pr view <n> --json headRefOid --jq .headRefOid` and `gh pr checks <n> --required --json name,state,bucket`; on green, under the lock, the `mergeCommit` pre-read, then `gh pr merge <n> --merge --match-head-commit <head>`, then `gh pr view <n> --json mergeCommit --jq .mergeCommit.oid`. | F10 first line NO DEFECTS, 15 lines or fewer, exit 0 |
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

`cas wave flow --from <source plan> --out-dir .saga/flows` writes one flow per non-empty
wave, `cas-cpm-wave-w<i>.toml`, with `<i>` the plan's wave index, so a wave emptied by a
moved gate gets no file (D2). A literal `verify_cmd` cannot carry a wave parameter (F9, F16). Each flow holds:

- one step per `WaveStep` of the wave: impl (Builder), review (Critic) or test
  (Probe). Its `verify_cmd` is the literal argv (plus `--max-concurrency N` WHERE the plan was generated with N != 0, so the flow regenerates as written) `["cas", "wave", "step",
  "--flow", "<source>", "--step", "<id>"]`. It carries `agent = "auto"`, the
  value saga auto-classifies (F15, as retracted). The prompt tells that agent to do nothing else (D2).
- a gate step (`heimdal:approval`) for each MainSession step with a direct
  dependent in the wave. Those dependents depend on it. A MainSession step
  with no dependent stays in its own wave's flow.
- `deps` that name only steps in the same flow. `cas wave step` checks deps on
  earlier waves from their records.
- `[limits] max_in_flight`, the plan's `max_concurrency`. No gate sits between waves.

Between two runs, the wave's open PRs merge as D15 rules: the executor merges WHERE `merge_on_green`, else the lane does. Before a wave with a
dependent Builder, the integrator runs `git fetch`, moves the custody checkout
HEAD to the new development head and runs `blackwall init .`; no gate waits on CI (F18).

`cas wave step` re-plans the source (ties break on step id) and runs one step. Its record
goes to `--state-dir`, else `CAS_WAVE_STATE_DIR`, else
`${XDG_STATE_HOME:-~/.local/state}/cas/wave/<plan>/`, outside the repo, because `init`
copies the tree. All wave runs of one plan share it. The step finds the custody checkout
from `--repo`, else `CAS_WAVE_REPO`, else its cwd. Saga may run `verify_cmd` in a run
worktree with no `init-head` (F16); the step then exits 2. A `Passed` record on the current
base exits 0 and runs nothing, so a saga retry is safe. Any `Halted` record makes the step
exit 1 and run nothing. Only a person removes one.

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | `WaveExecutor` in `cas-engine` dispatches by role (table above). A Builder is a jailed `blackwall start` and `settle select`, then the landed `LandingActivity`. Amendment: `LandingActivity` splits into `reconcile` and `open_pr`, and `land` keeps calling both. `open_pr` takes the reconciled receipt and writes it as the spec 0022 D11 body. The PR opens only after the Critic passes (F10). | Caller's default for Christian's roadmap item, lane w25:p1, 2026-10-10; amendment cas-engineer, 2026-10-10 (Christian can override) |
| D2 | Saga drives one run per wave, through the generated `cas-cpm-wave-w<i>.toml`. Saga owns the run journal and the gates. `cas-engine` owns the plan, the custody calls, the halt and the compensation, because saga has neither (F17). Each step is an agent shim whose `verify_cmd` runs `cas wave step`. The shim spends one agent run per step, Probes included (F15). | Caller's default, lane w25:p1, 2026-10-10; one run per wave and the shim cost, cas-engineer, 2026-10-10 (Christian can override) The shim agent runs before `verify_cmd` in a writable worktree, so the relative plan path is agent-writable: CER-2788. |
| D3 | Process boundary: subprocesses with exact argv, as `LandingActivity` does. Every exit and output contract is `asserted` or `reported` until the P9c live run records it. | Caller's default, lane w25:p1, 2026-10-10 |
| D4 | Halt and compensate as the requirements and the Compensation section say. Blackwall runs stay for inspection. PRs and pushed branches stay; the halt report lists them. | Caller's default, lane w25:p1, 2026-10-10 |
| D5 | Evidence: a new `WorkflowEvent::WaveStep` per step, with role, persona, run, changeset, base, branch, head, PR, `merge_commit` (D15, absent unless merged) and outcome. The wave result is one `LandingReceipt` per Builder, plus the halt reason and the compensation receipts. | Caller's default, lane w25:p1, 2026-10-10 |
| D6 | Models by persona. Builder `cas-builder` runs as `--model haiku`, because `start` takes no persona (F6). Its `task.toml` must say `model = "haiku"`, because that file beats the flag (F12). Critic `cas-critic` runs through `--agent` with no `--model`, so its opus line holds (F7). The executor does not dispatch `cas-foreman`. Probe and MainSession use no model. D6 supersedes 0024's `ModelTier::Pro` for dispatch. | Christian's model rule, 2026-10-10; mapping lane w25:p1, 2026-10-10 |
| D7 | Base per wave: `.blackwall/init-head`, with HEAD equal to it (F5). A Builder waits until every upstream Builder in its deps' closure has merged. Merge commits make each reconciled head an ancestor of the next base. The wait falls between saga runs, so no gate waits on CI (F18). No `--parent` stacking. | cas-engineer, 2026-10-10 (Christian can override) |
| D8 | One host-wide lock file (`File::lock`, F11) serializes reconcile, critic runs and Probes. So at most one `make check JOBS=2` runs at a time (rule 2, spec 0020). `start` runs may overlap up to `max_concurrency`. | cas-engineer, 2026-10-10 |
| D9 | A wave Builder has no CAS slot (F2). Its receipt sets `slot_id = wave-<step id>`, `audit = NotAudited` (a new `AuditStatus` variant), `run_hash` empty and `cpu_cap = "uncapped"`. This extends 0022 D4 to runs with no audit at all. **Ruled: `NotAudited` lands, flagged.** The receipt and the PR body say so, and the planner warns. A `NotAudited` wave PR may be merged by the executor's merge-on-green with the flag in its body (D19). | Christian, 2026-10-10, in session (lane w25:p1 interview); lane default cas-engineer |
| D10 | Plan admission checks the admission rows before any subprocess runs. One Critic per Builder means one PR opener per Builder. | cas-engineer, 2026-10-10 |
| D11 | The blackwall task name is the step id: `tasks/<step id>/`. | cas-engineer, 2026-10-10 |
| D12 | Receipts carry the 12-character run and changeset prefixes that `start` prints (F3). The other subcommands resolve prefixes (F4); `show` does not (Notes). [verified, P9c runs 0dbb7f02bf2d, 20261010-141346-cas-cpm-wave-w0 and 20261010-141552-cas-cpm-wave-w1] `settle` and `reconcile` took the 12-character run id, the records carry run 47c34f0dd060 and changeset 4abbf3bebe21, reconcile reached head 42560966 on `blackwall/47c34f0dd060`, and no PR opened (D16). Full hashes wait for `start --json` (CER-2787). | cas-engineer, 2026-10-10 |
| D13 | The critic reviews in a transient detached worktree at the reconciled head. It runs the gates (F10) and leaves HEAD in the custody checkout alone (F5). | cas-engineer, 2026-10-10 |
| D14 | First live run: the local `blackwall` CLI on ceres, or blackwall-serve in the cluster through cortex-worker, reflexd and escapement (F13). Lane proposal: local, because the 1.2.0 `serve` only inspects (F6). **Ruled: the local CLI on ceres.** P9c ran on it (runs 20261010-141346-cas-cpm-wave-w0 and 20261010-141552-cas-cpm-wave-w1); a blackwall-serve run needs its own brief when `serve` executes. | Christian, 2026-10-10, in session (lane w25:p1 interview); proposal lane w25:p1 |
| D15 | Merging a wave's PRs between runs: may the lane merge on green, or does a person always merge? Lane proposal: the lane merges each PR on green CI with `gh pr merge --merge --match-head-commit`, outside the executor. **Ruled: the executor may merge on green**, behind a default-off flag, in a later package with its own brief: CI green on the exact head, `--match-head-commit`, merge commit, signed commits; a `NotAudited` PR merges with the flag in its body (D19). Until that package (P12a, P12b) ships, the lane lands each PR by hand and the executor never merges (what shipped in P9a). **Mechanism:** `WaveExecConfig.merge_on_green`, false by default; admission refuses it without `open_prs`. P12b sets it only when the source plan's `[flow] merge_on_green = true` and `CAS_WAVE_MERGE=1` are both set. It acts after a Critic pass with an open PR, keyed on "no merge recorded yet", not on "PR just opened", so a retried Critic still merges. Order: (1) `git log --format=%G? <base>..<head>` in the custody checkout must print at least one line, each `G`; (2) rounds: `max(1, ceil(merge_wait / max(1 s, merge_poll)))`, outside the host lock (one wait would block every build), a `thread::sleep(merge_poll)` between rounds and none after the last. Each round checks for a `Halted` record, then compares `headRefOid` with the record head (full SHA, exact), then parses the checks JSON on any exit code. Green: a non-empty array, every `state` `SUCCESS`. Pending: empty output or `[]` (checks not yet reported; never green), or no red entry and at least one `bucket` `pending`. Red: anything else, `SKIPPED`, `NEUTRAL`, `CANCELLED` and output that does not parse included. A `gh pr view` that exits non-zero counts as a pending round. (4) Green: one lock hold for the halt check, the `mergeCommit` pre-read (a set value records the merge with no `gh pr merge` call), `gh pr merge`, the read-back (re-read once if empty) and the record save; the merge commit goes to the receipt (`LandingFacts.merge_commit`, spec 0022), the Builder record and the Critic's `WaveStep` event. Every merge halt compensates (D4: a record with a PR keeps everything). Defaults: `merge_wait` 900 s, about 3.6 times the 247 s max in F21; `merge_poll` 30 s; the plan field is `[flow] merge_wait_secs`. No `--delete-branch` and no `--auto`: post-merge hygiene (fetch, custody HEAD, `blackwall init .`, branch removal) stays with the lane. | Christian, 2026-10-10, in session (lane w25:p1 interview): the ruling; proposal lane w25:p1. Mechanism, order, defaults, halt names and the signature check (protection requires none, F21): lane decisions w25:p1, 2026-10-10; Christian can override |
| D16 | `open_prs` is off by default, so a wave run stops after the critic verdict and opens no PR. `CAS_WAVE_LIVE_PR=1` sets it for the live smoke. `CAS_WAVE_OPEN_PRS=1` sets it for `cas wave step`, because a literal `verify_cmd` carries no flag. The integrator then lands or closes each PR. | cas-engineer, 2026-10-10 |
| D17 | Before a critic, the executor runs `git diff --name-only -z --no-renames <base> <head>` and splits the output on NUL. Without `-z`, git C-quotes a non-ASCII path, so no pattern matches it. Without `--no-renames`, it lists only a rename's destination, so a renamed-away `Makefile` is not seen [verified, 2026-10-10, git 2.43 scratch repo]. The halt set: a path under `.claude/`; `.mcp.json`; `**/CLAUDE.md`, `**/CLAUDE.local.md` and `**/AGENTS.md`; and the spec 0023 D4 and D6 lists. A `**/` pattern matches at the root too. The rule files are in the set because `claude -p` loads the root `CLAUDE.md` of its cwd. That file names `AGENTS.md` as the only rule source. It also loads a nested `CLAUDE.md` when the critic reads that directory [reported, critic-0025-3]. So a Builder could rewrite its own rubric. `claude -p` starts a project `.mcp.json` server from its cwd with no approval [reported, critic-0025-4]. The Critic argv sets `--strict-mcp-config`, which ignores all other MCP configurations [verified, 2026-10-10, `claude --help`, 2.1.296]. Spec 0023 D6 lists every `Cargo.toml` and `build.rs`, and `crates/cas-ebpf/ebpf/**`, fail closed. `build. `crates/cas-ebpf/ebpf/**` is in the set because `build.rs` trusts the object and its `.sha256` as a pair; the reason and its evidence are in spec 0023 D6. Rebuilding that object is kernel work, never a wave Builder step. So a Builder that touches one of these paths halts the wave, and the executor compensates its siblings' passed records. A dependency add is not wave Builder work, because rule 12 needs a spec 0004 pin first. A match sets `evaluator_touched` and halts the step. The executor rebuilds the receipt with `LandingReceipt::new`, so the tier is `reported`. The critic never loads a persona, settings, rule file, MCP server or gate from the changeset. The branch stays; a person may open its PR at `reported` (0023 D3). | cas-engineer, 2026-10-10; halt set widened per critic-0025-3 and critic-0025-4 (`.mcp.json`, `--strict-mcp-config`, 0023 D6), lane w25:p1, 2026-10-10; `crates/cas-ebpf/ebpf/**` via 0023 D6, lane decision w25:p1, 2026-10-10; narrows the wave path of Christian's 0023 D3 (Christian can override) |
| D18 | A Probe runs at the wave base in a transient worktree under the lock. It checks the base, not a Builder's output; the Critic checks that. | cas-engineer, 2026-10-10 |
| D19 | Every wave receipt is `NotAudited` (F21). **Ruled: the flag merges a `NotAudited` wave PR, flagged in the body.** The PR already carries a Critic pass and green CI; the exclusion of D9 and D15 is withdrawn, so there is no `MergeNotAudited` halt. | Christian, 2026-10-10, in session (lane w25:p1 interview); raised by lane w25:p1 |

## Plan

- [ ] P9a, `cas-engine` (plus `cas-domain`), `cas-builder` haiku. It adds `AuditStatus::NotAudited`,
  and `EVALUATOR_PATHS` with `is_evaluator_path` (the spec 0023 plan item). It splits
  `LandingActivity` into `reconcile` and `open_pr`, with `land` unchanged. `open_pr` reuses
  P5c's `--body -` code. `WaveExecutor` gets admission, dispatch, the D17 diff, the verdict
  parse, the host lock, `WorkflowEvent::WaveStep` and `WaveCompensator`. Tests use fake
  `blackwall`, `git` and `claude` scripts, red first. It starts after P5b and P5c (spec 0022
  D7, D9 to D11) land: all three edit `landing.rs`.
- [ ] P9b, `cas-cli`, `cas-builder` haiku. It adds `cas wave flow` and `cas wave step` with the state directory, and the source plan
  `.saga/plans/cas-wave-smoke.toml`. It generates `.saga/flows/cas-cpm-wave-w*.toml`
  and adds `tasks/wave-smoke-build/`. Tests: a round-trip and a drift test.
- [x] P9c, integrator on ceres, once D14 is ruled: `blackwall doctor`,
  `blackwall init .` in a clean checkout at `origin/development`, the live smoke,
  then the saga runs `cas-cpm-wave-w0` and `cas-cpm-wave-w1`. Record every
  output and exit contract, then amend D12 and F3 to F5. Done 2026-10-10 at 642c3f2: smoke run 7878209325ad, saga runs 20261010-141346-cas-cpm-wave-w0 and 20261010-141552-cas-cpm-wave-w1. Not shown: the F5 stale root, and whether `init` keeps earlier runs.
- [x] Christian ruled D9, D14 and D15 on 2026-10-10. D15 needs a package (merge flag). D19 ruled 2026-10-10: a `NotAudited` wave PR merges, flagged.
- [ ] P12a, `cas-engine` plus the one `cas-domain` field, `cas-builder` haiku: D15's config, admission row, rounds, merge, halts and `merge_commit`; a fake `gh`; red first.
- [ ] P12b, `cas-cli`, not briefed yet: `[flow] merge_on_green` and `merge_wait_secs` in the source plan, `CAS_WAVE_MERGE=1`, refused without `CAS_WAVE_OPEN_PRS=1`.

## Test

```bash
make spec-check
cargo nextest run -p cas-domain --build-jobs 2 landing_receipt
cargo nextest run -p cas-domain --build-jobs 2 evaluator_path
cargo nextest run -p cas-engine --build-jobs 2 landing
cargo nextest run -p cas-engine --build-jobs 2 --test wave_exec
cargo nextest run -p cas-cli --build-jobs 2 --test wave_flow
```

A pass means every test above is green with the fakes. The live smoke (P9c)
runs on ceres only, after `blackwall init .`, with `L` the clean checkout:

```bash
CAS_WAVE_LIVE=1 CAS_WAVE_REPO=$L CAS_WAVE_PLAN=$L/.saga/plans/cas-wave-smoke.toml \
  cargo nextest run -p cas-engine --build-jobs 2 --test wave_exec --run-ignored only live_smoke
```

## Notes

- Rejected: stacking a dependent Builder on its upstream world with
  `start --parent`. Its whole-file blob would carry the upstream edits onto a
  base without them: the revert trap in the custody rebasing SOP.
- Rejected: opening the PR in the Builder step, as `land` does. It would precede the critic.
- Rejected: one saga run with a gate between waves. That gate would wait on CI
  and the merge, past the 600 s window (F18).
- P9c settled neither unknown. `blackwall show 7878209325ad` returned `NotFound`, though `list` prints that 12-character id [verified, run 7878209325ad], so whether `show` prints the base (custody-run skill against F4) is still open. `init` ran once, before any run, so whether it keeps earlier runs (D4) is still open.
- Under `cargo nextest` the smoke's reconcile halted `VerifyFailed`, yet the smoke exited 0 because it passes on any outcome, while the same run reconciled outside nextest [verified, runs 7878209325ad and 0dbb7f02bf2d]; the cause is not proven [heuristic: the inherited nextest environment], and the receipt keeps no reconcile stderr (CER-2789).
- Amends spec 0022 D1 and D9 (its D12 logs it), and adds `LandingFacts.merge_commit` for D15 (its amendment log). P9a builds 0023's `EVALUATOR_PATHS`.
- Out of scope: the `?` at `wave.rs:421` and `compensator.rs:339` in
  `crates/cas-engine/src` [verified at b149d73] stops compensation at the first
  `Err`, against spec 0009 (F19). Ticket CER-2783 is the fix package, not P9a.
- Open with the saga lane: a command-only step kind (D2 cost), the `verify_cmd`
  cwd, when 35ee74a ships, and whether saga bounds a `verify_cmd` run time (D15 waits up to 900 s). Unverified [asserted]: what `gh pr checks --json` prints before any check registers (D15 reads empty or `[]` as pending). Open with the blackwall owners: overlapping
  `start` runs on one store, and a full-hash or JSON `start` output.
