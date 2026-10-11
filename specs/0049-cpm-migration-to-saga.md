---
status: planned
created: 2026-10-10
tags: [saga, cpm, wave, migration, custody]
priority: high
depends_on: [0008-soma-cortex-saga-reverie-integration, 0022-custody-landing, 0024-estate-saga-cpm-and-agent-sprint-wave-strategy, 0025-wave-execution-on-the-custody-chain, 0046-saga-team-and-topology-files]
---

# 0049-cpm-migration-to-saga

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

Christian ruled on 2026-10-10 to "migrate all CPM and team-checking from uas to saga" (Notes).
Spec 0046 took the team-checking half; saga spec 0039 now implements it (F13). This spec takes
the CPM half. The critical-path and wave planning of spec 0024 and the wave execution of spec
0025 move to saga. uas keeps what a slot runtime needs: slot leases and the custody landing
primitive of spec 0022. The spec 0025 banner already says wave execution is a saga domain.

The uas half is ruled here: a deprecation window, a freeze, and one removal PR. The saga half
is proposed for the saga lane, which owns `cerebral-work/saga`. Saga has no CPM, no waves, no
halt and no compensation today (F10, F11), so the saga half is a build, not a port.

### Facts

Measured 2026-10-10 on ceres. uas: `origin/development` 19c19ea (21:06 -0400), worktree
`/tmp/uas-w49`, `uas` built there with `cargo build -p uas-cli -j 2`. Saga: `origin/development`
2f9b23a (21:05 -0400) of `/home/ctodie/projects/platform/runtime/saga`, read with
`git show origin/development:<path>` or `git grep ... origin/development`, because that
checkout's worktree is on `feat/yggdrasil-skald-trace` (ef0919e).

- **F1 [verified]:** `uas wave` has three subcommands: `flow`, `step`, `merge-gate`
  (`grep -n 'WaveCommand' crates/uas-cli/src/main.rs`: lines 67, 1117 to 1160).
  `uas wave --help` exits 0, and `grep -ci deprecated` on it counts 0.
- **F2 [verified]:** the CLI reads `UAS_WAVE_REPO`, `UAS_WAVE_STATE_DIR` and `UAS_WAVE_OPEN_PRS`
  (`crates/uas-cli/src/wave_flow.rs` 378 to 382). The engine tests also read `UAS_WAVE_LIVE`,
  `UAS_WAVE_LIVE_PR` and `UAS_WAVE_PLAN`. Command: `grep -rnoE 'UAS_WAVE_[A-Z_]+' crates`.
- **F3 [verified]:** PR #132 merged at 2026-10-10T23:58:31Z as 9503684, an ancestor of 19c19ea.
  It added `--open-prs` to `uas wave step` (`gh pr view 132 --json mergedAt,mergeCommit,files`).
  The spec 0025 hold landed earlier, in b1e9cdf at 16:49Z (`git log -S 'HELD 2026-10-10'`).
  Since then two spec 0042 `feat` commits changed the wave CLI: c61c812 (`main.rs`,
  `wave_flow.rs`) and 799e05b (`main.rs` only) (`git show --stat`).
- **F4 [verified]:** CPM reaches past `uas wave`. `uas harness run` has `--show-cpm`, `--wave`
  and `--max-concurrency` (main.rs 168 to 179). It runs waves in-process through
  `WaveCoordinator` (main.rs 642 to 700). `HarnessReport` carries `cpm_schedule` and
  `wave_plan` (main.rs 225). `SagaFlowCoordinator` has `cpm_schedule` and `plan_waves`
  (`crates/uas-engine/src/saga.rs` 225 to 231). `grep -rlnE
  'WavePlan|WaveCoordinator|CpmSchedule|WaveExecutor|wave_exec|wave_flow' crates xtask
  --include=*.rs` lists 10 files, all in `uas-cli` and `uas-engine`.
- **F5 [verified]:** `uas-engine` holds `WaveExecutor`, its records and `WaveCompensator` in
  `src/wave_exec.rs` (1528 lines); `WaveStep`, `WavePlan`, `WaveCoordinator`, `classify_step`
  and `validate_verdict` in `src/wave.rs` (610); `CpmSchedule` in `src/cpm.rs` (365); and
  `WorkflowEvent::WaveStep` in `src/event.rs` line 65. Commands: `wc -l`;
  `grep -rn 'WaveStep\|wave_exec' crates/uas-engine/src`.
- **F6 [verified]:** `grep -cE '#\[(tokio::)?test'` counts 127 wave and CPM tests: uas-engine
  `tests/wave_exec.rs` 70 (one is the ignored live smoke), `tests/cpm_wave.rs` 7, `src/wave.rs`
  6, `src/cpm.rs` 4, `src/wave_exec.rs` 0; uas-cli `tests/wave_flow.rs` 37, `src/wave_flow.rs`
  1, and 2 in `src/main.rs`. `tests/landing.rs` has 15. No fixture files exist: the tests write
  fake `blackwall`, `git`, `claude` and `make` scripts at run time (`tests/wave_exec.rs` 146 to 195).
- **F7 [verified]:** `ls .saga/flows` lists `estate-custody-rollout`, `gates`, `uas-builder` and
  the generated `uas-cpm-wave-w0` and `uas-cpm-wave-w1`. `ls .saga/plans` lists
  `uas-wave-smoke.toml`. Each generated step runs `uas wave step` as its `verify_cmd`, with
  `agent = "auto"`. `uas wave flow --from .saga/plans/uas-wave-smoke.toml --out-dir .saga/flows`
  leaves `git status --porcelain .saga` empty. Wave 0 holds `wave-smoke-build` and
  `wave-smoke-spec`; wave 1 holds `wave-smoke-critic`.
- **F8 [verified]:** `LandingActivity` has `land`, `reconcile` and `open_pr`
  (`crates/uas-engine/src/landing.rs` 182, 202, 268). Its only callers are `wave_exec.rs` line
  1253 and `tests/landing.rs` (`grep -rn LandingActivity crates | grep -v src/landing.rs`).
  `uas-domain` holds `AuditStatus::NotAudited` and `merge_commit` (`src/landing.rs` 25, 77).
  `EVALUATOR_PATHS` and `CRITIC_INPUT_PATHS` have one caller, `wave_exec.rs`.
- **F9 [verified]:** the spec 0025 D8 host lock is `${XDG_STATE_HOME:-$HOME/.local/state}/uas/build.lock`
  (`wave_flow.rs` 436 to 442 and 594), taken with `File::lock` (`wave_exec.rs` 1263 to 1279).
  [reported, spec 0025] `uas wave step` exits 0 on a pass, 1 on a halt, 2 when not ready, 3 on
  an executor error.
- **F10 [verified]:** saga has no CPM and no waves. `dagr stats` reports `critical_path`, the
  longest chain of unfinished tasks by count, with no durations, no float and no wave split
  (`crates/dagr/src/stats.rs` 45 to 50 and 189). A flow limits concurrency only by
  `max_in_flight` and `per_agent` (`crates/saga-domain/src/flow.rs` 37, 41). `git grep -nE
  'merge_gate|merge-gate|merge_on_green|automerge' origin/development -- crates` prints nothing;
  control: `git grep -li compensat origin/development -- crates` finds 2 files.
- **F11 [verified]:** saga has no halt-all and no compensation. `compensat` occurs 0 times in
  bifrost `executor.rs`; a failed step cancels only its dependents (lines 217 to 252).
  Compensation exists only as states in `saga-domain/src/cortex_types.rs` 95 to 98.
- **F12 [verified]:** saga commit 962e476 (2026-10-10 13:02 -0400) changed three spec 0025 facts.
  `run_verify` now bounds `verify_cmd` by `SAGA_VERIFY_TIMEOUT_SECS`, default 900 s, and kills
  the process group (`executor.rs` 1324 to 1373); spec 0025 F22 said no bound. `agent = "none"`
  runs `verify_cmd` with no harness (line 697, test line 2207), so the spec 0025 D2 shim cost is
  avoidable. `verify_cmd` gets parameter substitution (`saga-flow/src/instantiate.rs` 193 to 200);
  spec 0025 F9 said none. Any non-zero exit fails the step as `exit N` (line 1392): saga has
  no not-ready state. The uas merge step's outer bound is 960 s (`wave_flow.rs` 106, 242), so
  saga's 900 s default kills it first.
- **F13 [verified]:** saga spec 0039 (commit 1a2efb0, status implemented 2026-10-11) took the
  team-checking half. Its Notes (line 188) say the uas lane drafts the CPM migration first and
  the saga lead lane reviews it against `crates/dagr/src/stats.rs`.
- **F14 [verified]:** `~/.local/bin/saga` resolves to the `target/release/saga` of the same saga
  checkout, built 2026-10-10 16:29 -0400 (`readlink -f`; `stat`). [asserted] It holds 962e476.
- **F15 [verified]:** `gh api repos/unsigned-gg/uas --jq .private` prints `true`. clap exits 2
  for `uas wave` with no subcommand and for `uas no-such-cmd --help` ("unrecognized subcommand").

## Design

### Decisions

A `ruled` row binds uas under the ruling. A `proposed: for the saga lane` row needs that lane's
agreement. A row marked `Christian to rule` waits on him.

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Saga owns plan reading, CPM, wave planning, wave execution (dispatch, readiness, halt, compensation), PR opening and the merge step. uas owns slot leases, the slot runtime, `LandingActivity::reconcile` and `LandingReceipt` with its spec 0022 D11 JSON. | The ruling; the spec 0025 banner puts wave execution in saga. | Christian, lane w25:p1, 2026-10-10; w25:p1 brief; ruled for uas |
| D2 | The removal set (below) leaves uas in one PR, after saga passes the parity set (D5). Until then it carries the D10 line and takes fixes only. | One cut keeps one working path at every commit. A new feature in the window is work saga must port (F3). | w25:p1 brief, 2026-10-10; ruled for uas |
| D3 | Saga builds the wave executor of spec 0025 natively (asks below). | Saga has none of it (F10, F11). | w25:p1 brief, 2026-10-10; proposed: for the saga lane |
| D4 | `.saga/plans/uas-wave-smoke.toml`, `tasks/wave-smoke-build/`, the parity golden under `.saga/parity/` and the three hand-written flows stay in uas as data saga reads. The removal PR deletes the generated `uas-cpm-wave-w*.toml`. `engineering/plans/` and the `uas-planner` persona stay uas documents. | A generated flow calls `uas wave step` (F7); no flow may call a removed command. | w25:p1 brief, amended by uas-engineer for F7, 2026-10-10; ruled for uas |
| D5 | The parity set: the spec 0025 admission, not-ready and halt rows; the `tests/cpm_wave.rs` cases; the smoke plan's wave split (F7); and the smoke Builder receipt. The uas tests (F6) are the reference. | No fixture files exist (F6). Row tables port, as saga 0039 ported the spec 0046 rows. | w25:p1 brief; uas-engineer, 2026-10-10; acceptance ruled for uas, port proposed: for the saga lane |
| D6 | Specs 0024 and 0025 get a banner that points here and marks each moved rule superseded. Spec 0022 records D8. | One subject has one live spec. | w25:p1 brief, 2026-10-10; ruled for uas |
| D7 | The landing edge form. Lane lean: saga takes a rev-pinned Cargo dependency on `uas-domain` and `uas-engine`. Alternatives: a `uas` subcommand that prints the receipt JSON; or saga runs `blackwall reconcile` itself and the D11 JSON is the only contract. | The lean follows the brief's "library". Its costs: uas is private (F15), spec 0008 sets service edges, and spec 0022 D1 sets a subprocess boundary. | w25:p1 brief (library); proposed: for the saga lane; Christian to rule |
| D8 | `LandingActivity::open_pr` and `land` leave uas with the removal set. Saga opens the PR with `blackwall pr <run> --base development --body -` and the D11 JSON. `reconcile` stays. | D1: no uas code opens a PR. | uas-engineer reading of the brief, 2026-10-10; Christian may override |
| D9 | `merge_commit`, `NotAudited` and the D11 field set stay in the receipt; saga becomes the only writer of `merge_commit`. `EVALUATOR_PATHS` and `CRITIC_INPUT_PATHS` stay in `uas-domain` (spec 0023) as the D17 halt set saga applies. | The receipt is the contract. Spec 0023 owns the path lists. | uas-engineer, 2026-10-10; ruled for uas |
| D10 | The deprecation line is `deprecated: moves to saga (spec 0049); fixes only`. | A saga run keeps a step's stderr in its receipt, so the log shows it. | uas-engineer, 2026-10-10 |

Removal set (D2), deleted in one PR:
- `uas wave` and its three subcommands, `crates/uas-cli/src/wave_flow.rs`,
  `crates/uas-cli/tests/wave_flow.rs`, the `UAS_WAVE_*` variables (F2) and their rows in
  `crates/uas-cli/tests/conventions.rs`.
- `uas harness run --show-cpm --wave --max-concurrency`, its in-process wave loop, the two
  `src/main.rs` wave tests, and the `HarnessReport` fields `cpm_schedule` and `wave_plan`. The
  report JSON changes, so the removal PR adds a `Removed` CHANGELOG line.
- In `uas-engine`: `src/cpm.rs`, `src/wave.rs`, `src/wave_exec.rs`, `cpm_schedule` and
  `plan_waves` on `SagaFlowCoordinator`, `WorkflowEvent::WaveStep`, `WaveOutcome`,
  `LandingActivity::open_pr` and `land` (D8), `tests/wave_exec.rs` and `tests/cpm_wave.rs`.
- `.saga/flows/uas-cpm-wave-w0.toml` and `uas-cpm-wave-w1.toml` (D4).

The saga asks (D3), proposed for the saga lane, each with its spec 0025 source:
- CPM per spec 0024: durations, ES, EF, LS, LF, float, and waves of at most `max_concurrency`
  steps, zero-float first.
- Dispatch by role (0025 table). A Builder is a blackwall run. A Critic is a `uas-critic` run
  with the 0025 argv and the D17 halt set. A Probe is a command at the wave base. A MainSession
  step is a saga gate.
- The 0025 D8 host lock at the F9 path, shared with uas while both executors exist.
- Not-ready kept apart from failure (0025 not-ready rows). Saga fails every non-zero exit today (F12).
- Halt-all and newest-first compensation (0025 halt rows, Compensation section; spec 0009).
- PR opening (D8) and the merge step through `blackwall merge-gate`, unarmed per 0025 D15.
  The readiness pre-read runs under `timeout -k 5 30`. The D11 receipt is the contract.

### Requirements (EARS)

- **[REQ-0049-01]** (Ubiquitous): After the removal PR, no uas crate shall read `.saga/plans/`,
  compute a CPM schedule or a wave plan, run a wave step, or open or merge a PR (D1, D8).
- **[REQ-0049-02]** (Ubiquitous): uas shall keep `LandingActivity::reconcile` and
  `LandingReceipt`, and the receipt JSON shall keep the spec 0022 D11 field set, including
  `merge_commit` and `NotAudited` (D1, D9).
- **[REQ-0049-03]** (State-driven): WHILE the removal set exists, `uas wave --help` shall print
  the D10 line. Each run of a `uas wave` subcommand, and of `uas harness run` with `--show-cpm`
  or `--wave`, shall print it on stderr (D2, D10).
- **[REQ-0049-04]** (State-driven): WHILE the removal set exists, a commit that touches a
  wave-only file (Test) shall not carry a `feat` subject, and no commit shall add a subcommand,
  flag, variable or plan key to the removal set (D2).
- **[REQ-0049-05]** (Unwanted behavior): IF saga has not passed the parity set (D5), THEN the
  removal PR shall not merge.
- **[REQ-0049-06]** (Event-driven): WHEN the removal PR merges, it shall delete the whole removal
  set and keep `.saga/plans/uas-wave-smoke.toml`, `tasks/wave-smoke-build/` and `.saga/parity/`
  (D2, D4).
- **[REQ-0049-07]** (Event-driven, saga): WHEN saga runs a wave plan, saga shall compute the spec
  0024 schedule, split the steps into waves of at most `max_concurrency`, zero-float first, and
  dispatch each step by its spec 0025 role (D3).
- **[REQ-0049-08]** (Ubiquitous, saga): Saga shall hold the F9 host lock around each reconcile,
  Critic run and Probe, so one host runs at most one `make check JOBS=2` (spec 0025 D8).
- **[REQ-0049-09]** (Unwanted behavior, saga): IF a step meets a spec 0025 not-ready row, THEN
  saga shall keep it queued and check it again. It shall not fail the step or cancel its
  dependents. Where the check runs as a command, exit 2 means not ready (F12).
- **[REQ-0049-10]** (Unwanted behavior, saga): IF a step meets a spec 0025 halt row, THEN saga
  shall start no further step of the plan and compensate the passed steps newest first. It shall
  go on past a failed compensator (spec 0009, F11).
- **[REQ-0049-11]** (Event-driven, saga): WHEN a Builder's Critic passes and the plan sets
  `open_prs`, saga shall open the PR with the D11 JSON on stdin (D8). WHERE the plan also sets
  `merge_on_green`, saga shall run the spec 0025 D15 merge step under a bound it enforces.
- **[REQ-0049-12]** (Optional feature, saga): WHERE saga runs `.saga/plans/uas-wave-smoke.toml`
  on the spec 0025 fake tools, saga shall produce the F7 wave split. The Builder receipt JSON,
  with `at` removed, shall equal `.saga/parity/wave-smoke-receipt.json` (D5).

## Plan

Order: uas deprecation, saga build, parity, uas removal, spec amendments. The integrator sends
every saga ask over the mesh; this lane sends nothing.

- [ ] P1, uas deprecation, one PR, `uas-builder` on sonnet, a `chore` subject: the D10 line in
      `uas wave --help` and on stderr (REQ-0049-03), and a `Deprecated` CHANGELOG line. Red
      first: Test rows 1 and 2.
- [ ] Send the saga lane D3, D5 and D7 and the F12 corrections to spec 0025. Saga spec 0039
      already names its review against `dagr/src/stats.rs` (F13).
- [ ] Saga build, by the saga lane in its own spec: REQ-0049-07 to 11.
- [ ] Parity golden, one uas PR before removal: run `uas wave step --step wave-smoke-build` on
      the smoke plan with the spec 0025 fake tools on PATH. Commit `jq -S '.receipt | del(.at)'`
      of its `<state-dir>/wave-smoke-build.json` record as `.saga/parity/wave-smoke-receipt.json`,
      with the fake tools' canned outputs beside it (D4, D5).
- [ ] Parity: saga ports the D5 set into its tests and runs the smoke plan on those canned
      outputs (Test rows 9 to 15).
- [ ] Removal, one uas PR after parity (REQ-0049-05), a `refactor!` subject: delete the removal
      set, rewrite the `tests/landing.rs` helper to call `reconcile`, add a `Removed` CHANGELOG line.
- [ ] Spec amendments (D6): banners on 0024 and 0025, D8 in 0022, and the follow-up item of
      0046's Plan ticked.

## Test

```bash
cargo build -q -p uas-cli -j 2 && u=target/debug/uas
"$u" wave --help 2>&1 | grep -c 'deprecated: moves to saga (spec 0049)'
"$u" wave --help >/dev/null 2>&1; echo "exit $?"
```

Run from the repo root with `u` set as above. Today is 2026-10-10 at 19c19ea. Rows 1 to 8 are
uas checks; rows 9 to 15 are saga checks, whose commands the saga lane names. The wave-only
files are `wave_flow.rs` and `tests/wave_flow.rs` in `crates/uas-cli`, and `src/wave_exec.rs`,
`src/wave.rs`, `src/cpm.rs`, `tests/wave_exec.rs` and `tests/cpm_wave.rs` in `crates/uas-engine`.

| # | Check | Today | Passes when | Covers |
|---|---|---|---|---|
| 1 | `"$u" wave --help \| grep -c 'spec 0049'` | 0 (F1) | 1 after P1 | REQ-0049-03 |
| 2 | `"$u" wave flow --from .saga/plans/uas-wave-smoke.toml --out-dir "$(mktemp -d)" 2>&1 >/dev/null \| grep -c 'spec 0049'`; the same for `"$u" harness run --dry-run --show-cpm --flow .saga/flows/gates.toml` | 0; 0 | 1; 1 after P1 | REQ-0049-03 |
| 3 | `git log --format=%s <P1>..origin/development -- <wave-only files> \| grep -c '^feat'` | 1 over `b1e9cdf..19c19ea` (c61c812, F3), the control | 0 | REQ-0049-04 |
| 4 | `"$u" wave --help; echo $?` | exit 0 | exit 2, `unrecognized subcommand 'wave'`; control: F15 | REQ-0049-01, 06 |
| 5 | `"$u" harness run --help \| grep -cE -- '--wave\|--show-cpm'` | 2 | 0 | REQ-0049-01 |
| 6 | the F4 grep; `grep -cE 'pub fn (open_pr\|land)\b' crates/uas-engine/src/landing.rs` | 10 files; 2 | 0; 0. Control: `grep -c 'pub fn reconcile'` stays 1 | REQ-0049-01, 02 |
| 7 | `ls .saga/plans/uas-wave-smoke.toml tasks/wave-smoke-build .saga/flows/uas-cpm-wave-w0.toml` | all three exist | the first two exist; the flow is gone | REQ-0049-06 |
| 8 | `cargo nextest run -p uas-engine --build-jobs 2 --test landing`; the same `-p uas-domain landing_receipt` | 15 and 13 pass | green, reconcile cases kept | REQ-0049-02 |
| 9 | saga's wave split of the smoke plan | no planner (F10) | w0 build and spec, w1 critic (F7) | REQ-0049-07, 12 |
| 10 | `diff .saga/parity/wave-smoke-receipt.json <(jq -S 'del(.at)' saga.json)`, the golden from the Plan | no golden, no saga receipt | empty diff. Control: another changeset id in the fake `start` output makes it non-empty | REQ-0049-12 |
| 11 | a fake reconcile that logs `LOCK NOT HELD` when the F9 lock is free | no saga run | nothing logged | REQ-0049-08 |
| 12 | a step whose readiness check exits 2 | fails as `exit 2` (F12) | stays queued | REQ-0049-09 |
| 13 | a Critic that returns `VERDICT: DEFECTS` | no halt (F11) | no new step starts; receipts newest first | REQ-0049-10 |
| 14 | with `open_prs`, the fake `blackwall pr` argv and stdin | no saga PR step (F10) | argv ends `--body -`; stdin is the D11 fenced JSON, `pr_number` null | REQ-0049-11 |
| 15 | with `merge_on_green`, a fake `blackwall merge-gate` that sleeps past the step bound | no merge step (F10) | the step fails at its bound and its dependents are canceled; nothing hangs | REQ-0049-11 |

Row 4's control: `uas no-such-cmd --help` exits 2 today, so clap's exit is proven. Row 3's
control is the window since the hold: it counts c61c812, the spec 0042 `feat` commit on a
wave-only file (F3). P1 is a `chore` and the removal a `refactor!`, so neither counts.

## Notes

- The ruling (Christian, lane w25:p1, 2026-10-10 about 23:55Z, verbatim): "migrate all CPM and
  team-checking from uas to saga". [reported: w25:p1 brief, 2026-10-10]
- The saga lane's reply: no mesh reply was relayed to this lane as of drafting [reported: w25:p1
  brief]. Saga spec 0039 Notes commit that lane to review this draft (F13).
- Caveat: the saga facts are from `origin/development` 2f9b23a. The installed binary's commit is
  asserted (F14).
- Caveat for the window: a uas merge step needs `SAGA_VERIFY_TIMEOUT_SECS` of at least 960, or
  saga kills it at 900 s (F12). The generated flows could use `agent = "none"` (F12). Both are
  parked: D2 allows fixes only, and the flows go at removal.
- Parked: after removal, `EVALUATOR_PATHS` and `CRITIC_INPUT_PATHS` have no caller in uas (F8).
  They stay for spec 0023 and for saga (D9).
- Parked: `LandingActivity` has no slot caller yet; the spec 0022 Plan wires it after settle.
- Parked: spec 0025 P15 (the pre-read `timeout`) is built in `wave_exec.rs` 972 to 999 but not
  ticked there. D6's amendment of 0025 can tick it.
- Parked: where spec 0048 lands, the registry row for 0049 sits after it.
