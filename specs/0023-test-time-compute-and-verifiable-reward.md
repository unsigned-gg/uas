---
status: planned
created: 2026-10-10
tags: [ttc, rollout, saga, budget, receipt, evaluator, phase-4]
priority: medium
depends_on: [0002-verification-gates-and-evidence, 0007-lease-rules, 0008-soma-cortex-saga-reverie-integration, 0014-ebpf-priority-clamping-and-audit-probes, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0022-custody-landing]
---

# 0023-test-time-compute-and-verifiable-reward

## Overview

The research design (section 3.5, Pillar V.2) says CAS "activates test-time
compute scaling" for hard tasks: candidate branches, each validated against
execution receipts, failed branches pruned. Saga already specifies that loop:
rollout trees with a promote, advance, backtrack and speculate policy (saga
spec 0017) and budget caps with harness-reported tokens only (saga spec 0020).
CAS does not build a second loop. This spec says what CAS provides to saga's
loop and what it refuses to do, and it adds the one hardening the research
finding asks for: the evaluator stays out of the agent's reach.

Facts, read 2026-10-10:

- **F1 [reported]:** saga spec 0017 (`unsigned-gg/saga@a53a9d6:specs/0017-speculative-rollout-trees.md`,
  status draft): `TrajectoryNode { node_id, parent_id, attempt_number, agent,
  model, verified, remediation_delta, git_checkpoint, children }`; `verified`
  is true iff a `verify_cmd` exit-0 receipt exists. Policy: promote on verified;
  advance on net progress; backtrack when `regressed > resolved` or the same
  errors persist two rounds; speculate `k` siblings under a semaphore. Rules:
  each rollout branch runs in its own `IsolatedWorktree`; rollouts never satisfy
  a human gate.
- **F2 [reported]:** saga spec 0020 (`0020-budget-caps-and-token-accounting.md`,
  decision of record Christian 2026-10-06): only harness-reported usage counts,
  never a tokenizer estimate; `cost_source` is `reported` or `none`; a step over
  budget settles `failed`, reason `budget exceeded`, evidence `asserted`; a run
  over budget cancels every queued step.
- **F3 [reported]:** research finding
  `cerebral-work/research@4b3ed94:agentic-systems/findings/autoresearch-loop-pattern-and-evaluator-hardening.md`:
  the loop that finds real improvement has a frozen evaluator, one mutable
  surface, a fixed budget per experiment, a scalar metric, and a git-backed
  keep-or-revert ledger. "The loop finds real improvements only when the
  evaluator is isolated from the agent and gated against shortcut gains.
  Without that hardening the agent optimizes the harness, not the task." Its
  Limits section says the public numbers are vendor claims, not replications.
- **F4 [reported]:** literature named by the research lane (mesh reply
  1b8213f2): Snell et al. 2024, arXiv:2408.03314 (test-time compute can beat
  pre-training scale on hard problems); DeepSeek-R1 and DeepSeekMath
  (rule-based execution verification over neural reward models, against reward
  hacking).
- **F5 [verified]:** `cas-domain::lease::Budget { max_tokens: u64, max_wall_secs: u64 }`,
  both above zero (spec 0007). One lease, one slot, one budget. The domain never
  reads a clock.
- **F6 [verified]:** a slot has its own worktree (spec 0015 D2, `gix` checkout
  under `/workspace/slots/<slot-id>`, reset on reclaim), its own cgroup with a
  CPU quota (spec 0020), its own Landlock ruleset (spec 0010), and an
  `AuditReceipt` with a per-run `run_hash` (spec 0014).
- **F7 [verified]:** the verify gate of this repo is `make check JOBS=2 && make
  hygiene JOBS=2` (`blackwall.toml`), and its definition lives in files a worker
  can edit inside its workspace: `Makefile`, `.github/workflows/*`,
  `blackwall.toml`, `deny.toml`, `rust-toolchain.toml`, `_typos.toml`,
  `.config/nextest.toml`. Landlock denies writes outside the workspace, not
  inside it (spec 0010).

## Design

### Requirements (EARS)

- **Ubiquitous:** CAS shall implement no rollout policy, no branch selection and
  no scalar metric of its own; those are saga's (`volva`, spec 0017) and the
  world model's.
- **Ubiquitous:** One rollout branch shall map to one CAS slot lease: its own
  worktree, cgroup, Landlock ruleset, budget and audit receipt.
- **Event-driven:** WHEN saga backtracks or prunes a branch, CAS shall release
  the lease and reclaim the slot (worktree reset, cgroup removed); nothing from
  the pruned branch survives in the pool.
- **Event-driven:** WHEN a branch's `verify_cmd` exits 0 inside the landing
  activity (spec 0022), the branch receipt shall be `verified`; any other exit
  or no run shall leave it at most `reported`.
- **State-driven:** WHILE a branch runs, its token and wall-clock use shall count
  against the lease `Budget` from harness-reported usage only; CAS shall never
  estimate tokens.
- **Unwanted behavior:** IF a branch's changeset touches an evaluator file
  (F7 list), THEN the landing receipt shall carry `evaluator_touched: true` and
  the branch shall not be `verified` by its own verify run; a human reviews
  the evaluator change before the tier can rise.
- **Unwanted behavior:** IF a lease budget is exhausted, THEN CAS shall freeze
  the slot (spec 0015 D3) and report `budget exceeded` to saga; it shall not
  kill the branch on its own, so saga can decide to extend or prune.
- **Optional feature:** WHERE saga speculates `k` siblings from one checkpoint,
  CAS shall fork `k` worktrees from the same base commit, each a separate
  lease, bounded by the pool's slot count and the host throttling invariant.

### What CAS provides to the loop

| saga concept (spec 0017, 0020) | CAS object | spec |
|---|---|---|
| `TrajectoryNode.git_checkpoint` | the slot worktree's base commit; the lease records it | 0015, 0022 |
| `IsolatedWorktree` per branch | one slot: worktree, cgroup, Landlock, netns | 0010, 0011, 0015 |
| `verified` (exit-0 receipt) | `LandingReceipt.verify_exit == Some(0)` | 0022 |
| the reward signal | the `LandingReceipt` plus the `AuditReceipt` (`run_hash`) of the branch | 0014, 0022 |
| step budget, `cost_source = reported` | `lease::Budget` fed by the harness report of spec 0008; `cost_source` copied through | 0007, 0008 |
| backtrack or prune | lease release and slot reclaim | 0007, 0015 |
| speculate `k` | `k` leases from one base; capped by slot count | 0015 |

### Evaluator hardening (F3, F7)

The agent must not be able to move the goalposts. Three lines, in order of cost:

1. **Detect.** The landing activity (spec 0022) diffs the changeset against the
   base and sets `evaluator_touched` when any path matches the evaluator list.
   The list lives in `cas-domain` as a constant with a test, and the repo's
   `blackwall.toml` may extend it.
2. **Cap.** A touched evaluator caps the branch tier at `reported` regardless of
   the verify exit code. Only a human review (the PR) can raise it.
3. **Pin.** The verify command itself comes from the base commit's
   `blackwall.toml`, read by `reconcile` in its temporary worktree before the
   changeset is applied, so a changeset cannot rewrite the command it is judged
   by. Whether blackwall does this today is not measured; the Plan has the probe.

This is the "frozen evaluator" of F3 applied to a repo whose evaluator is files
in the repo. It does not stop a worker from writing a test that cannot fail;
the critic and the PR review remain the line for that (spec 0002).

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | No CAS rollout loop. CAS is the substrate for saga's. Design section 3.5 V.2 is read as a requirement on saga plus the mapping above. | CAS lane w25:p1, 2026-10-10 |
| D2 | Budget accounting is harness-reported only, as saga 0020 rules; `cost_source = none` when a harness reports nothing, and such a branch cannot be `verified` for cost claims. | follows saga 0020 (Christian, 2026-10-06) |
| D3 | A touched evaluator caps the tier at `reported`; it does not block the PR. Alternatives considered: block landing until a human clears it; no special handling. | Christian, interview in lane w25:p1, 2026-10-10: flag and cap at `reported` |
| D4 | The evaluator file list for this repo: `Makefile`, `.github/workflows/**`, `blackwall.toml`, `deny.toml`, `rust-toolchain.toml`, `_typos.toml`, `.config/nextest.toml`, `xtask/**`. | CAS lane w25:p1, 2026-10-10 |
| D5 | Budget exhaustion freezes the slot and reports; it never kills. Saga decides. | CAS lane w25:p1, 2026-10-10 |

## Plan

- [x] Christian ruled D3 (2026-10-10): flag and cap at `reported`.
- [ ] `cas-domain`: `EVALUATOR_PATHS` constant and `is_evaluator_path(&Path)`
  with a test for every entry and a negative control (`crates/cas-domain/src/lib.rs`
  is not an evaluator path).
- [ ] Spec 0022's `LandingReceipt` gains `evaluator_touched: bool` and
  `cost_source: CostSource { Reported, None }`.
- [ ] Probe: does `blackwall reconcile` read `[verify]` from the base commit or
  from the applied changeset? Record the answer with the command in this spec;
  if the latter, file the gap to the blackwall lane.
- [ ] Wire budget exhaustion to the freezer (spec 0015) with a test that freezes
  on `max_tokens` from a fake harness report and never on an estimate.
- [ ] Cross-link: saga spec 0017 names the CAS slot as its `IsolatedWorktree`
  carrier (proposal to lane w1P, not an edit by this lane).

## Test

```bash
make spec-check
cargo nextest run -p cas-domain --build-jobs 2 evaluator_path
```

The evaluator test goes red first by adding `src/lib.rs` to the list and
watching the negative control fail.

## Notes

- Dual-stream `<think>` telemetry (design 3.5 V.1) is spec 0017 D4 of this repo
  (CER-2731), not this spec.
- Gitoxide snapshot and rollback before destructive tool actions (design 3.5
  V.3) is spec 0015 D2, not this spec.
- The public autoresearch numbers (F3 Limits) are not evidence for CAS; nothing
  here cites them as measured.
