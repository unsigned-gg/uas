# Task: Update CAS Spec 0001 to adopt DeltaDB and Blackwall custody ({{ticket}})

You work in the cas-substrate Rust workspace. Change only `specs/0001-cas-architecture-and-crate-layout.md` and `CHANGELOG.md`.

## Background

On 2026-10-08, the operator ruled in Linear issue CER-2671 to adopt the estate DeltaDB and Blackwall custody process across estate Rust repositories.
This reverses the earlier draft note in `specs/0001-cas-architecture-and-crate-layout.md` lines 212-218 which stated "Proposed, not adopted".

## The change

1. In `specs/0001-cas-architecture-and-crate-layout.md`:
Under the "Notes" section:
Replace the paragraph:
"Proposed, not adopted: a DeltaDB virtual-worktree integration with Saga,
Blackwall and Soma
(`cerebral-work/research@ddf42c8:agentic-systems/designs/deltadb-virtual-worktree-integration.md`,
draft, no approver). This spec does not adopt it. the operator ruled on 2026-10-08
to record it here only. The cortex lane sent six review findings to the
proposing lane (w1R:p1) the same day. One finding blocks: the design commits
directly to main."

With:
"Adopted with PR-floor constraints: DeltaDB and Blackwall custody process across
estate Rust repositories (CER-2671, operator ruling 2026-10-08). Adopts the
integration design
`cerebral-work/research@ddf42c8:agentic-systems/designs/deltadb-virtual-worktree-integration.md`
with PR-only landing: changesets reconcile to feature branches, open PRs targeting
`development`, and merge on green with merge commits. Direct push to `main` remains
forbidden."

2. In `CHANGELOG.md`:
Add one line under `## [Unreleased]` (`### Changed`):
`- Adopt DeltaDB and Blackwall custody process with PR-floor constraints in spec 0001 (CER-2671).`

## Gates

- `make spec-check` must pass with 0 warnings.
- `make check JOBS=2` and `make hygiene JOBS=2` must pass.
