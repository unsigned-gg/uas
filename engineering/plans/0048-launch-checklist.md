<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Launch checklist: spec 0048 D14

**Status: drafted. Execution is BLOCKED on plan 0048 P13**, the public form: the cut date and
the public repository name are not ruled, and creating the repository is the operator's act.
No box below is ticked until the cut. The repository stays private while any box is open or
the operator has not given his go (REQ-0048-17); `gh api repos/unsigned-gg/uas --jq .private`
must print `true` until then (REQ-0048-09).

How to use it: run every item on the exact head that the cut will publish. A red item is a stop,
not a note. Each item names who runs it and the command or read-back that proves it. A box is
ticked by the person named, with the date and the head SHA in the commit that ticks it.
Fix-forward items go through a normal PR into `development`.

Sources: `specs/0048-licence-and-publication.md` D7, D9, D13, D14 and REQ-0048-09, 10, 17;
`specs/0051-repository-guards.md` D8; `specs/0053-changelog-and-commit-text-policy.md` D10;
`RELEASING.md`.

## A. The tree, before the cut

- [ ] **Hygiene green.** Lane runs `make hygiene JOBS=2` on the head. Proof: exit 0. It covers
  typos, machete, spec-check, toolchain-check, pin-check, oss-check, spdx-check and hook-test.
- [ ] **oss-check green on its own.** Lane runs `make oss-check`. Proof: exit 0 and no output.
  It fails on a tailnet hostname, an operator path, a removed vendor directory, or a
  `vendor/` directory with no `LICENSE`.
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
  planted fake key in a scratch copy before its clean result counts.
- [ ] **Docs build green.** There is no separate docs build: no mdBook or site generator is in
  the tree [verified 2026-10-11: `git ls-files` lists no book.toml, mkdocs or similar]. The API docs are what stands in for it.
  Lane runs `make doc JOBS=2` (cargo doc, workspace, no deps). Proof: exit 0 with no warnings
  printed. The prose docs are covered by `make spec-check` and `make hygiene` above.
  [asserted: no spec names a docs build command; this item names it, per plan 0048 P17.]
- [ ] **Full gate.** Lane runs `make check JOBS=2` on the head. Proof: exit 0.
- [ ] **CI green on the exact head.** Lane runs `gh pr checks <N>` or
  `gh run list --branch main --limit 1`. Proof: every required check is SUCCESS on that SHA.

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

## C. The fresh public repository (spec 0048 D9, REQ-0048-10)

the operator creates the repository; the checks below are read back by the lane before the flip
or the cut and again on the new repository after its first push.

- [ ] **One commit, the tree of `main`.** Before the push, lane compares trees:
  `git rev-parse main^{tree}` equals `git rev-parse <cut-commit>^{tree}`. After the push,
  `git rev-list --count HEAD` on a clone of the new repository prints 1.
- [ ] **`make check` green on that commit.** Lane runs `make check JOBS=2` in the fresh clone.
  Proof: exit 0.
- [ ] **DCO is a required check on the new repository.** Lane opens a scratch pull request
  with one commit that lacks a `Signed-off-by:` trailer. Proof: the DCO check fails it; then
  a signed-off commit passes (positive control). Close the scratch pull request.
- [ ] **Private vulnerability reporting is on** (spec 0045 D3). the operator turns it on.
  Proof: `gh api repos/<owner>/<name>/private-vulnerability-reporting --jq .enabled` prints
  `true`.
- [ ] **The crates.io `uas` crate points at a URL that opens** (REQ-0048-12). the operator
  publishes per D6. Proof: `curl -s -o /dev/null -w '%{http_code}' <repository URL>` prints
  200, with a known-good URL as the control.
- [ ] **This repository stayed private until the go.** Lane runs
  `gh api repos/unsigned-gg/uas --jq .private`. Proof: it printed `true` through every item above
  (REQ-0048-09). Any `false` before the go is a stop.

## D. Names and the go

- [ ] **Sign-off on operator names in attributions (spec 0048 D7).** the operator reads the
  decision attributions in `specs/` and `engineering/` that name an operator, and either keeps
  them or has them removed. Proof: his explicit reply in his own session, recorded in the
  commit that ticks this box. A relay from another lane is not a sign-off. The names stay by
  default (spec 0048 Notes); this item is the cut-time confirmation, not a new gate on the tree.

## E. Soft launch

- [ ] **No announcement until docs and CI are green on the public repository** (D14,
  REQ-0048-17). Lane runs `gh run list --repo <owner>/<name> --limit 5` and `make doc JOBS=2`
  in a fresh clone. Proof: every required check on the public head is SUCCESS and the docs
  build exits 0. Until then the repository is public but quiet.
- [ ] **The announcement is a parked decision.** Its text and timing are the operator's call after
  this checklist. It is not in this file and no lane drafts or sends it.

## F. The go

- [ ] **the operator's go.** Only the operator ticks this box, in his own session, after every box
  above is ticked.
