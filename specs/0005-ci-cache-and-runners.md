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
- The `ci-kernel` runner is live (OPS-1660). It admits pods with uid 0 and bounding set {BPF, PERFMON}, read-only tracefs. [reported, mesh c73dfbe3, 2026-10-10]

## Design

### Requirements (EARS)

- **Ubiquitous:** The CI system shall execute Rust jobs on ephemeral `ci-rust` runners using the pre-baked toolchain.
- **Ubiquitous:** The build pipeline shall configure `Swatinem/rust-cache` with write access restricted to `development` and `main`.
- **Event-driven:** WHEN commits land on `main`, the CI system shall execute SBOM generation and `release-plz` publishing.
- **State-driven:** WHILE running inside CI runner pods, the system shall execute jobs without mutating the read-only rustup directory.
- **Unwanted behavior:** IF a job on the `ci-rust` lane attempts toolchain installation or network toolchain download, THEN the job shall fail immediately.
- **Ubiquitous:** The `ebpf-live` job shall run on the `ci-kernel` lane and run `make ebpf-test SUDO=` as uid 0.
- **Event-driven:** WHEN CI runs, the `ebpf-live` job shall rebuild `cas-ebpf.o` with the pinned nightly and bpf-linker and fail when it differs from the committed object.
- **State-driven:** WHILE on the `ci-kernel` lane, the job shall install the pinned nightly only into a job-local `RUSTUP_HOME`.
- **Unwanted behavior:** IF a live case prints a `skipped:` line other than the nested unshare case, THEN the `ebpf-live` job shall fail.

### Runner lanes

| Lane | Label | Use |
|---|---|---|
| `ci-rust` | `runs-on: {group: ent-default, labels: [ci-rust]}` | Rust jobs. 10 CPU. Rust 1.94.1, rustfmt, clippy, cargo-deny, typos. |
| `self-hosted` | `self-hosted` | Jobs without Rust. Example: `secrets`. |
| `ci-kernel` | `runs-on: [self-hosted, ci-kernel]` | The live eBPF gate, job `ebpf-live`. Uid 0 in the pod, bounding set {BPF, PERFMON}, tracefs read-only. Same image as `ci-rust`: Rust 1.94.1 only, no nightly, no bpf-linker. One runner at a time, scales from zero. |

### Workflows and jobs

| Workflow | Jobs or role |
|---|---|
| `ci.yml` | `secrets`, `rust`, `cargo-deny`, `hygiene`, `moon-affected`, `ebpf-live` |
| `_rust.yml` | Reusable. The `rust` job calls it. |
| `template-check.yml` | `template-check`. `make new-repo` deletes this file. |
| `codeql.yml` | Dormant. See below. |
| `sbom.yml` | `cargo-cyclonedx` on push to `main`. |
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
private repos. Enabling it is a paid decision for the operator, Christian.
Nobody turns it on without his go.

### Release flow

Refined from decision 8, Christian, interviews 2026-10-08.

1. A person opens a normal PR to `development`. The PR runs
   `make release-notes VERSION=x.y.z`. It sets the workspace version, runs
   `cargo update -w`, and moves `[Unreleased]` entries under a dated heading.
   CI runs on this PR.
2. The promotion PR moves the change to `main` (spec 0003).
3. On push to `main`, release-plz `release` runs with `git_only = true`. When
   the version has no tag yet, it creates tag `vX.Y.Z` and the GitHub release.
   The job first installs a pinned `gh`, because the `ci-rust` image has none
   and the release-plz action needs it (spec 0004, CI tools). The release-plz
   step runs only on `main`. A manual run on another branch creates no tag.
   The job also has `pull-requests: read`, because release-plz lists the pull
   requests of the released commit.
4. `release-plz.toml` sets in `[workspace]`: `git_only = true`, `publish =
   false`, `changelog_update = false`, `git_release_enable = false`,
   `git_tag_enable = false`, `semver_check = false`. The package `cas-cli`
   turns on the tag and the release. Its tag name is `v{{ version }}`.
5. `CHANGELOG.md` follows Keep a Changelog 1.1.0. Each user-visible PR adds one
   line under `## [Unreleased]`. Nobody edits a released section.

### Decisions

- **D1 (lane w25:p1, 2026-10-10):** The `ebpf-live` job runs on the `ci-kernel` lane. It installs the pinned nightly into a job-local `RUSTUP_HOME`. It rebuilds `cas-ebpf.o` and fails on any difference from the committed object. Christian can override.
- **D2 (lane w25:p1, 2026-10-10):** `ebpf-live` is not a required check until one green run on `development`. Then the integrator adds it to the protection and to the spec 0003 table. Christian can override.

## Plan

- [x] Write `ci.yml` and `_rust.yml`. (`template-check.yml` is template-only. `make new-repo` deletes it.)
- [x] Write `codeql.yml`, `sbom.yml`, `release.yml`, `audit.yml`, `labeler.yml`.
- [x] Pin every action by SHA.
- [x] Add the sccache step and its hard stats check, skipped by `if:`.
- [x] Run once on a PR. Confirm the four required check names (spec 0003).
- [ ] Ask the paas lane for the R2 bucket, secrets and image changes.
- [x] Run `ebpf-live` green on a PR, with `9 passed` and only the nested skip line (PR #51 at cf78b64, run 38028859773, 2026-10-10).
- [x] Add `ebpf-live` to the required checks and to the spec 0003 table (D2; development run 38028975897 green, 2026-10-10).
- [ ] Retire the operator-run `make ebpf-test` gate for the 8 cases that load. The nested case waits for the OPS-1660 seccomp half.

## Test

Check the workflows for the faults this spec bans.

```bash
! grep -rn '|| cargo test' .github && ! grep -rnE 'uses: [^ ]+@(main|master)' .github
make -n ebpf-test | grep -q sudo && ! make -n ebpf-test SUDO= | grep -q sudo && actionlint
make ebpf-build JOBS=2 && git diff --exit-code -- crates/cas-ebpf/ebpf/
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
