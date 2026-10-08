---
status: in-progress
created: 2026-10-08
tags: [git, branches, protection, promotion]
priority: high
depends_on: [0002-verification-gates-and-evidence]
---

# 0003-branch-model-and-promotion

## Overview

This spec sets the branch model. Work lands on `development`. Releases come
from `main`. A promotion PR moves work from one to the other.

Evidence from 2026-10-08: a PR opened by the default `GITHUB_TOKEN` does not
trigger CI. Any automation that opens a PR must use a normal PR flow or a
token that triggers workflows. Spec 0005 uses a normal PR for the version bump.

The model copies cortex spec 0008. Cortex lefthook diffed `origin/main` while
spec 0008 branched from `development`. That mismatch is drift. This template
diffs `origin/development` everywhere.

## Design

### Branches

| Branch | Role |
|---|---|
| `development` | Integration branch. Feature branches start here. PRs target here. |
| `main` | Release branch. Only promotion PRs target it. |

### Rules

- Merge with merge commits only. No squash. No rebase merge. Merge commits keep
  the signed commits intact.
- Commits are signed. Subjects follow Conventional Commits.
- Nobody pushes directly to `main` or `development`. Every change uses a PR.
- The repo setting `delete_branch_on_merge` is on. Squash merge is off.

### Protection table

Both branches carry the same classic protection.

| Setting | `development` | `main` |
|---|---|---|
| Deletion blocked | yes | yes |
| Force-push blocked | yes | yes |
| PR required, admins included | yes | yes |
| Required approvals | 0 | 0 |
| Required check `secrets` | yes | yes |
| Required check `rust` | yes | yes |
| Required check `cargo-deny` | yes | yes |
| Required check `hygiene` | yes | yes |

The template repo itself also requires `template-check` on both branches. A
repo made from the template does not, because `make new-repo` deletes that
workflow.

The approval count is 0 on purpose. Estate rule "prod is dev" (operator ruling
2026-09-30) lets a lane merge on green CI. Review can follow the merge.
The check names are confirmed after the first CI run. Spec 0005 lists the jobs.

### Promotion PR

A promotion PR merges `development` into `main`.

- The operator decides. Christian Todie merges it. This is his go, per
  decision of record, interview 2026-10-08.
- No agent approves or merges its own promotion PR.
- After the merge, `development` and `main` must point to the same content.
  Check it with `git diff origin/development origin/main`. It must print nothing.

## Plan

- [ ] Create the repo with `main` and `development`.
- [ ] Turn on `delete_branch_on_merge`. Turn off squash and rebase merge.
- [ ] Write the protection for both branches.
- [ ] Add the four required checks after the first green run.
- [ ] Open the first promotion PR. The operator merges it.

## Test

Read the protection back from the API. Do not trust the call that wrote it.

```bash
gh api repos/cerebral-work/cas-substrate/branches/main/protection \
  --jq '{force: .allow_force_pushes.enabled, del: .allow_deletions.enabled, admins: .enforce_admins.enabled, checks: .required_status_checks.contexts}'
```

Passing means `force` and `del` are false, `admins` is true, and `checks`
lists `secrets`, `rust`, `cargo-deny` and `hygiene`. Repeat for `development`.
A push to `main` from a throwaway clone must be refused.

## Notes

Alternatives rejected:

- Squash merge. It removes the commit signatures.
- Trunk only. It gives no separate release branch to protect.

Open question: add a merge queue. The agentic repos surveyed on 2026-10-08 use
plain protected branches. No queue is needed at this size.

References:

- `cerebral-work/cortex@524a97b:specs/0008-branch-model-and-promotion.md`
