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
them and the one activity in `cas-engine` that drives it. It is the custody
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
- **F6 [verified]:** `cas-domain::attestation` has `EvidenceTier { Verified,
  Reported, Heuristic, Asserted }` and `Attestation { signer: PrincipalRef,
  evidence: EvidenceTier, receipt: String, at: u64 }`. The vendored saga
  contract `vendor/saga/contracts/evidence.rs` has the same four tiers,
  lowercase on the wire.
- **F7 [verified]:** spec 0014 gives every slot an `AuditReceipt { run_hash,
  events, dropped_events, attested }`: a per-run transcript digest of the
  exec events, `attested` only when nothing was dropped.
- **F8 [verified]:** `cas-engine::saga` already parses flows (`SagaFlowDef`,
  `SagaStep`) and runs them with compensation (`SagaFlowCoordinator`,
  `CompensationReceipt`), spec 0009. There is no landing activity yet.
- **F9 [verified]:** `.saga/flows/estate-custody-rollout.toml` in this repo drives
  the chain by prompt: terrain assert, custody verify, jailed run, settle,
  reconcile gate, PR landing on green. Its steps are agent prompts with
  `verify_cmd` checks, not a typed activity.

## Design

### Requirements (EARS)

- **Ubiquitous:** The substrate shall land worker changesets only through
  Blackwall `reconcile` and `pr`; no slot shall push a branch or open a PR
  itself.
- **Ubiquitous:** Every landed PR shall carry a `LandingReceipt` in its body:
  the blackwall run hash, the changeset hash, the base commit, the verify
  command and its exit code, the slot `run_hash`, and the evidence tier.
- **Event-driven:** WHEN a slot run settles with a changeset, `cas-engine` shall
  run the landing activity: reconcile against the slot's recorded base commit,
  then open the PR.
- **Event-driven:** WHEN the verify command exits non-zero in reconcile, the
  landing activity shall fail the step, record the exit code in the receipt,
  and open no PR.
- **State-driven:** WHILE a `LandingReceipt` has no `verify_exit = 0`, its
  evidence tier shall be at most `reported`; `verified` requires the exit code.
- **Unwanted behavior:** IF the slot's base commit is not an ancestor of the
  current `development` head, THEN the landing activity shall refuse to apply
  the changeset and shall record `stale_base` with both commits (F3).
- **Unwanted behavior:** IF the slot's `AuditReceipt` is not `attested`
  (dropped events), THEN the landing receipt shall carry `audit: unattested`
  and the PR body shall say so; landing is not blocked.
- **Optional feature:** WHERE saga spec 0035 ships, the landing activity shall
  accept a `blackwall_dispatch` run handle in place of a local run hash, with
  the same receipt.

### The landing activity

`cas-engine::landing::LandingActivity`, one activity in the saga step graph
(spec 0009), after the slot's settle step and before the operator gate:

| # | Step | Command or call | Receipt field |
|---|---|---|---|
| 1 | Resolve the base | the slot's recorded base commit from the lease (spec 0015 worktree fork point) | `base_sha` |
| 2 | Stale-base check | `git merge-base --is-ancestor <base_sha> origin/development` | `stale_base: bool` |
| 3 | Reconcile | `blackwall reconcile <run> --base <base_sha>` (runs `[verify]` in a temporary worktree) | `blackwall_run`, `changeset_hash`, `verify_cmd`, `verify_exit` |
| 4 | Open the PR | `blackwall pr <run>` (pushes `blackwall/<short>`, PR to `development` with custody trailers) | `pr_number`, `branch` |
| 5 | Attach evidence | `AuditReceipt` from spec 0014, the harness report from spec 0008 | `run_hash`, `audit`, `evidence` |

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
    pub verify_cmd: String,
    pub verify_exit: Option<i32>,  // None when reconcile did not run
    pub branch: Option<String>,    // blackwall/<short>
    pub pr_number: Option<u64>,
    pub evidence: EvidenceTier,    // Verified iff verify_exit == Some(0)
    pub at: u64,
}
```

It lives in `cas-domain` as pure types (zero I/O), is serialized with serde,
and gets a roundtrip test. `cpu_cap` is copied from the slot's audit record
(spec 0020 D4), so the PR body shows whether the host throttling invariant held. The PR body renders it as a table. The same record
is the `receipt` string of a spec 0008 `Attestation` whose signer is the
landing activity's principal.

### What stays with people

- The merge. `automerge` stays false (F2). A lane merges on green with a merge
  commit pinned to the head, or the operator does. The activity ends at the
  open PR.
- The operator gate before landing (spec 0008 settle queue, heimdal in saga)
  stays where it is; this spec does not move it.

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | Landing goes through the `blackwall` CLI (`reconcile`, `pr`) by subprocess in phase 9. A library path is a later decision with a measurement. | CAS lane w25:p1, 2026-10-10 |
| D2 | The receipt is `LandingReceipt` above, in `cas-domain`; `verified` only with `verify_exit == 0`. | CAS lane w25:p1, 2026-10-10 |
| D3 | A stale base refuses to land (F3). No automatic rebase of a whole-file blob. The slot re-runs on a fresh base. | CAS lane w25:p1, 2026-10-10 |
| D4 | An unattested audit does not block landing; it is recorded and visible in the PR body with the dropped count. Alternatives considered: block until attested; land with the tier capped at `reported`. | Christian, interview in lane w25:p1, 2026-10-10: record it, land anyway |
| D5 | Target branch is `development` (spec 0003, `blackwall.toml`). Promotion to `main` stays the promotion PR. | spec 0003 |
| D6 | The `estate-custody-rollout.toml` flow stays as the operator-driven path; the activity is the programmatic one. They share the receipt. | CAS lane w25:p1, 2026-10-10 |

## Plan

- [x] Christian ruled D4 (2026-10-10): record it, land anyway.
- [ ] `cas-domain`: `LandingReceipt`, `AuditStatus`; serde roundtrip test.
- [ ] `cas-engine::landing`: the activity, with a fake `blackwall` (a script on
  `PATH` that records its argv and returns a canned run record) for tests; one
  red-first test per Unwanted-behavior requirement (stale base refuses;
  non-zero verify opens no PR).
- [ ] Wire the activity into `SagaFlowCoordinator` after settle; compensation on
  failure removes the temporary branch if `blackwall pr` did not push.
- [ ] Live gate on ceres: `blackwall doctor` green, one real run through
  `estate-custody-rollout.toml`, PR body shows the receipt table.
- [x] Spec 0008: cite this receipt as the `Attestation.receipt` for landings.

## Test

```bash
make spec-check
cargo nextest run -p cas-domain --build-jobs 2 landing_receipt
cargo nextest run -p cas-engine --build-jobs 2 landing
blackwall doctor
```

The engine tests run unprivileged with the fake `blackwall`. `blackwall doctor`
is the live check; it needs Landlock and the custody store on the host.

## Notes

- Saga's `orlog` and this receipt carry the same facts (path, hash, exit code,
  tier). When saga spec 0028's server is live, the landing activity can post the
  receipt there as a `receipt_written` event; the schema here is a strict
  superset of what that event needs.
- Queue label: this spec was "0027 custody landing" on the lane's candidate list
  (`~/handoffs/2026-10-09/cas-ears-spec-queue.md`); spec numbers are sequential,
  so it is 0022.
