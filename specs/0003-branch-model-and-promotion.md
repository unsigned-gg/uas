---
status: in-progress
created: 2026-10-08
tags: [git, branches, protection, github-flow]
priority: high
depends_on: [0002-verification-gates-and-evidence]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0003-branch-model-and-promotion

## Overview

This spec sets the branch model. `main` is the only integration and release
branch (GitHub Flow). Feature branches are cut from `main`, a PR into `main`
merges on green CI, and releases come from `main`.

History: until the last promotion, work landed on `development` and a
promotion PR moved it to `main`. The file name keeps the old title so links
stay valid.

Evidence from 2026-10-08: a PR opened by the default `GITHUB_TOKEN` does not
trigger CI. Any automation that opens a PR must use a normal PR flow or a
token that triggers workflows. Spec 0005 uses a normal PR for the version bump.

## Design

### Decisions

| ID | Decision | Source |
|---|---|---|
| D1 | `main` is the only integration and release branch. Feature branches are cut from `main` and PRs target `main`. Finish GitHub Flow: update AGENTS.md rules 4 and 9, specs 0003, 0005 and 0054, fix the chart RBAC, then make one last promotion of `development` to `main`. After that promotion `main` is the base. | operator ruling 2026-10-18 (uas lane decision UAS-D1): Finish GitHub Flow |
| D2 | `development` is retired after the last promotion. It keeps its protection until the operator decides what happens to it. Nothing in this spec proposes deleting it. | operator ruling 2026-10-18 (UAS-D1) |
| D3 | The earlier model (work on `development`, a promotion PR to `main`, land-on-green for the promotion PR, ruled 2026-10-08) ran until the last promotion. | history |

### Requirements (EARS)

- **Ubiquitous:** The repository shall enforce PR-only integration with signed commits and merge commits without squashing.
- **Ubiquitous:** The repository branch protections shall block force-pushes and deletions on `main`.
- **Event-driven:** WHEN merging a PR into `main`, the lane shall pin the head commit with `--match-head-commit` and verify green CI.
- **Event-driven:** WHEN commits land on `main`, the release automation shall execute the `release` workflow.
- **Unwanted behavior:** IF a push targets `main` directly, THEN repository guards and branch protections shall reject the push.

### Branches

| Branch | Role |
|---|---|
| `main` | The only integration and release branch. Feature branches start here. PRs target here. |
| `development` | Retired after the last promotion. Keeps its protection until the operator decides. |

### Rules

- Merge with merge commits only. No squash. No rebase merge. Merge commits keep
  the signed commits intact.
- Commits are signed. Subjects follow the Conventional Commits regex in spec 0051
  (D4); Merge and Revert subjects stay allowed. Amended 2026-10-11 (plan 0051 P6).
- Nobody pushes directly to `main`. Every change uses a PR.
- The repo setting `delete_branch_on_merge` is on. Squash merge is off.

### Protection table

State as of 2026-10-18. Both branches carry the same classic protection except
for the last two checks.

| Setting | `main` | `development` (retired after the last promotion) |
|---|---|---|
| Deletion blocked | yes | yes |
| Force-push blocked | yes | yes |
| PR required, admins included | yes | yes |
| Required approvals | 0 | 0 |
| Required check `secrets` | yes | yes |
| Required check `rust / cargo` | yes | yes |
| Required check `cargo-deny` | yes | yes |
| Required check `hygiene` | yes | yes |
| Required check `ebpf-live` | no. Adding it is a separate operator go, pending. | yes (2026-10-10, after development run 38028975897; spec 0005 D2) |
| Required check `fork-gate` | no. Adding it is a separate operator go, pending. | yes (2026-10-11, the operator, "Add fork-gate to the required checks now", lane w25:p1 about 05:25Z; plan 0051 P7) |

`main` requires four checks today: `secrets`, `rust / cargo`, `cargo-deny` and
`hygiene`. The target on `main` is the six that `development` carries. Spec
0054 D7 and REQ-0054-11 name the target.

The template repo itself also requires `template-check` on both branches. A
repo made from the template does not, because `make new-repo` deletes that
workflow.

The approval count is 0 on purpose. Estate rule "prod is dev" (operator ruling
2026-09-30) lets a lane merge on green CI. Review can follow the merge.
The check names were read back from the API on 2026-10-08. The `rust` job
reports as `rust / cargo`. Spec 0005 lists the jobs.

### Merging a PR into main

- Land-on-green applies. A lane merges its own PR when every required check is
  green on the head. Use a merge commit pinned to the head:
  `gh pr merge <N> --merge --match-head-commit <sha>`.
- No agent approves its own PR. The protection needs 0 approvals.
- On ceres, the user hook `guard-merge-to-main.sh` blocks every
  `gh pr merge`. The lane adds the `# allow-merge` marker, cites the land-on-green
  ruling and reports the merge.
- The push to `main` starts the `release` workflow. Check that the run is
  green. A red run is a stop, not a detail.

## Plan

- [x] Create the repo with `main` and `development`.
- [x] Turn on `delete_branch_on_merge`. Turn off squash and rebase merge.
- [x] Write the protection for both branches.
- [x] Add the four required checks after the first green run.
- [x] Open the first promotion PR and merge it (#4, merged 2026-10-08).
- [ ] Move governance text and specs to GitHub Flow (CER-2986 part 1).
- [ ] Make the last promotion of `development` to `main`. Then `main` is the base.
- [ ] Operator go: add `ebpf-live` and `fork-gate` to the `main` required checks.
- [ ] Operator decision: what happens to `development`.

## Test

Read the protection back from the API. Do not trust the call that wrote it.

```bash
gh api repos/unsigned-gg/uas/branches/main/protection \
  --jq '{force: .allow_force_pushes.enabled, del: .allow_deletions.enabled, admins: .enforce_admins.enabled, checks: .required_status_checks.contexts}'
```

Passing means `force` and `del` are false, `admins` is true, and `checks`
lists `secrets`, `rust / cargo`, `cargo-deny` and `hygiene` (the four of 2026-10-18;
`ebpf-live` and `fork-gate` join them after the operator go).
A push to `main` from a throwaway clone must be refused.

## Notes

Alternatives rejected:

- Squash merge. It removes the commit signatures.
- Trunk only. It was rejected at first. Reversed by the operator on 2026-10-18 (D1): `main` is now the only branch.

Open question: add a merge queue. The agentic repos surveyed on 2026-10-08 use
plain protected branches. No queue is needed at this size.

References:

- `cerebral-work/cortex@524a97b:specs/0008-branch-model-and-promotion.md`
