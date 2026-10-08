---
status: in-progress
created: 2026-10-08
tags: [ci, cache, runners, release, codeql]
priority: high
depends_on: [0002-verification-gates-and-evidence, 0003-branch-model-and-promotion]
---

# 0005-ci-cache-and-runners

## Overview

This spec sets the CI runners, the build cache, the pinning rules and the
release flow.

Measured 2026-10-08:

- The `ci-rust` image `ci-rust-runner:v0.1.6` bakes Rust 1.94.1. Its rustup
  directory is read-only to the runner user. CI must verify the baked
  toolchain. CI must never install one.
- Code Security is disabled on `cerebral-work` private repos. The code scanning
  API returned 403.
- release-plz skips crates with `publish = false` unless `git_only = true`.
- A PR opened by `GITHUB_TOKEN` does not trigger CI.
- Runner pods are ephemeral and have no volumes. A cache must live off the pod.
- sccache on R2 exists only in reverie. Its stats are never checked there.

## Design

### Runner lanes

| Lane | Label | Use |
|---|---|---|
| `ci-rust` | `runs-on: {group: ent-default, labels: [ci-rust]}` | Rust jobs. 10 CPU. Rust 1.94.1, rustfmt, clippy, cargo-deny, typos. |
| `self-hosted` | `self-hosted` | Jobs without Rust. Example: `secrets`. |

### Workflows and jobs

| Workflow | Jobs or role |
|---|---|
| `ci.yml` | `secrets`, `rust`, `cargo-deny`, `hygiene`, `moon-affected` |
| `_rust.yml` | Reusable. The `rust` job calls it. |
| `template-check.yml` | `template-check`. `make new-repo` deletes this file. |
| `codeql.yml` | Dormant. See below. |
| `sbom.yml` | `cargo-cyclonedx` on push to `main` and on release. |
| `release.yml` | release-plz `release` on push to `main`. |
| `audit.yml` | Weekly `cargo audit`. |
| `labeler.yml` | Labels PRs by path. |

Required checks are `secrets`, `rust`, `cargo-deny` and `hygiene`. Spec 0003
holds the protection table.

### Cache

- `Swatinem/rust-cache` runs in every Rust job. It uses `shared-key`. Only the
  `development` and `main` refs save the cache (`save-if`); PRs read it.
  `cache-on-failure` is true.
- sccache on Cloudflare R2 is planned. The secret names match reverie's:
  `SCCACHE_R2_BUCKET`, `SCCACHE_R2_ENDPOINT`, `R2_ACCESS_KEY_ID`,
  `R2_SECRET_ACCESS_KEY`. The job maps the bucket secret to `SCCACHE_BUCKET`
  and every sccache step skips by `if: env.SCCACHE_BUCKET != ''` until the
  bucket and secrets exist. The paas lane provisions them.
- When sccache is on, `sccache --start-server` must succeed, or the job fails.
  A final step reads `sccache --show-stats`. It fails the job when the
  `Cache errors` count is above 0 or the `Cache location` is not the S3 (R2)
  backend. A cache that fails in silence is a cost with no benefit.
- The `ci-rust` image has no sccache or nextest today. `taiki-e/install-action`
  installs nextest, pinned by SHA. The paas lane may bake both into the image.

### Pinning

- Every action is pinned by commit SHA with a version comment. No `@main`.
- Images are pinned by digest where the registry gives one. The `ci-rust` tag
  has no digest pin today. The paas lane tracks that gap.
- Every job sets `permissions`. The default is `contents: read`.
- A fork guard blocks jobs on PRs from forks.

### CodeQL is dormant

`codeql.yml` runs only when the repo variable `CODE_SCANNING` equals `true`.
Without it, the workflow does nothing. Code Security is a paid feature on
private repos. Enabling it is a paid decision for the operator, the operator.
Nobody turns it on without his go.

### Release flow

Refined from decision 8, the operator, interviews 2026-10-08.

1. A person opens a normal PR to `development`. The PR runs
   `make release-notes VERSION=x.y.z`. It sets the workspace version, runs
   `cargo update -w`, and moves `[Unreleased]` entries under a dated heading.
   CI runs on this PR.
2. The promotion PR moves the change to `main` (spec 0003).
3. On push to `main`, release-plz `release` runs with `git_only = true`. When
   the version has no tag yet, it creates tag `vX.Y.Z` and the GitHub release.
4. `release-plz.toml` sets in `[workspace]`: `git_only = true`, `publish =
   false`, `changelog_update = false`, `git_release_enable = false`,
   `git_tag_enable = false`, `semver_check = false`. The package `cas-cli`
   turns on the tag and the release. Its tag name is `v{{ version }}`.
5. `CHANGELOG.md` follows Keep a Changelog 1.1.0. Each user-visible PR adds one
   line under `## [Unreleased]`. Nobody edits a released section.

## Plan

- [ ] Write `ci.yml`, `_rust.yml` and `template-check.yml`.
- [ ] Write `codeql.yml`, `sbom.yml`, `release.yml`, `audit.yml`, `labeler.yml`.
- [ ] Pin every action by SHA.
- [ ] Add the sccache step and its hard stats check, skipped by `if:`.
- [ ] Run once on a PR. Confirm the four required check names.
- [ ] Ask the paas lane for the R2 bucket, secrets and image changes.

## Test

Check the workflows for the faults this spec bans.

```bash
! grep -rn '|| cargo test' .github && ! grep -rnE 'uses: [^ ]+@(main|master)' .github
```

Passing means exit 0 and no output. Then read a CI run. The `rust` job must
pass with the sccache step skipped. The `codeql` workflow must skip when
`CODE_SCANNING` is unset. After a release, `gh release view v0.1.0` must show
the release that release-plz created.

## Notes

Alternatives rejected:

- release-plz opens the release PR. A PR from `GITHUB_TOKEN` runs no CI.
- A custom runner image per repo. The paas lane owns the shared image.

Open questions: R2 bucket name and org secrets. The ARC PAT expires 2026-12-26.

References:

- `cerebral-work/cortex@524a97b:.github/workflows/ci.yml`
- `cerebral-work/cortex@524a97b:specs/0008-branch-model-and-promotion.md`
