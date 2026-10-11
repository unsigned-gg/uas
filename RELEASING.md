# Releasing

This page says how a uas version is numbered, recorded and tagged. The rules come from
`specs/0048-licence-and-publication.md` D13.

## Versioning

uas is pre-1.0 and follows semver as Cargo reads it for `0.y.z`:

- A breaking change to a public item bumps the minor version (`0.y.z` to `0.(y+1).0`).
- A compatible change or a fix bumps the patch version.
- Nothing is promised stable before 1.0. The pre-1.0 rule still holds: a break is never hidden in a patch.

## CHANGELOG

Each user-visible PR adds one line under `## [Unreleased]` in `CHANGELOG.md`. Never edit a
released section. The text rules are in `specs/0053-changelog-and-commit-text-policy.md`.
`make release-notes VERSION=x.y.z` turns the Unreleased lines into a release section, in a PR
to `development`.

## Tags

release-plz tags as it does today (`release-plz.toml`): on a push to `main` it tags `vX.Y.Z`
for `uas-cli` and creates the GitHub release. It does not publish to a registry, and it does
not write the changelog.

## Publication

The crates.io `uas` crate is the publication vehicle (spec 0048 D6). Publishing is
Christian's act. Every crate sets `publish = false` until then.

## Semver gate

`.github/workflows/semver.yml` runs `cargo-semver-checks` on each crate that is not
`publish = false` and fails a breaking change that lacks the minor bump. While every crate
sets `publish = false` it prints `skipped: no published crate` and passes. The job is not a
required check until a crate publishes. The tool version and sha256 are pinned in
`specs/0004-dependency-and-licence-policy.md`.

## Go or no-go

Before the repository turns public, the launch checklist of spec 0048 D14 holds, in
`engineering/plans/0048-launch-checklist.md`. The last item is Christian's go.
