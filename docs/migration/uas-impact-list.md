<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# uas transfer impact list

What this is: the transfer impact list that the implementation plan's step 7 waits on,
published per spec 0037 D12 so the `vespertin` lane and the operator read one file rather
than the spec. Dated 2026-10-10. The facts below are spec 0037's F1 to F9, measured on
`origin/development` at `ab8112a` unless a fact says otherwise; the spec stays the source
and this file is a copy. The settings export at the end was taken on 2026-10-10 about
22:50Z, after PR #118, from `cerebral-work/cas-substrate` before any transfer.

## Facts

| # | Tier | Fact | Command |
|---|---|---|---|
| F1 | verified | The name is bound in 18 tracked file groups: `crates` 94, `specs` 38, `.claude` 8, `.saga` 6, `tasks` 3, `docs` 2, `xtask` 1, `scripts` 1, `release-plz.toml` 1, `README.md` 1, `Makefile` 1, `.github` 1, `engineering` 1, `.ears-spec` 1, `CHANGELOG.md` 1, `Cargo.toml` 1, `AGENTS.md` 1, `.agents` 1. | `git grep -lI -- 'cas-\|cas_\|\bcas\b' -- ':!vendor' ':!Cargo.lock' \| sed 's\|/.*\|\|' \| sort \| uniq -c` |
| F2 | verified | `make new-repo NAME=<kebab> [ORG=...]` does the rename and `make template-check` proves it; it failed on `development` until PR #117 (CER-2813). | `make template-check JOBS=2` |
| F3 | verified | Private, default branch `main`, merge commits only, delete-branch-on-merge on, 14 labels, no rulesets, no repository Actions variables or secrets. | `gh api repos/cerebral-work/cas-substrate` and its `rulesets`, `actions/variables`, `actions/secrets`, `labels` routes |
| F4 | verified | `.github/workflows/_rust.yml` uses sccache on R2 with organisation secrets of `cerebral-work`, which do not travel with a transfer. The step is gated on `env.SCCACHE_BUCKET != ''`, so a missing secret skips the cache and does not fail the job. | `grep -n sccache .github/workflows/_rust.yml` |
| F5 | verified | At 21:36Z and again at 22:40Z neither `unsigned-gg/uas` nor `unsigned-gg/cas-substrate` existed and `cerebral-work/cas-substrate` was present and private. | `gh api repos/unsigned-gg/uas` (404); `gh api repos/cerebral-work/cas-substrate` |
| F6 | verified | crates.io `uas` 0.1.0 was published 2026-10-10T20:46Z by the operator. | `curl https://crates.io/api/v1/crates/uas` |
| F7 | verified | CI runs on the runner group `ent-default` with labels `self-hosted`, `ci-rust`, `ci-kernel`. The group's owner cannot be read with this token (403). The `self-hosted` runner scale set `self-hosted-j75dx` also serves `unsigned-gg/soma` and `unsigned-gg/unsigned-onboard`, so that set is visible to both organisations. | `gh api repos/<repo>/actions/runs/<id>/jobs` on both organisations |
| F8 | verified | Five other checkouts exist on ceres and belong to other lanes: the shared checkout and worktrees `docs/spec-0019-d2-reopened`, `docs/spec-0027-pool-harness-runner`, `fix/cer-2800-pin-check`, detached `cas-wave-live`. | `git worktree list` |
| F9 | verified | `release-plz.toml` tags from the `cas-cli` package as `v{{ version }}`; the specs cite 52 distinct `CER-` tickets. | `grep -n name release-plz.toml`; `grep -rhoE 'CER-[0-9]{4}' specs/ \| sort -u \| wc -l` |

## Settings exported

Taken from `cerebral-work/cas-substrate` before the transfer, names only, no values.

| File | Holds |
|---|---|
| `settings-before.merge.json` | `allow_merge_commit`, `allow_squash_merge`, `allow_rebase_merge`, `delete_branch_on_merge` |
| `settings-before.repo.json` | name, visibility, default branch, issues, projects, wiki, update-branch, auto-merge, signoff |
| `settings-before.protection-development.json` | branch protection of `development`: the five required checks and the admin enforcement |
| `settings-before.protection-main.json` | branch protection of `main` |
| `settings-before.labels.json` | the 14 labels with colour and description |
| `settings-before.actions.json` | the Actions permissions of the repository |
| `settings-before.rulesets.json` | rulesets (none) |
| `settings-before.codeowners.txt` | whether `.github/CODEOWNERS` is present |
