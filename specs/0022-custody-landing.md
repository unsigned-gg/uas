---
status: planned
created: 2026-10-10
tags: [custody, blackwall, saga, orlog, receipt, landing, phase-9]
priority: high
depends_on: [0002-verification-gates-and-evidence, 0003-branch-model-and-promotion, 0008-soma-cortex-saga-reverie-integration, 0009-saga-compensation-runner-and-durability, 0014-ebpf-priority-clamping-and-audit-probes, 0015-slot-pool-cgroup-freezer-and-gitoxide]
---

# 0022-custody-landing

## Overview

Roadmap phase 9 (research design section 6): connect saga `orlog` exit-0
receipts to Blackwall `reconcile`, so a worker's changeset lands as a signed PR
to `development` with its provenance attached. Today the chain exists as
separate tools and a hand-driven flow. This spec names the contract between
them and the one activity in `uas-engine` that drives it. It is the custody
half of the roadmap; the slot half is spec 0015.

Facts, measured 2026-10-10 on ceres unless stated:

- **F1 [verified]:** the Blackwall chain is `start` (Landlock jail) then `settle`
  then `reconcile` (temporary worktree from a base commit, applies the
  changeset, runs the `[verify]` command, creates a signed branch
  `blackwall/<short>` with custody trailers) then `pr` then `merge-gate`.
  `blackwall --help` lists each. `blackwall.toml` in this repo sets
  `verify.command = "make check JOBS=2 && make hygiene JOBS=2"`,
  `pr.base = "development"`, `merge.automerge = false`.
- **F2 [verified]:** `merge.automerge` is false on purpose: blackwall 1.2.0
  `merge-gate` squash-merges with no head pin, and this repo lands with a merge
  commit pinned to the head (`blackwall.toml` comment; spec 0003; merge-style SOP).
- **F3 [verified]:** a custody changeset is a whole-file blob, not a patch.
  `reconcile` must get `--base <sha>`; applying a stale blob reverts work landed
  since the clone (`~/.agents/sops/custody-changeset-rebasing.md`, measured
  2026-08-26).
- **F4 [reported]:** saga spec 0018 (`unsigned-gg/saga@a53a9d6:specs/0018-persistence-and-messaging.md`):
  `orlog` is the append-only run journal; receipts hold the file path, hash and
  exit code; a projection cannot mark `verified` without a `receipt_written`
  event that carries an exit code. Spec 0028: the server never raises a tier;
  `verified` comes only from a `verify_cmd` receipt; no unsigned commit, no
  direct push to `main`, PRs target `development`.
- **F5 [reported]:** saga spec 0035 (status proposed, evidence asserted) adds a
  `runner = { blackwall_dispatch = ... }` placement to a flow step and a
  `RunnerKind` in `saga-domain`. It is a design seam, not shipped code.
- **F6 [verified]:** `uas-domain::attestation` has `EvidenceTier { Verified,
  Reported, Heuristic, Asserted }` and `Attestation { signer: PrincipalRef,
  evidence: EvidenceTier, receipt: String, at: u64 }`. The vendored saga
  contract `vendor/saga/contracts/evidence.rs` has the same four tiers,
  lowercase on the wire.
- **F7 [verified]:** spec 0014 gives every slot an `AuditReceipt { run_hash,
  events, dropped_events, attested }`: a per-run transcript digest of the
  exec events, `attested` only when nothing was dropped.
- **F8 [verified]:** `uas-engine::saga` already parses flows (`SagaFlowDef`,
  `SagaStep`) and runs them with compensation (`SagaFlowCoordinator`,
  `CompensationReceipt`), spec 0009. There is no landing activity yet.
- **F9 [verified]:** `.saga/flows/estate-custody-rollout.toml` in this repo drives
  the chain by prompt: terrain assert, custody verify, jailed run, settle,
  reconcile gate, PR landing on green. Its steps are agent prompts with
  `verify_cmd` checks, not a typed activity.

Facts added by the 2026-10-10 amendment (P5a critic findings). Source lines are
`unsigned-gg/blackwall@3d98dc1`, whose `crates/blackwall-cli/Cargo.toml` says
1.2.0; the local checkout is dirty, so source facts are `reported`.

- **F10 (superseded by F13, F14):** blackwall 1.2.0 has no exit code mapping.
  [verified 2026-10-10, installed `blackwall` 1.2.0 in an empty temp dir]
  `blackwall reconcile` with no arguments exits 2 (usage error);
  `blackwall reconcile deadbeefdeadbeef --base HEAD` exits 1
  (`Error: Store(NotFound(..))`); the control `--help` exits 0.
  [reported, `crates/blackwall-cli/src/main.rs:27`] `main` returns
  `Result<(), Box<dyn Error>>`, so every error exits 1, a failed verify
  command included. The verify command's own code is only in the error text
  (`reconcile.rs:76`).
- **F11:** with no `[verify] command`, `reconcile` warns and reconciles without
  a gate. [reported, `crates/blackwall-cli/src/reconcile.rs:218-223`]
- **F12 (superseded by F13) [verified 2026-10-10, `blackwall pr --help`]:**
  `blackwall pr` 1.2.0 takes `--base`, `--draft`, `--force` and `--no-verify`.
  It has no PR-body flag; it writes its own body (`pr.rs:260`, [reported]).

Facts added by the second 2026-10-10 amendment: blackwall rebuilt from soma
main `ec076f6`, installed on ceres at `~/.local/bin/blackwall` (mtime
2026-10-10 03:19).

- **F13 [verified 2026-10-10 07:22Z, `blackwall --version`, `--help`, `echo $?`
  in an empty temp dir]:** `--version` still prints 1.2.0, so the version string
  does not tell this build from the F10 build. `pr --help` and `reconcile --help`
  list `--body <BODY>`, "Supports "-" for stdin". `reconcile` with no arguments
  exits 2; `pr --bogus x` exits 2; `reconcile deadbeefdeadbeef --base HEAD`
  exits 1; the control `--help` exits 0.
- **F14 [reported, soma lane, mesh message 8a5bcb74, 2026-10-10]:** at soma
  `ec076f6`, `reconcile` and `pr` exit 3 when the verify gate fails, 1 on any
  other error and 2 on a usage error. `--help` lists no exit codes, so exit 3 is
  not verified on this host.
- **F15 [verified 2026-10-10, `blackwall pr --help`]:** `pr` "reconciles first
  if the run has no branch yet"; `--no-verify` skips the gate "if a reconcile is
  needed". That `pr` runs no gate after a reconcile is [asserted] from this text.
- **F16 [verified 2026-10-10, `blackwall doctor; echo $?`]:** `doctor` exits 1
  in an empty temp dir and in a fresh worktree of this repo, which has no
  `.blackwall/` (`.gitignore` excludes it). The control: it exits 0 in the main
  checkout `/home/ctodie/projects/cerebral/uas`, where `blackwall init`
  ran. So the Test line `blackwall doctor` can fail.

## Design

### Requirements (EARS)

- **Ubiquitous:** The substrate shall land worker changesets only through
  Blackwall `reconcile` and `pr`; no slot shall push a branch or open a PR
  itself.
- **Ubiquitous:** Every landing outcome shall carry a `LandingReceipt`: the
  blackwall run hash, the changeset hash, the base commit, the verify command,
  the `reconcile` exit code, the slot `run_hash`, and the evidence tier. A
  `LandingError` is not an outcome and carries no receipt (D7).
- **Event-driven:** WHEN a slot run settles with a changeset, `uas-engine` shall
  run the landing activity: reconcile against the slot's recorded base commit,
  then open the PR.
- **Event-driven:** WHEN `blackwall reconcile` exits 3 (the verify gate failed,
  F14), the landing activity shall return `VerifyFailed`, record 3 as
  `verify_exit`, and open no PR (D7).
- **Event-driven:** WHEN `blackwall reconcile` exits 0, the landing activity
  shall run `blackwall pr` with `--body -` and write the receipt as JSON to its
  stdin (D4, D11).
- **State-driven:** WHILE a `LandingReceipt` has no `verify_exit = 0`, its
  evidence tier shall be at most `reported`; `verified` requires the exit code.
- **Unwanted behavior:** IF the slot's base commit is not an ancestor of the
  current `development` head, THEN the landing activity shall refuse to apply
  the changeset and shall record `stale_base` with both commits, `base_sha`
  and `development_head`, on the receipt (F3, D9).
- **Unwanted behavior:** IF the host `blackwall.toml` sets no `[verify]`
  command, THEN the landing activity shall make no `blackwall` call, open no
  PR, and return `NoVerifyGate` (F11, D8).
- **Unwanted behavior:** IF `blackwall reconcile` exits with a code other than
  0 or 3, or a signal ends it, THEN the landing activity shall return a
  `LandingError`, build no receipt, and open no PR (D7).
- **Unwanted behavior:** IF the slot's `AuditReceipt` is not `attested`
  (dropped events), THEN the landing receipt shall carry `audit: unattested`
  with the dropped count; landing is not blocked (D4).
- **Optional feature:** WHERE saga spec 0035 ships, the landing activity shall
  accept a `blackwall_dispatch` run handle in place of a local run hash, with
  the same receipt.

### The landing activity

`uas-engine::landing::LandingActivity`, one activity in the saga step graph
(spec 0009), after the slot's settle step and before the operator gate:

| # | Step | Command or call | Receipt field |
|---|---|---|---|
| 0 | Read the gate | `[verify] command` from `<repo_dir>/blackwall.toml`, the host file that `reconcile` reads (spec 0023, Pin) | `verify_cmd` |
| 1 | Resolve the base | the slot's recorded base commit from the lease (spec 0015 worktree fork point) | `base_sha` |
| 2 | Stale-base check | `git merge-base --is-ancestor <base_sha> <development_ref>`: 0 goes on; 1 runs `git rev-parse <development_ref>` and returns `StaleBase`; any other exit is a `LandingError` | `stale_base`, `development_head` |
| 3 | Gate check | no `verify_cmd`: return `NoVerifyGate`, with no `blackwall` call (D8) | none |
| 4 | Reconcile | `blackwall reconcile <run> --base <base_sha> --branch blackwall/<first 12 chars of run>`; exit 3 returns `VerifyFailed`; any other non-zero exit or a signal is a `LandingError` (D7) | `blackwall_run`, `changeset_hash`, `verify_exit`, `branch` |
| 5 | Open the PR | `blackwall pr <run> --base <pr_base> --body -`, the receipt JSON on stdin (D11); a non-zero exit is a `LandingError`; the PR number is the last `/pull/<n>` in stdout | `pr_number` |
| 6 | Attach evidence | `AuditReceipt` from spec 0014, the harness report from spec 0008 | `run_hash`, `audit`, `evidence` |

The steps run in this order, so a stale base wins over a missing gate. The
activity never passes `--force`, `--no-verify`, `--pr` or `--draft`.

The activity shells out to the `blackwall` CLI in phase 9 (stand it up, then
harden); a library call into `blackwall-store` is a later change recorded in a
new decision, as saga spec 0028 does for `gix`.

### `LandingReceipt`

```rust
pub struct LandingReceipt {
    pub slot_id: SlotId,
    pub run_hash: String,          // spec 0014 AuditReceipt.run_hash
    pub audit: AuditStatus,        // Attested | Unattested { dropped_events }
    pub cpu_cap: String,           // spec 0020: "200000 100000", "max" or "uncapped"
    pub blackwall_run: String,     // run hash from `blackwall show`
    pub changeset_hash: String,    // BLAKE3 changeset id
    pub base_sha: String,
    pub stale_base: bool,
    pub development_head: Option<String>, // D9: Some iff stale_base
    pub verify_cmd: Option<String>, // host blackwall.toml [verify]; None: NoVerifyGate
    pub verify_exit: Option<i32>,  // D7: reconcile process exit; None when it did not run
    pub branch: Option<String>,    // blackwall/<short>
    pub pr_number: Option<u64>,
    pub merge_commit: Option<String>, // spec 0025 D15: set only from a `gh pr view` pre-read
    pub evaluator_touched: bool,   // spec 0023 D3
    pub cost_source: CostSource,   // spec 0023 D2: Reported | None
    pub evidence: EvidenceTier,    // D2 rule; above it only with human_raise (D10)
    pub human_raise: Option<HumanRaise>, // D10, P5b: { by: PrincipalRef, at: u64 }
    pub at: u64,
}
```

It lives in `uas-domain` as pure types (zero I/O), is serialized with serde,
and gets a roundtrip test. `cpu_cap` is copied from the slot's audit record
(spec 0020 D4), so the PR body shows whether the host throttling invariant
held. The PR body carries this receipt as JSON (D11). The same record
is the `receipt` string of a spec 0008 `Attestation` whose signer is the
landing activity's principal.

### What stays with people

- The merge. `automerge` stays false (F2). A lane merges on green with a merge
  commit pinned to the head, or the operator does. The activity ends at the
  open PR. Spec 0025 D15, re-ruled on 2026-10-10, has the wave executor never
  merge: merging runs through saga and blackwall, and no `uas` code reads PR
  checks or runs `gh pr merge`. An earlier form of D15 allowed an executor merge
  behind a default-off flag; that form is retired.
- The operator gate before landing (spec 0008 settle queue, heimdal in saga)
  stays where it is; this spec does not move it.

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | Landing goes through the `blackwall` CLI (`reconcile`, `pr`) by subprocess in phase 9. A library path is a later decision with a measurement. | UAS lane w25:p1, 2026-10-10 |
| D2 | The receipt is `LandingReceipt` above, in `uas-domain`. The computed tier is `verified` iff `verify_exit == Some(0)`, a `[verify]` command exists (`verify_cmd` is `Some`), and `evaluator_touched` is false (spec 0023 D3); otherwise `reported`. | UAS lane w25:p1, 2026-10-10; amended the same day from the P5a critic findings (lane w25:p1, Christian can override) |
| D3 | A stale base refuses to land (F3). No automatic rebase of a whole-file blob. The slot re-runs on a fresh base. | UAS lane w25:p1, 2026-10-10 |
| D4 | An unattested audit does not block landing; it is recorded and visible in the PR body with the dropped count. Alternatives considered: block until attested; land with the tier capped at `reported`. | Christian, interview in lane w25:p1, 2026-10-10: record it, land anyway |
| D5 | Target branch is `development` (spec 0003, `blackwall.toml`). Promotion to `main` stays the promotion PR. | spec 0003 |
| D6 | The `estate-custody-rollout.toml` flow stays as the operator-driven path; the activity is the programmatic one. They share the receipt. | UAS lane w25:p1, 2026-10-10 |
| D7 | `verify_exit` is the exit code of the `blackwall reconcile` process, not of the verify command. For blackwall at soma `ec076f6` (F14): exit 0 goes on to `pr`; exit 3 returns `VerifyFailed` with `verify_exit = Some(3)`; exit 1 (another reconcile error) and exit 2 (a usage error in the activity's own argv) return a `LandingError`, never a receipt. Any other code or a signal is a `LandingError` too (fail closed). An F10 build exits 1 on a failed verify, so it gets a `LandingError`, not a receipt; it also has no `--body`, so `pr` exits 2. A non-zero `blackwall pr` exit, 3 included, is a `LandingError`: `pr` runs the gate only when it must reconcile (F15). | UAS lane w25:p1, 2026-10-10, P5a critic finding; amended the same day for soma `ec076f6` (Christian can override) |
| D8 | With no `[verify]` command in the host `blackwall.toml`, the activity returns `NoVerifyGate` before any `blackwall` call and opens no PR. Fail closed: blackwall itself would reconcile without a gate (F11), and SOP `blackwall-custody-process` requires a changeset to pass the verify gate before a PR opens. | UAS lane w25:p1, 2026-10-10, P5a critic finding (Christian can override) |
| D9 | A stale base records both commits on the receipt, `base_sha` and `development_head`, not only on the `StaleBase` outcome. `development_head` is `Some` iff `stale_base`. | UAS lane w25:p1, 2026-10-10, P5a critic finding (Christian can override) |
| D10 | For P5b: the receipt's `try_from` check allows the spec 0023 D3 human raise. Tier order: `verified` > `reported` > `heuristic` > `asserted` (`EvidenceTier` derives no `Ord`, so P5b adds an explicit rank). The stored tier shall not exceed the computed tier (D2) unless `human_raise` is `Some(HumanRaise { by: PrincipalRef, at: u64 })`, which names who raised it and when. A raise lifts only the evaluator cap: the stored tier shall not exceed the tier computed with `evaluator_touched = false`, so no raise gives `verified` without exit 0 and a `[verify]` command. A stored tier below the computed tier is accepted. | UAS lane w25:p1, 2026-10-10, P5a critic finding (Christian can override) |
| D11 | D4's PR-body record goes through `blackwall pr <run> --base <pr_base> --body -` (F13). The activity writes to stdin a fenced block: the line `` ```json ``, then `serde_json::to_string_pretty` of the receipt, then the line `` ``` ``. It is the receipt after reconcile: `verify_exit` 0, `branch` set, `pr_number` null, because the PR does not exist yet. The `Opened` receipt adds `pr_number`; both have the same tier, since the tier rule (D2) does not read `branch` or `pr_number`. Alternatives considered: a Markdown table (people read it, no program can read it back); blackwall's own body (no receipt). | UAS lane w25:p1, 2026-10-10 (Christian can override) |
| D12 | Spec 0025 amends this spec. `AuditStatus` gains `NotAudited` for a wave Builder with no UAS slot: `run_hash` is empty and `cpu_cap` is `"uncapped"`. Landing proceeds; this extends D4 to runs with no audit, pending Christian's ruling on spec 0025 D9. `LandingActivity` splits into `reconcile` and `open_pr`; `land` keeps calling both, so a caller can put a critic between them. `open_pr` takes the reconciled receipt and writes it as the D11 body. | uas-engineer, 2026-10-10, spec 0025 D1 (Christian can override) and D9 (**Christian to rule**) |

## Plan

- [x] Christian ruled D4 (2026-10-10): record it, land anyway.
- [x] `uas-domain`: `LandingReceipt`, `AuditStatus`; serde roundtrip test (P5a, PR #47).
- [x] `uas-engine::landing` (P5a, PR #47): the activity, with a fake `blackwall` (a script on
  `PATH` that records its argv and returns a canned run record) for tests; one
  red-first test per Unwanted-behavior requirement (stale base refuses;
  non-zero verify opens no PR; no `[verify]` command makes no `blackwall` call).
- [x] P5b, `uas-domain`: `development_head` on the receipt (D9); `HumanRaise`
  and the D10 `try_from` rule. Red first: a raise above the computed tier with
  no `human_raise` is rejected; a raise to `verified` with `verify_exit` 1 is
  rejected even with `human_raise` (PR #61).
- [ ] P5c, `uas-engine::landing`: the D7 exit mapping and the D11 PR body. Red
  first: reconcile exit 1 and exit 2 each return a `LandingError`; the `pr`
  argv ends in `--body -`; the fake `blackwall` gets the receipt JSON on stdin,
  `pr_number` null, and an unattested run shows its dropped count there.
- [ ] Wire the activity into `SagaFlowCoordinator` after settle; compensation on
  failure removes the temporary branch if `blackwall pr` did not push.
- [ ] Live gate on ceres: `blackwall doctor` green in the main checkout (F16),
  one real run through `estate-custody-rollout.toml`; the PR body shows the
  receipt JSON (D11).
- [x] Spec 0008: cite this receipt as the `Attestation.receipt` for landings.

## Test

```bash
make spec-check
cargo nextest run -p uas-domain --build-jobs 2 landing_receipt
cargo nextest run -p uas-engine --build-jobs 2 landing
blackwall doctor
```

The engine tests run unprivileged with the fake `blackwall`. `blackwall doctor`
is the live check; it needs Landlock and the custody store on the host. Run it
in the main checkout, where `blackwall init` ran. It exits 1 in a fresh
worktree or a tree with no store (F16), and a non-zero exit fails the gate.

## Notes

- Saga's `orlog` and this receipt carry the same facts (path, hash, exit code,
  tier). When saga spec 0028's server is live, the landing activity can post the
  receipt there as a `receipt_written` event; the schema here is a strict
  superset of what that event needs.
- D4's PR-body record no longer waits on blackwall: the build at soma
  `ec076f6` takes `--body -` (F13, D11). The help text says "Custom body for the
  opened PR". That it replaces blackwall's own provenance body is [asserted];
  the custody trailers stay on the commits either way (F1). This lane does not
  change blackwall.
- Amendment log: 2026-10-10, from the P5a critic findings: F10 to F12, the
  `NoVerifyGate` requirement, the PR-body requirement moved to an optional
  feature, D2 amended, D7 to D10 added, the activity table in the P5a order.
  Second, the same day, for blackwall at soma `ec076f6`: F13 to F16, F10 and
  F12 marked superseded, D7 rewritten for exit 3, D11 added, the PR-body
  requirement made event-driven, a reconcile-exit requirement added, P5b
  ticked, P5c planned.
- Amendment log: 2026-10-10, from spec 0025: D12 (`NotAudited`, the
  `reconcile`/`open_pr` split). Second, from spec 0025 D15: `merge_commit` in the receipt.
  Third, 2026-10-10, Christian's ruling on spec 0025 D15: no uas code merges; a pre-read sets it.
- Queue label: this spec was "0027 custody landing" on the lane's candidate list
  (`~/handoffs/2026-10-09/uas-ears-spec-queue.md`); spec numbers are sequential,
  so it is 0022.
