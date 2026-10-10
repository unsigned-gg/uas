---
status: planned
created: 2026-10-10
tags: [engine, wave, custody, blackwall, saga, cpm]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0008-soma-cortex-saga-reverie-integration, 0009-saga-compensation-runner-and-durability, 0020-build-job-cap, 0022-custody-landing, 0024-estate-saga-cpm-and-agent-sprint-wave-strategy]
---

# 0025-wave-execution-on-the-custody-chain

## Overview

Roadmap item from the operator, 2026-10-10, lane w25:p1: "implement cpm and agent
waves with deltadb/blackwall and saga". Spec 0024 (landed in b409386) plans
waves: `WavePlan`, `WaveCoordinator`, the roles Probe, Builder, Critic and
MainSession, and `cas flow --wave`. It executes nothing on the custody chain.
This spec makes each wave step run on that chain. A Builder is a jailed
blackwall run. A Critic is a `cas-critic` run. A Probe is a command. A
MainSession step is a saga gate. Saga drives the run; `cas-engine` owns the
plan and the custody calls.

Facts, 2026-10-10. Source facts from `unsigned-gg/blackwall@3d98dc1` are
`reported`: that checkout is dirty (`run.rs` has 6 local lines).

- **F1 [verified, read `crates/cas-engine/src/wave.rs`]:** `validate_verdict`
  checks only the 15-line cap; nothing parses a verdict. `classify_step` gives
  Builder and Critic `ModelTier::Pro`. `cas flow --wave` runs each step's
  `verify_cmd` in-process (`crates/cas-cli/src/main.rs:568-640`).
- **F2 [verified, read `crates/cas-engine/src/landing.rs`]:** `LandingActivity::land`
  runs the base check, `reconcile` and `pr` in one call and never passes
  `--draft`. `LandingInput` needs a slot id, a spec 0014 `run_hash`, an
  `AuditStatus` and a `cpu_cap`. A wave run has no CAS slot, so none of these
  has an honest value today.
- **F3 [reported, `crates/blackwall-cli/src/run.rs:815-825,884,898`]:** `start`
  prints on stderr `run <model> (ok|err): hash <12 chars> (new|dedup)`, then
  `changeset: <12 chars> (<n> entries)` and `world commit: <12 chars>`.
  `run.rs` has no `println!`, so nothing goes to stdout. Each line starts with
  a check mark (U+2713) and a space, so a parse must match substrings, not `^run`.
- **F4 [reported, `reconcile.rs:151`, `pr.rs:149`, `settle.rs:60`,
  `report.rs:437`]:** `settle`, `reconcile`, `pr` and `changeset` resolve a hash
  prefix. `show` does not (`report.rs:420-425`); it prints the `RunRecord` JSON,
  whose fields are `prompt`, `model`, `stdout`, `stderr`, `exit_code`, `usage`
  and `failure` (`crates/blackwall-store/src/record.rs:210`). It has no base and
  no changeset hash.
- **F5 [reported, `run.rs:309-321`]:** `init` writes the git HEAD to
  `.blackwall/init-head`. A `start` whose HEAD differs only warns. `reconcile`
  refuses a stale root unless given `--force` [verified, `blackwall reconcile --help`].
- **F6 [verified, `blackwall <cmd> --help` in an empty dir]:** `start` takes
  `--task`, `--provider`, `--model` and `--parent`; it has no persona flag.
  `show`, `list` and `changeset` have no JSON flag. `serve` is "read-only
  custody inspection over the object store".
- **F7 [verified, `claude --help`]:** `claude -p` and `--agent <agent>` exist.
  [reported, `.claude/agents/README.md`] `--model` overrides a persona's model.
- **F8 [verified, `saga run --help`, `saga gate approve --help`, `saga flows`]:**
  `saga run <flow> --param k=v --dry-run`; `saga gate approve <run> <task>`.
  `saga flows` in this repo lists `.saga/flows/*.toml`. A dry-run of
  `estate-custody-rollout` printed a dagr `run.json` with `inputs` equal to
  `deps` and no `verify_cmd`.
- **F9 [reported, lane-package flow header, architect w10:pC]:** the shipped
  step kinds are impl, test, review, gate and note. Saga copies `verify_cmd` and
  `cwd` raw, so `{{params}}` is not substituted there. [reported,
  `vendor/saga/contracts/flow.rs`, saga@1e4be86] a step has `agent`
  (`Named`, `Auto` or `Unassigned`), `cwd` and an optional `persona`.
- **F10 [verified, read `.claude/agents/cas-critic.md`]:** the critic's first
  line is `VERDICT: DEFECTS` or `VERDICT: NO DEFECTS`. It says NO DEFECTS only
  after it ran the done-when command and the gates itself.
- **F11 [verified, probe on rustc 1.94.1]:** `std::fs::File::lock` works on the
  pinned toolchain. A host lock needs no new dependency.
- **F12 [verified, read `blackwall.toml` and `tasks/*/task.toml`]:** both set
  model `sonnet`. the operator's rule of 2026-10-10 sets builders to haiku
  [reported, `.claude/agents/README.md`]. [reported, `run.rs:489-503`] a
  `task.toml` `model` overwrites the CLI `--model` with no condition, although
  the comment there says the CLI wins.
- **F13 [reported, DeltaDB design surface, section 3.1, draft, w19:p1]:** the
  in-cluster loop is cortex-worker, reflexd, escapement, blackwall-serve.
  `fork_from_delta` (DeltaDB parity) is target, not built (section 2).
- **F14 [verified, read `.claude/settings.json`]:** the project allows
  `Bash(cargo *)`, `Bash(make *)`, `Bash(git fetch *)` and `Bash(git rebase *)`.
  A `claude -p` critic cannot answer a permission prompt, so `git diff` needs
  an explicit `--allowed-tools` [asserted until P9c].

## Design

### Requirements (EARS)

- **Ubiquitous:** The `WaveExecutor` shall dispatch each `WaveStep` by its
  `AgentRole`, as the dispatch table below gives.
- **Ubiquitous:** The executor shall run `blackwall`, `git`, `claude` and
  `saga` as subprocesses with fixed argv in the repo directory. It shall not
  link their crates.
- **Ubiquitous:** Every step shall emit one `WorkflowEvent::WaveStep` with the
  role, persona, blackwall run, changeset, base, branch, reconciled head, PR
  number and outcome. A fact the step did not produce is `None`.
- **Ubiquitous:** The executor shall hold one host lock around each call that
  runs a build: `blackwall reconcile` and the critic run.
- **Event-driven:** WHEN a wave starts, the executor shall read the wave base
  from `<repo>/.blackwall/init-head` and require `git rev-parse HEAD` to equal it.
- **Event-driven:** WHEN a Builder run settles, the executor shall reconcile it
  with `--base <wave base>`. It shall open no PR at this point.
- **Event-driven:** WHEN a Critic's first line is `VERDICT: NO DEFECTS`, its
  verdict has 15 lines or fewer and it exits 0, the executor shall open the PR
  of each Builder that the Critic depends on, and record one `LandingReceipt`
  per Builder.
- **State-driven:** WHILE a MainSession step is not approved through saga, the
  executor shall start no step of a later wave. It shall never approve a gate.
- **Unwanted behavior:** IF reconcile exits non-zero, the base is stale, the
  verify gate is missing, a verdict is DEFECTS or unparsed, or a Probe exits
  non-zero, THEN the executor shall halt the wave, start no further step, and
  run the spec 0024 reverse-order compensation. A gate timeout halts the run in
  saga, which owns the gates (D2).
- **Unwanted behavior:** IF compensation runs, THEN it shall not discard,
  release or delete a blackwall run, close a PR, or delete a pushed branch. It
  shall delete only an unpushed local `blackwall/<run>` branch.
- **Unwanted behavior:** IF a Builder has no Critic that directly depends on
  it, a Builder has no `tasks/<step id>/prompt.md`, its `task.toml` names a
  model other than the builder model (F12), a Probe has no `verify_cmd`, or a
  step id is not a valid `SlotId` after the `wave-` prefix, THEN the executor
  shall refuse the plan before its first subprocess call.
- **Unwanted behavior:** IF any Builder in the transitive closure of a
  Builder's deps has a reconciled head that is not an ancestor of the wave
  base, THEN the executor shall not start it. That upstream PR has not merged.
- **Unwanted behavior:** IF `start` prints no `run ... (ok)` line or no
  `changeset:` line (F3), THEN the Builder step shall fail.
- **Optional feature:** WHERE `CAS_WAVE_LIVE=1` is set, the live smoke shall run
  the real tools, and shall fail, not skip, when a tool is missing.
- **Optional feature:** WHERE saga runs a step with no agent (`Unassigned`), the
  flow generator shall emit that shape instead of the agent shim (D2).

### Dispatch by role

| Role | Persona (D6) | Subprocess argv, in order | Pass when |
|---|---|---|---|
| Builder | `cas-builder` | `blackwall start --task <id> --provider claude --model haiku`; `blackwall settle <run> select`; under the lock, `git merge-base --is-ancestor <base> <dev>` then `blackwall reconcile <run> --base <base> --branch blackwall/<run>`; `git rev-parse blackwall/<run>` | `ok` run line, a changeset line, reconcile exit 0 |
| Critic | `cas-critic` | per Builder dependency: `git worktree add --detach <scratch>/<critic>-<builder> <reconciled head>`; under the lock, in that tree, `claude -p --agent cas-critic --allowed-tools Read Grep Glob 'Bash(git diff *)' 'Bash(git log *)' 'Bash(git show *)' 'Bash(make *)' 'Bash(cargo *)' <prompt>`, no `--model` (F14); `git worktree remove --force` on every exit path; on pass, `blackwall pr <run> --base development` | F10 first line NO DEFECTS, 15 lines or fewer, exit 0 |
| Probe | none | the step's `verify_cmd` in the repo directory | exit 0 |
| MainSession | none | none: the saga gate step blocks until `saga gate approve` | saga approves; the executor never does |

`<run>` is the 12-character prefix from the `start` line (D12). The critic
prompt names the base, the branch, the reconciled head, the reconcile receipt
and the Builder's brief `tasks/<builder id>/prompt.md`. `--force` on
`worktree remove` drops only the transient critic tree.

### The saga flow

`cas wave flow --from <source flow> --out .saga/flows/cas-cpm-wave.toml` plans
the source with `WavePlan::plan` and writes one saga step per `WaveStep`:

- id `w<i>-<step id>`; kind impl (Builder), review (Critic), test (Probe) or
  gate (MainSession); deps mapped to the `w<j>-` ids.
- `verify_cmd = ["cas", "wave", "step", "--flow", "<source>", "--step", "<id>"]`,
  literal argv (F9). `agent = "auto"` with a prompt that tells the agent to do
  nothing else (the shim, D2).
- one gate step `w<i>-gate` (`heimdal:approval`) after every wave but the
  last. Wave `i+1` steps depend on it. `[limits] max_in_flight` is the plan's
  `max_concurrency`.

`cas wave step` re-plans the source flow (the plan is deterministic: ties break
on step id), runs one step, and writes its record to
`${XDG_STATE_HOME:-<home>/.local/state}/cas/wave/<flow>/<step>.json`, outside the
repo (`init` copies the tree, F5). It reads its dependencies' records from the
same directory, for every step in the transitive closure of its deps. A
`Passed` record whose `base_sha` equals the current wave base is returned and
nothing runs, so a saga retry is safe. A `Halted` record is returned with exit
1 and nothing runs, so a retry after compensation starts no new run.

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | `WaveExecutor` in `cas-engine` dispatches by role (table above). Builder = jailed `blackwall start`, `settle select`, then the landed `LandingActivity` for reconcile and pr. Amendment: `LandingActivity` splits into `reconcile` and `open_pr` (`land` keeps calling both), and the PR opens only after the Critic passes, so no PR is ready before its critic (F10, agents README). | Caller's default for the operator's roadmap item, lane w25:p1, 2026-10-10; amendment cas-engineer, 2026-10-10 (the operator can override) |
| D2 | Saga drives the run through the generated `.saga/flows/cas-cpm-wave.toml`: one step per wave step, a gate between waves. Saga owns the run journal and the gates; `cas-engine` owns the plan and the custody calls. Shipped saga has no command-only step (F9), so each step is an agent shim whose `verify_cmd` runs `cas wave step`. | Caller's default, lane w25:p1, 2026-10-10 |
| D3 | Process boundary: subprocesses with exact argv, as `LandingActivity` does. Every exit and output contract is `asserted` or `reported` until the P9c live run records it. | Caller's default, lane w25:p1, 2026-10-10 |
| D4 | Halt and compensate as in the requirements. Blackwall runs stay for inspection. PRs stay open and pushed branches stay; the halt report lists them for the operator. | Caller's default, lane w25:p1, 2026-10-10 |
| D5 | Evidence: a new `WorkflowEvent::WaveStep` variant per step. The wave result is the list of `LandingReceipt`s, one per Builder, plus the halt reason. | Caller's default, lane w25:p1, 2026-10-10 |
| D6 | Models by persona: Builder `cas-builder`, run as `--model haiku` because `start` takes no persona (F6); its `task.toml` must also say haiku, because that file beats the flag (F12). The persona name is recorded; its body does not reach the jail. Critic `cas-critic` through `--agent`, no `--model`, so its opus line holds (F7). Foreman `cas-foreman` (sonnet) leads only a pane-gang run; the executor does not dispatch it. Probe and MainSession use no model. | the operator's model rule, 2026-10-10; mapping lane w25:p1, 2026-10-10 |
| D7 | Base per wave: `.blackwall/init-head`, with HEAD equal to it (F5). A Builder waits until every Builder in the transitive closure of its deps has a reconciled head that is an ancestor of the base, which merge commits make true after each PR merges. No `--parent` stacking. | cas-engineer, 2026-10-10 (the operator can override) |
| D8 | One host-wide lock file (`File::lock`, F11) serializes reconcile and critic runs, so the executor runs at most one `make check JOBS=2` at a time (rule 2, spec 0020). `start` runs may overlap up to `max_concurrency`. | cas-engineer, 2026-10-10 |
| D9 | A wave Builder has no CAS slot (F2). Its receipt sets `slot_id = wave-<step id>`, `audit = NotAudited` (a new `AuditStatus` variant), `run_hash` empty, `cpu_cap = "uncapped"` (a spec 0020 value). Landing proceeds, as spec 0022 D4 does for an unattested audit. | cas-engineer, 2026-10-10, extends the operator's 0022 D4 (the operator can override) |
| D10 | Plan admission refuses the cases in the requirements before any subprocess runs. | cas-engineer, 2026-10-10 |
| D11 | The blackwall task name is the step id: `tasks/<step id>/`. | cas-engineer, 2026-10-10 |
| D12 | Receipts carry the 12-character run and changeset prefixes that `start` prints (F3); the other subcommands resolve prefixes (F4). Full hashes wait for a source that P9c finds. | cas-engineer, 2026-10-10 |
| D13 | The critic reviews in a transient detached worktree at the reconciled head, so it can run the gates (F10) without moving HEAD in the custody checkout (F5). | cas-engineer, 2026-10-10 |
| D14 | First live run: the local `blackwall` CLI on ceres, or blackwall-serve in the cluster through cortex-worker, reflexd and escapement (F13). Lane proposal: local, because the 1.2.0 `serve` only inspects (F6). | **the operator to rule** |
| D15 | Merging a wave's PRs at the gate between waves: may the executor (or the lane through it) merge on green, or does a person always merge? Until ruled, the executor never merges (spec 0022, "What stays with people"). | **the operator to rule** |
| D16 | The live smoke stops after the critic verdict. `CAS_WAVE_LIVE_PR=1` adds `blackwall pr`, and the integrator then lands or closes that PR. | cas-engineer, 2026-10-10 |

## Plan

- [ ] P9a, `cas-engine` (plus one `cas-domain` variant), `cas-builder` haiku:
  `AuditStatus::NotAudited`; `LandingActivity::{reconcile, open_pr}` with
  `land` unchanged; `WaveExecutor` with plan admission, dispatch, verdict
  parse, host lock, `WorkflowEvent::WaveStep`, and a compensator that never
  touches runs, PRs or pushed branches; tests with fake `blackwall`, `git` and
  `claude` scripts on `PATH`, red first; the ignored live smoke. Starts after
  P5b (spec 0022 D9, D10) lands: both edit the two `landing.rs` files.
- [ ] P9b, `cas-cli`, `cas-builder` haiku: `cas wave flow` and `cas wave step`
  with the state directory; `.saga/plans/cas-wave-smoke.toml` (source);
  `.saga/flows/cas-cpm-wave.toml` (generated); `tasks/wave-smoke-build/`; a
  round-trip test and a drift test against the checked-in flow.
- [ ] P9c, integrator on ceres: `blackwall doctor`, `blackwall init .` in a
  clean checkout at `origin/development`, the live smoke, then
  `saga run cas-cpm-wave --dry-run` and one real run to the first gate. Record
  every stdout and exit contract here, and amend D12 and F3 to F5.
- [ ] the operator rules D14 and D15.

## Test

```bash
make spec-check
cargo nextest run -p cas-domain --build-jobs 2 landing_receipt
cargo nextest run -p cas-engine --build-jobs 2 landing
cargo nextest run -p cas-engine --build-jobs 2 --test wave_exec
cargo nextest run -p cas-cli --build-jobs 2 --test wave_flow
```

A pass means every test above is green with the fakes. The live smoke (P9c)
runs on ceres only, after `blackwall init .`:

```bash
CAS_WAVE_LIVE=1 cargo nextest run -p cas-engine --build-jobs 2 --test wave_exec --run-ignored only live_smoke
```

## Notes

- Rejected: stacking a dependent Builder on its upstream world with
  `start --parent`. Its whole-file blob would carry the upstream edits onto a
  base without them: the revert trap in the custody rebasing SOP. Waiting for
  the merge costs one gate per dependency level.
- Rejected: opening the PR inside the Builder step, as `land` does. The PR would
  be ready before its critic ran.
- Spec 0024 gives Builder `ModelTier::Pro`. D6 supersedes that for dispatch;
  the tier enum stays as planning data.
- The repo's custody-run skill says `blackwall show` prints the base commit.
  The 3d98dc1 `RunRecord` has no base field (F4). P9c settles it.
- The wave gate prompt: when the next wave holds a dependent Builder (D7),
  merge this wave's PRs, `git fetch`, move the custody checkout HEAD to the new
  development head, run `blackwall init .`, then approve. Whether `init` keeps
  the earlier runs is unknown; D4 depends on it, and P9c checks `blackwall list`.
- Amends spec 0022: `AuditStatus::NotAudited` (D9) and the
  `reconcile`/`open_pr` split (D1). Spec 0022 Notes log the amendment.
- Open with the saga lane: a command-only step (`agent = Unassigned`), how
  `inputs` pass between steps, the gate timeout (a lane-package gate window of
  about 10 minutes was seen on 2026-10-10 [reported, reverie session context]),
  and whether saga stops sibling steps when one fails.
- Open with the blackwall owners: whether two `start` runs on one store can
  overlap, and a full-hash or JSON output for `start`.
