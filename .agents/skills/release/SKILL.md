---
name: release
description: Use when you cut a new version of this repo. Covers the version bump and changelog roll with make release-notes, the PR into development, the promotion PR to main, and the tag that release-plz creates.
---

# Release

The release flow is in spec 0005 (`specs/0005-ci-cache-and-runners.md`). Follow the steps in order.

## Before you start

- You are on a clean tree. Run `git status --short`. The output must be empty.
- `development` is green. Check the latest CI run on `development`.
- `## [Unreleased]` in `CHANGELOG.md` has entries. If it is empty, there is nothing to release.

## Steps

1. Cut a branch from `development`:

```bash
git fetch origin
git switch -c chore/release-x.y.z origin/development
```

2. Roll the version and the changelog. This sets `[workspace.package] version`, runs `cargo update -w`, and moves the `[Unreleased]` entries under `## [x.y.z] - YYYY-MM-DD`. It leaves an empty `## [Unreleased]`.

```bash
make release-notes VERSION=x.y.z
```

3. Read the diff. Check `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`. Nothing else should change.
4. Run the gates:

```bash
make check
```

5. Commit with a signed Conventional Commit, for example `chore: release x.y.z`.
6. Open a PR into `development`. Use a normal PR so that CI runs. A PR opened by the default GITHUB_TOKEN does not trigger CI.
7. Merge on green CI with a merge commit.
8. Open the promotion PR from `development` to `main`.
9. Merge the promotion PR on green CI with a merge commit pinned to the head SHA (spec 0003). An agent never approves it.
10. Check that the `release` run on `main` is green. A red run is a stop.

## After the promotion merge

- On the push to `main`, release-plz runs `release` with `git_only = true`. If the version has no tag yet, it creates tag `vX.Y.Z` and the GitHub release. The release body says "See CHANGELOG.md for vX.Y.Z."
- Verify the result. Do not trust the green run alone:

```bash
git fetch origin --tags
git tag --list 'v*'
gh release view vX.Y.Z
```

- Check that the tag points at the commit on `main` that holds version x.y.z.

## Rules

- Never edit a released section of `CHANGELOG.md`.
- Never tag by hand. release-plz owns the tag.
- Never push to `main`.
- If the tag or release is missing, stop and report. Do not improvise.
