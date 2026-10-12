<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Launch checklist: spec 0048 D14

**Status: drafted; the cut waits on spec 0048 D21**, the grooming pass of `development`
against `main` and the seven green days of `text-check`. The name is ruled: `unsigned-gg/od`
(D17). The form is ruled: the full history of `main`, rewritten by `scripts/public-history.sh`
(D19). Creating the repository is the operator's act. No box below is ticked until the cut. The
repository stays private while any box is open or the operator has not given the go (REQ-0048-17);
`gh api repos/unsigned-gg/uas --jq .private` must print `true` until then (REQ-0048-09).

How to use it: run every item on the exact head that the cut will publish. A red item is a stop,
not a note. Each item names who runs it and the command or read-back that proves it. A box is
ticked by the person named, with the date and the head SHA in the commit that ticks it.
Fix-forward items go through a normal PR into `development`.

Sources: `specs/0048-licence-and-publication.md` D7, D9, D13, D14, D17 to D21 and REQ-0048-09, 10,
17, 20 to 22; `specs/0051-repository-guards.md` D8; `specs/0053-changelog-and-commit-text-policy.md`
D10; `RELEASING.md`.

Dry run of 2026-10-11 on `development` at `4922094`, lane w25:p1 [verified, each command exit 0]:
`make hygiene JOBS=2`, `make oss-check`, `make spdx-check`, `make doc JOBS=2`, `gitleaks git` and
`gitleaks dir` with a planted `ghp_` token as the red control, the fenced spec command and
`make check JOBS=2`. Not run there: the README grep (the purpose text did not exist yet) and the
names leg (it did not exist yet). The rewrite measurements are in spec 0048 Notes. Nothing is
ticked by a dry run.

## A. The tree, before the cut

- [ ] **Hygiene green.** Lane runs `make hygiene JOBS=2` on the head. Proof: exit 0. It covers
  typos, machete, spec-check, toolchain-check, pin-check, oss-check, spdx-check and hook-test.
- [ ] **oss-check green on its own.** Lane runs `make oss-check`. Proof: exit 0 and no output.
  It fails on a tailnet hostname, an operator path, an operator's personal name (D20), a
  removed vendor directory, or a `vendor/` directory with no `LICENSE`.
- [ ] **spdx-check green on its own.** Lane runs `make spdx-check`. Proof: exit 0. Code files
  carry an SPDX line; Markdown under `docs`, `specs` and `engineering` carries the CC-BY-4.0 tag.
- [ ] **Licence files and SPDX complete.** Lane runs the fenced command in spec 0048, Test
  section ("Passing means exit 0 after packages 1 to 4"). Proof: exit 0. Read the whole
  chain; it stops at its first red leg.
- [ ] **Secret scan, tree and history.** Lane runs gitleaks at the version pinned in
  `.github/workflows/ci.yml` and spec 0004, from a full clone (`fetch-depth: 0` equivalent):
  `gitleaks git . --redact --no-banner --verbose` for every reachable commit, and
  `gitleaks dir . --redact --no-banner` for the working tree. Proof: both exit 0. A hit is a
  stop and a rotation: treat the credential as exposed, flag it to the operator for rotation, and
  do not cut until it is rotated and the scan is clean. Positive control: the scan must flag a
  planted fake key in a scratch copy before its clean result counts. Use a random `ghp_` token
  as spec 0055 F11 does; the AWS documentation example key is on gitleaks' own allowlist and
  does not fire (measured 2026-10-11).
- [ ] **Docs build green.** There is no separate docs build: no mdBook or site generator is in
  the tree [verified 2026-10-11: `git ls-files` lists no book.toml, mkdocs or similar]. The API docs are what stands in for it.
  Lane runs `make doc JOBS=2` (cargo doc, workspace, no deps). Proof: exit 0 with no warnings
  printed. The prose docs are covered by `make spec-check` and `make hygiene` above.
  [asserted: no spec names a docs build command; this item names it, per plan 0048 P17.]
- [ ] **Full gate.** Lane runs `make check JOBS=2` on the head. Proof: exit 0.
- [ ] **CI green on the exact head.** Lane runs `gh pr checks <N>` or
  `gh run list --branch main --limit 1`. Proof: every required check is SUCCESS on that SHA.
- [ ] **README says public.** Lane changes the README status line from "Private until the spec
  0048 cut" to the public form, in the last PR before the promotion. Proof:
  `grep -c 'Private until' README.md` prints 0 and `grep -c -i -E 'trace corpus|swarm orchestration|reconstruction' README.md`
  prints at least 3 (REQ-0048-20).

## B. Required checks on the repository

- [ ] **Seven required checks on `development`.** Lane runs
  `gh api repos/unsigned-gg/uas/branches/development/protection --jq .required_status_checks.contexts`.
  Proof: it lists `secrets`, `rust / cargo`, `cargo-deny`, `hygiene`, `ebpf-live`, `fork-gate`
  and `text-check` (spec 0053 D10 counts seven).
- [x] **fork-gate is required** (done for `development` 2026-10-11, protection read back; `main` at the cut)**.** Plan 0051 P7, the operator's act. Proof: `fork-gate` is in the
  list above, and `gh api repos/unsigned-gg/uas/labels/fork-ci-approved` returns 200. Until this
  is ticked, the fork-PR hole of spec 0051 D8 is open and the cut waits.
- [ ] **text-check is required after its seven days.** Lane checks that `text-check` ran
  green for seven days with no false failure (spec 0053 D10), then adds it to the required
  checks. Proof: it is in the list above, and the date of its first run is at least seven days
  before the cut (`gh run list --workflow ci.yml --limit 100`, read the oldest `text-check`).
  First run 2026-10-11T04:56Z (read 2026-10-11), so 2026-10-18 at the earliest (D21).
- [ ] **The grooming pass is finished (D21).** The review of `development` against `main` is
  worked off and the promotion PR is merged (spec 0003). Proof: `git rev-list --count origin/main..origin/development`
  prints 0 and the `release` run on `main` is green.

## C. The public repository `unsigned-gg/od` (spec 0048 D17, D19, REQ-0048-10)

The operator creates the repository; the lane reads back each check before the push and again on
the new repository after its first push.

- [ ] **The persona manifest of record is current.** `engineering/plans/0048-personas.toml`
  (spec 0048 D19, the elf conclave). Proof: `[rewrite] emails` holds every operator address that
  authored a commit on `main` (`git log --format=%ae main | sort -u`, minus the `[keep]` list),
  and `[keep] emails` holds only identities published on purpose (dependabot, GitHub's merge
  identity, an outside contributor who agreed).
- [ ] **The rewrite is green.** Lane runs
  `scripts/public-history.sh <this repository> <dir> <manifest> main`. Proof: exit 0 and the
  last line `public-history: ok`, after the author table, the vendor leg, the pattern leg over
  every blob and message, the file-list leg and the gitleaks leg. Red control: a manifest whose
  `[rewrite]` omits an operator address must exit 1 naming the surviving identity.
- [ ] **`make check` and `make hygiene` green on the rewritten head.** Lane runs `make check JOBS=2`
  and `make hygiene JOBS=2` inside `<dir>`. Proof: both exit 0. The hygiene run proves that the
  rewrite left the Makefile legs and the script intact (spec 0048 REQ-0048-21, 22).
- [ ] **The repository is created with its purpose stated** (D18, REQ-0048-20). The operator runs
  `gh repo create unsigned-gg/od --public --description "od (overdrive) - Unsigned Agent Substrate, a Rust-native pooled runtime for agent harnesses, built by agent swarms as a trace corpus of agents implementing complicated Rust and a testbed for agentic swarm orchestration in the SDLC"`
  and sets the topics `rust`, `agents`, `agentic-sdlc`, `trace-corpus`, `swarm-orchestration`. Proof:
  `gh repo view unsigned-gg/od --json description,repositoryTopics` holds "trace corpus"
  and the topics.
- [ ] **The rewritten history is pushed.** The operator runs
  `git -C <dir> push git@github.com:unsigned-gg/od.git main` and sets `main` as the
  default branch. Proof: `git ls-remote git@github.com:unsigned-gg/od.git main` prints the
  head of `<dir>`, and `git -C <fresh clone> rev-list --count main` equals the count the
  rewrite printed.
- [ ] **Branch protection on the public `main` and `development`.** The operator mirrors the
  required checks of this repository and keeps force-push off. Proof: the protection read-back
  lists the same contexts and `allow_force_pushes.enabled` is `false`.
- [ ] **DCO is a required check on the new repository.** Lane opens a scratch pull request
  with one commit that lacks a `Signed-off-by:` trailer. Proof: the DCO check fails it; then
  a signed-off commit passes (positive control). Close the scratch pull request.
- [ ] **The self-hosted runner group admits the public repository.** CI runs on the runner group
  `ent-default` (`.github/workflows/ci.yml`). The operator confirms in the enterprise or
  organisation runner settings that the group allows public repositories; a lane cannot read it
  (`gh api orgs/unsigned-gg/actions/runner-groups` returned 403 on 2026-10-11). Proof: the first
  push's `rust / cargo` run reaches a runner instead of queueing. The fork guards of spec 0051
  stay.
- [ ] **Private vulnerability reporting is on** (spec 0045 D3). The operator turns it on.
  Proof: `gh api repos/unsigned-gg/od/private-vulnerability-reporting --jq .enabled` prints
  `true`.
- [ ] **The crates.io `uas` crate points at a URL that opens** (REQ-0048-12). The operator
  publishes per D6. Proof: `curl -s -o /dev/null -w '%{http_code}' https://github.com/unsigned-gg/od`
  prints 200, with a known-good URL as the control.
- [ ] **This repository stayed private until the go.** Lane runs
  `gh api repos/unsigned-gg/uas --jq .private`. Proof: it printed `true` through every item above
  (REQ-0048-09). Any `false` before the go is a stop.

## D. Names and the go

- [ ] **Operator names are out of the tree (spec 0048 D20).** The operator ruled on 2026-10-11 in
  lane w25:p1 that personal names in attributions become roles; that ruling is the sign-off.
  Lane proves it on the head: `make oss-check` exits 0 with its names leg, and the D20 grep
  prints nothing outside the files D20 keeps.

- [ ] **The unix identifiers are ruled (spec 0048 D20).** `ctodie` and `ctodie@cerebral.work` remain
  in `crates/uas-domain/src/landing.rs` test fixtures, `crates/uas-ebpf/src/daemon.rs`,
  `crates/uas-ebpf/uas-ebpfd.service` line 11 and spec 0014. They are not names and no leg sweeps
  them, at the head or in history. The operator rules whether they stay or become neutral values
  before the cut. A ruling for neutral values needs a rule in `scripts/public-history.sh` that maps
  them in every blob and message, and a history leg that goes red on any survivor. Proof: the
  ruling in the commit that ticks this box; at the head, `git grep -n -E 'ctodie' -- crates specs`
  matches the ruling; in the rewritten history, `git -C <dir> grep -l -E 'ctodie' $(git -C <dir> rev-list --all)`
  matches the ruling too.

## E. Soft launch

- [ ] **No announcement until docs and CI are green on the public repository** (D14,
  REQ-0048-17). Lane runs `gh run list --repo unsigned-gg/od --limit 5` and `make doc JOBS=2`
  in a fresh clone. Proof: every required check on the public head is SUCCESS and the docs
  build exits 0. Until then the repository is public but quiet.
- [ ] **The announcement is a parked decision.** Its text and timing are the operator's call
  after this checklist. It is not in this file and no lane drafts or sends it.

## F. The go

- [ ] **The operator's go.** Only the operator ticks this box, in their own session, after every
  box above is ticked.
