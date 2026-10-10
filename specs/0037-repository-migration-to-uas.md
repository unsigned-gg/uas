---
status: planned
created: 2026-10-10
tags: [migration, rename, uas, unsigned-gg, repository, cutover]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0003-branch-model-and-promotion, 0004-dependency-and-licence-policy, 0005-ci-cache-and-runners, 0022-custody-landing, 0025-wave-execution-on-the-custody-chain]
---

# 0037-repository-migration-to-uas

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

The Kube-rs Integration Plan v5 moves this repository from `cerebral-work` to
`unsigned-gg` and renames it from `cas-substrate` to `uas`, the Unsigned Agent
Substrate, as the open compute runtime of a split-monorepo strategy [reported:
the plan, copied at `<surfaces>/cas-ears/kube-rs-plan.md` from the
author pane w1X:p22's brain file, metadata updated 2026-10-10T20:36Z; its
approval provenance is unverified, see Notes]. This spec says what the move
changes inside the repository, in what order, what must be carried across by
hand, what breaks for lanes working here, and which steps are the operator's acts.

Why now: the operator ruled an estate-wide hold on development work for the
migrations plan at 21:08Z and, asked directly, ruled at about 21:15Z that the
migration itself is to be specified under the hold's exemption [reported: his own
session, lane w25:p1, 2026-10-10]. A planned spec lands nothing.

Facts, measured 2026-10-10 on `origin/development` at `ab8112a`:

- **F1 [verified]:** The name is bound in 18 file groups. Tracked files that
  contain `cas-`, `cas_` or the bare word `cas`, counted by top-level path:
  `crates` 94, `specs` 38, `.claude` 8, `.saga` 6, `tasks` 3, `docs` 2, `xtask` 1, `scripts` 1, `release-plz.toml` 1, `README.md` 1, `Makefile` 1, `.github` 1, `engineering` 1, `.ears-spec` 1, `CHANGELOG.md` 1, `Cargo.toml` 1, `AGENTS.md` 1, `.agents` 1. An earlier
  version of this fact said 12 groups; it used a shell `grep` that stopped at
  symlinked skill directories and missed seven groups. Command:
  `git grep -lI -- 'cas-\|cas_\|\bcas\b' -- ':!vendor' ':!Cargo.lock' | sed 's|/.*||' | sort | uniq -c`.
- **F2 [verified]:** `make new-repo NAME=<kebab> [ORG=...]` exists for exactly
  this rename, and `make template-check`, which proves it, fails on
  `development` today because `new-repo` renames only `crates/cas-domain` and
  `crates/cas-cli` while ten `cas-*` crates exist. Tracked as CER-2813.
  Command: `make template-check JOBS=2` on pristine `origin/development`, which
  fails with "failed to read crates/aaa-probe-engine/Cargo.toml".
- **F3 [verified]:** The repository is private, default branch `main`, merge
  commits only with squash and rebase disabled, delete-branch-on-merge on, 14
  labels, no rulesets, no repository-level Actions variables and no
  repository-level Actions secrets. Command: `gh api repos/cerebral-work/cas-substrate`
  and its `rulesets`, `actions/variables`, `actions/secrets` and `labels` routes.
- **F4 [verified]:** `.github/workflows/_rust.yml` line 40 uses sccache on R2
  with "the same secret names as cerebral-work/reverie". With no repository
  secrets (F3), those names resolve to organisation secrets of `cerebral-work`,
  which do not travel with a transfer. Command: `grep -n sccache .github/workflows/_rust.yml`.
- **F5 [verified, 2026-10-10 21:36Z]:** Neither `unsigned-gg/uas` nor
  `unsigned-gg/cas-substrate` exists, and `cerebral-work/cas-substrate` is still
  present and private. Commands: `gh api repos/unsigned-gg/uas` and
  `gh api repos/unsigned-gg/cas-substrate`, both HTTP 404;
  `gh api repos/cerebral-work/cas-substrate`, `private=true`. the operator said in
  session at about 21:35Z "we're now an unsigned-gg repo" [reported]; the API
  did not show a transfer one minute later. At about 22:45Z, asked again with
  the same probe result, he directed this lane to perform the transfer (D1, amended).
- **F6 [verified]:** The crates.io name `uas` was published at version 0.1.0 on
  2026-10-10T20:46Z as "Unsigned Agent Substrate (UAS) core primitives", owned by
  the user `todie`. Command: `curl https://crates.io/api/v1/crates/uas` and
  `.../uas/owners`. Spec 0004 records `publish = false` for estate crates; this
  placeholder is outside that rule and is the operator's own act.
- **F7 [verified]:** CI runs on self-hosted runners addressed by the runner
  group `ent-default` (spec 0005). Whether that group belongs to the
  `cerebral-work` organisation or to an enterprise could not be read: both
  `orgs/<org>/actions/runner-groups` routes return 403 for this token. Command:
  `gh api orgs/cerebral-work/actions/runner-groups`.
- **F8 [verified]:** Five other checkouts of this repository exist on ceres and
  belong to other lanes: the shared checkout on `chore/lesson-table-cells` and
  worktrees on `docs/spec-0019-d2-reopened`, `docs/spec-0027-pool-harness-runner`,
  `fix/cer-2800-pin-check` and a detached `cas-wave-live`. Command: `git worktree list`.
- **F9 [verified]:** `release-plz.toml` tags releases from the `cas-cli` package
  as `v{{ version }}`, and the specs cite 52 distinct `CER-` tickets. Commands:
  `grep -n name release-plz.toml`; `grep -rhoE 'CER-[0-9]{4}' specs/ | sort -u | wc -l`.

## Design

### Decisions

| | Decision | Source |
|---|---|---|
| D1 | The transfer of the repository to `unsigned-gg`, the rename to `uas`, and any change of visibility from private to public are **the operator's acts**. Amended 2026-10-10 about 22:45Z: the operator, in lane w25:p1, directed the lane to perform the transfer and the rename itself and to dispatch workers for the rename; the lane performs those two steps on that instruction and records the API call it made. Visibility and crates.io publication stay his own acts. Every estate SOP already lists org transfers and visibility changes as operator-gated; his instruction is the gate's sign-off. | revenant-bridge-authority SOP, the floor; CAS lane w25:p1, 2026-10-10; the operator's instruction in lane w25:p1, 2026-10-10 about 22:45Z |
| D2 | The move is one cutover, not a drift. Before it: the hold stands, every lane commits and parks (F8), and the preparation PRs of this spec land on `development`. Then the operator transfers and renames. Then one post-cutover PR fixes what only the new name can fix. Development resumes only when he lifts the hold. **Proposed, not ruled:** where this cutover sits relative to the `soma` to `vespertin` move is undecided and is the operator's call [reported: research lane w1X:p22, 2026-10-10 about 21:5xZ, "not decided yet"]. | CAS lane w25:p1, 2026-10-10 |
| D3 | The crate prefix changes from `cas-` to `uas-` and the binary from `cas` to `uas`, in the same cutover, by `make new-repo NAME=uas ORG=unsigned-gg`. That target must first be fixed to rename every `cas-*` crate directory (F2, CER-2813), and `make template-check` must pass on `development` before the cutover is attempted. A rename tool that is proven broken is not used on the real repository. | spec 0001; CER-2813; CAS lane w25:p1, 2026-10-10 |
| D4 | Spec numbers, spec file names and their `REQ-NNNN-NN` ids do not change. A spec's prose that says `cas-pool` becomes `uas-pool` in the post-cutover PR; a requirement id is a stable reference and stays. The registry keeps its order. | CAS lane w25:p1, 2026-10-10 |
| D5 | History is preserved. The transfer keeps every commit, tag and PR number. No history rewrite, no re-initialisation. | AGENTS.md rule 10 and the NEVER floor; CAS lane w25:p1, 2026-10-10 |
| D6 | Settings are carried by export and re-apply, not by memory: branch protection on `development` and `main`, the merge-commit-only setting, delete-branch-on-merge, CODEOWNERS, the 14 labels, and the Actions configuration. Each is exported to a file before the cutover and compared after. | F3; CAS lane w25:p1, 2026-10-10 |
| D7 | The organisation secrets behind sccache (F4) and the runner group `ent-default` (F7) are the two things most likely to break CI on the other side. Both are resolved before the cutover, by the operator or the Director of Engineering, who hold organisation admin: either `unsigned-gg` gains equivalent organisation secrets and runner access, or CI is rewritten to need neither. A red CI after the transfer is a stop, not something to work around. | spec 0005; second-operator SOP section 3; CAS lane w25:p1, 2026-10-10 |
| D8 | `release-plz` continues to tag from the renamed CLI package as `v{{ version }}`, and the version does not reset. The first release after the cutover is a minor bump whose changelog names the rename. | F9; spec 0003; CAS lane w25:p1, 2026-10-10 |
| D9 | Linear stays as it is. The 52 `CER-` references in specs remain valid identifiers; the Linear project is renamed if the operator wants, which is his act. | F9; CAS lane w25:p1, 2026-10-10 |
| D10 | Other lanes' checkouts (F8) are not touched by this lane. After the transfer, GitHub redirects the old remote, so an existing checkout keeps working until its owner updates `origin`. The cutover notice tells each owner to run `git remote set-url origin` and to move the directory under `<home>/projects/unsigned/` when they choose. | post-merge-branch-hygiene SOP, concurrency; CAS lane w25:p1, 2026-10-10 |
| D11 | The estate brief, the agentic context package, memory files and the `.saga` team files name this repo. Those outside this repository are not this spec's to edit; the cutover notice lists them for their owners. Those inside, `.saga/teams.toml` and `.saga/topologies.toml`, are part of the post-cutover PR. | CAS lane w25:p1, 2026-10-10 |
| D12 | Facts F1 to F9 are the **transfer impact list** for `uas` that the implementation plan's step 7 waits on, and they are published as `docs/migration/uas-impact-list.md` in the preparation phase, so the `vespertin` lane and the operator read one file rather than this spec [reported: research lane w1X:p22 confirmed the list is "exactly what is needed", 2026-10-10 about 21:5xZ]. | CAS lane w25:p1, 2026-10-10 |
| D13 | **Publication.** the operator ruled, in his own session in lane w25:p1 at about 21:38Z on 2026-10-10, that the `uas` crate on crates.io, which he published and owns (F6), is the vehicle for publishing substrate work. This amends, for this repository only, spec 0004's standing `publish = false` for estate crates. crates.io has no namespaces: `uas` is one crate name, and any `uas-*` sub-crate is a separate name that is unreserved until published, with a 24-hour lock after any deletion (as `vesperan` and `zerodays` showed today). Which crates publish, under which names, and who presses publish, are spec 0039's to record; publishing itself stays the operator's act. | the operator, 2026-10-10 about 21:38Z, in session [reported: this lane heard it] |

### Requirements (EARS)

- **[REQ-0037-01]** (Ubiquitous): The migration shall preserve every commit, tag, branch and pull request number of the repository.
- **[REQ-0037-02]** (Ubiquitous): The migration shall keep every spec number, spec file name and `REQ-NNNN-NN` identifier unchanged.
- **[REQ-0037-03]** (Ubiquitous): The repository shall carry across the move its branch protection on `development` and `main`, its merge-commit-only setting, delete-branch-on-merge, its CODEOWNERS file and its labels.
- **[REQ-0037-04]** (Event-driven): WHEN the preparation phase begins, the repository shall export each setting in REQ-0037-03 to a file committed under `docs/migration/`, so that the post-cutover state can be compared against it.
- **[REQ-0037-05]** (Event-driven): WHEN `make new-repo NAME=uas ORG=unsigned-gg` runs, it shall rename every directory under `crates/` that carries the `cas-` prefix and every tracked file whose own name carries it, rewrite the `cas` token on word boundaries in every tracked text file outside `vendor/`, `Cargo.lock`, `specs/` and the `new-repo` skill itself, leave every reference to a spec file name unchanged, and leave the workspace building.
- **[REQ-0037-06]** (Event-driven): WHEN the cutover is complete, the repository shall build, and `make check`, `make hygiene` and `make smoke` shall pass on the renamed workspace with `JOBS=2`.
- **[REQ-0037-07]** (State-driven): WHILE the development hold stands, no lane shall land anything in this repository except the preparation and post-cutover pull requests of this spec.
- **[REQ-0037-08]** (State-driven): WHILE the organisation secrets behind sccache and the runner group have not been resolved for `unsigned-gg`, the cutover shall not begin.
- **[REQ-0037-09]** (Unwanted): IF `make template-check` fails on `development`, THEN the cutover shall not begin.
- **[REQ-0037-10]** (Unwanted): IF CI is red on the first push after the transfer, THEN the lane shall stop and report, and shall not merge around it.
- **[REQ-0037-11]** (Unwanted): IF a post-cutover setting differs from its exported file, THEN the difference shall be reported before any further pull request merges.
- **[REQ-0037-12]** (Unwanted): IF any step would change the repository's visibility, publish to crates.io, or rewrite history, THEN a lane shall stop and leave that step to the operator; the transfer and rename run only on his recorded instruction (D1, amended).
- **[REQ-0037-13]** (Optional): WHERE GitHub's redirect from the old repository name is in place, a lane's existing checkout shall keep working until its owner updates the remote, and no lane shall update another lane's checkout.

## Plan

Preparation, on `development`, before the transfer:

- [ ] Fix `make new-repo` to rename every `cas-*` crate directory, and make `make template-check` pass on `development` (CER-2813). Red first: the current failure is the control.
- [ ] Export the settings of REQ-0037-03 to `docs/migration/settings-before.*`, names only for anything secret-adjacent.
- [ ] Publish F1 to F9 as `docs/migration/uas-impact-list.md`, the step-7 input for the `vespertin` lane and the operator (D12).
- [ ] Rewrite `.github/workflows/_rust.yml` so the sccache step degrades to a warning when its secrets are absent, so a missing organisation secret does not fail the build on the other side.
- [ ] Add a `docs/migration/cutover.md` runbook from this spec: the order, the owner of each step, the rollback.
- [ ] Open a Linear ticket under CER for the cutover with the operator as owner of D1 steps and the two D7 items.

Cutover, by the operator:

- [ ] Resolve D7: organisation secrets and runner access on `unsigned-gg`, or CI that needs neither.
- [ ] Transfer `cerebral-work/cas-substrate` to `unsigned-gg` and rename it `uas`.
- [ ] Decide visibility. The plan says public; that is a separate act with its own blast radius, and this spec does not presume it. Confirmed undecided as of 2026-10-10 about 21:5xZ [reported: research lane w1X:p22].
- [ ] Decide the order of the `uas` cutover relative to the `vespertin` move (D2). Undecided as of the same time.

Post-cutover, one pull request:

- [ ] Run `make new-repo NAME=uas ORG=unsigned-gg` and commit the result.
- [ ] Rewrite spec prose from `cas-` to `uas-` crate names, without touching spec numbers, file names or requirement ids.
- [ ] Verify `.saga/teams.toml`, `.saga/topologies.toml`, `README.md` and `AGENTS.md` after `new-repo`, which rewrites them; fix by hand only what it missed.
- [ ] Compare live settings against `docs/migration/settings-before.*` and record the result.
- [ ] Send the cutover notice to every lane with a checkout (F8): the new remote, and that updating it is theirs to do.
- [ ] the operator lifts the hold.

## Test

```bash
make template-check JOBS=2 && \
  make check JOBS=2 && make hygiene JOBS=2 && make smoke JOBS=2 && \
  diff <(gh api repos/unsigned-gg/uas --jq '{m:.allow_merge_commit,s:.allow_squash_merge,r:.allow_rebase_merge,d:.delete_branch_on_merge}') docs/migration/settings-before.merge.json
```

A pass requires each of these to exist and to go red when its own guard is removed:

| Case | Where | Requirement |
|---|---|---|
| `template-check` fails when any `cas-*` crate directory is left unrenamed | Makefile, on a copy with one directory restored | REQ-0037-05, REQ-0037-09 |
| the settings comparison fails when squash merging is enabled on the new repository | the `diff` leg | REQ-0037-03, REQ-0037-11 |
| `make smoke` fails when the renamed domain crate gains an I/O import | `cas-domain`, as spec 0001 already proves | REQ-0037-06 |
| the sccache step warns rather than fails when its secret is unset | `_rust.yml`, with the secret name blanked | D7 |

Today's state is the primary control: `make template-check` fails on
`development` (F2). When this spec is implemented, the same command passes.
Until it does, the cutover does not begin (REQ-0037-09).

## Notes

- **The plan's approval, as reported.** Asked who approved it, the author pane
  w1X:p22 answered at about 21:3xZ that the operator approved the artifact himself,
  through the agy artifact approval in that pane, at about 21:12Z, and that the
  file is an uncommitted artifact in that pane's brain directory [reported:
  mesh reply from session-agy-ceres-estate-w1x-p22]. An artifact approval in his
  own pane is his act under the override SOP's section 4. It is still `reported`
  here because this lane did not see it. The author also named a second,
  uncommitted document, `implementation-plan.md` in the same directory, covering
  the `soma` to `vespertin` and `cas-substrate` to `uas` migrations; this spec is
  read against it in Notes below.
- **Alignment with the implementation plan.** The author's `implementation-plan.md`
  (brain file, updated 21:11Z) [reported] orders the work as: scaffold the three
  `*-k8s-*` crates in the paas repository, add the `SagaFlow` and `SagaRun` CRDs,
  rename `soma` to `vespertin`, add path dependencies in the `vespertin` operator,
  refactor, test, then "await transfer impact list from the vespertin lane before
  finalizing physical Git migrations". It does not place the `uas` transfer in
  that order, and its two open questions, where `cerebral-k8s-api` lives and the
  `vespertin` operator's new path, are open. Seven questions went to the research
  lane at about 21:4xZ on the operator's instruction, including where `uas` sits in
  that order and whether F1 to F9 of this spec are the "transfer impact list"
  step 7 wants. Until answered, D2's single cutover is this lane's proposal, and
  REQ-0037-08 holds the cutover anyway.
- **What this spec does not decide.** The `uas-operator` and its CRDs are spec
  0038. The k8s dependency set, its pins and the `k3d` CI job are spec 0039. The
  `soma` move to `cerebral-work` and the `unsigned-k8s-*` crates are other
  repositories' specs. Public visibility is the operator's decision, recorded here
  as a step, not presumed.
- **Why `new-repo` and not a hand rename.** The target already exists, it is the
  repository's own rename tool, and it is proven broken by a test the repository
  already has. Fixing the tool and watching its test go green is the same work
  as a hand rename, with a control attached.
- **Why the version does not reset.** Tags are history (D5), release-plz reads
  them, and a reset would make the first `uas` release look older than the last
  `cas` one.
- Reading: the plan at `<surfaces>/cas-ears/kube-rs-plan.md`;
  CER-2813; spec 0003 for the branch model the move preserves; spec 0005 for the
  runners; `<home>/.agents/sops/revenant-bridge-authority.md` for the floor that puts
  transfers and visibility with the operator.
