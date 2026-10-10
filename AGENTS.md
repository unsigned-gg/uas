# AGENTS.md - uas

This file is the canonical rule set for people and agents. Every rule is stated once, here.
`CLAUDE.md` only points to this file.

## What this is

`unsigned-gg/uas` is the Unsigned Agent Substrate (UAS). It is a Rust-native,
pooled runtime for agent harnesses. Status: scaffold and domain; no runtime yet.
The design is a draft in `cerebral-work/research@4068134`. Read
`specs/0001-cas-architecture-and-crate-layout.md` before you change any crate: it holds each
crate's role and its phase. The repo was made from the estate Rust monorepo template. The Go
repo `unsigned-gg/substrate` is a different system.

## Layout

Layering: from the top layer down, `deck > acp > engine > pool > sandbox, ebpf > router > domain`.
A crate depends only on crates in lower layers. Add a Cargo dependency only when real code needs it.
`uas-domain` depends on no other crate in the workspace. It has zero I/O: it must not use `std::net`,
`std::fs`, `std::process` or `tokio`. `make smoke` checks this.

| Path | Content |
|---|---|
| `crates/` | The `uas-*` crates. `uas-cli` is the binary `uas`; it parses arguments and calls the domain. |
| `xtask/` | Repo tasks in Rust: `spec-check`, `toolchain-check` and `pin-check`. The package name stays `xtask`. |
| `specs/` | LeanSpec specs and the registry `specs/README.md`. |
| `vendor/` | Vendored and never edited: `leanspec/` (the method, MIT), `soma/` (containment and dispatch JSON schemas), `cortex/` (Mesh v1 protobufs), `saga/` (workflow templates and domain contracts). |
| `.agents/skills/` | Skills for agents, surfaced to Claude Code through `.claude/skills/`. |
| `.claude/` | Claude Code settings and hooks. `.claude/agents/` holds the repo's subagents; see `.claude/agents/README.md`. |
| `docs/`, `engineering/` | Docs for people. Engineering notes. |

## Rules that bind

1. **LeanSpec first.** A new concept or an architecture change starts as a spec in `specs/`. Run `make spec-check`. It must pass before you open the PR.
2. **Use `-j 2` on shared workstations.** The Makefile sets `JOBS ?= 2` and passes it to every cargo call. Do not raise it on a shared host. CI is the one exception: it calls make with `JOBS=10`. Spec 0002 gives the reason.
3. **Evidence tiers.** Label every claim `verified`, `reported`, `heuristic` or `asserted`. Only `verified` satisfies a gate. A `verified` claim names the command that proved it.
4. **Branch model.** Work happens on feature branches cut from `development`. A PR into `development` merges on green CI. `main` is the release branch. Spec 0003 has the details.
5. **Sign every commit.** Never skip signing.
6. **Use Conventional Commits.** Do not put a ticket ID in the subject. Put `Part of CER-N` in the PR body.
7. **Merge with merge commits only.** Never squash. Enforced: squash and rebase merging are off on the GitHub repo (checked 2026-10-10).
8. **Write the changelog.** Add one line under `## [Unreleased]` in `CHANGELOG.md` for each user-visible PR. Never edit a released section.
9. **Merge the promotion PR on green.** The promotion PR carries `development` to `main`. A lane merges it when CI is green, with a merge commit pinned to the head SHA. Agents never approve it. Then check that `main` matches `development` and that the `release` run is green. Spec 0003 has the operator ruling.
10. **Never push to `main`.** Enforced by the hook `guard-main-push.sh`.
11. **No secrets in the repo.** Enforced: `make` and lefthook run gitleaks.
12. **Pin every external dependency** and record it in spec 0004 before first use. Enforced: copyleft licences are denied by `deny.toml`, and `make pin-check` binds the spec 0004 tables to the lockfiles.
13. **Write prose in short sentences.** Use active voice. Use ASCII only in files.
14. **Keep `unsafe` out, except where syscalls need it.** The workspace forbids `unsafe`. `uas-sandbox`, `uas-pool` and `uas-ebpf` deny it instead: a block opts in with `#[allow(unsafe_code)]` and a `// SAFETY:` comment, and clippy rejects an unsafe block with no SAFETY comment. Spec 0001 records the ruling.

## Commands

`make help` lists every target. These are the contract.

| Target | What it does |
|---|---|
| `make check` | `fmt-check`, `clippy`, `test`, `doc-test`, `deny`. Run before you push. |
| `make hygiene` | `typos`, `machete`, `spec-check`, `toolchain-check`, `pin-check`. |
| `make ci` | What the CI `rust` job runs. |
| `make smoke` | Build the CLI, run `uas --version`, check the domain has no I/O. |
| `make spec-check` | Validate `specs/`. |
| `make release-notes VERSION=x.y.z` | Roll `[Unreleased]` into a version heading. |
| `make custody-check` | Check the blackwall custody setup (`blackwall doctor`). See the `custody-run` skill. |

## Skills

Skills live in `.agents/skills/<name>/SKILL.md` and reach Claude Code through symlinks in
`.claude/skills/`. Run `/skills` to list them, and read the skill before you start that kind of
work: `spec-driven-dev` (a new concept, an architecture change, a feature), `release` (cutting a
version), `ci-triage` (a red CI run), `new-repo` (making a repo from this template), `custody-run`
(agent work under blackwall custody).

## Approval routes

- A lane merges its own feature PR into `development` when CI is green.
- A lane merges the promotion PR from `development` to `main` when CI is green (spec 0003).
- Secrets, Terraform, DNS and platform apps need the operator. Prepare the change and stop.
- Force-push to `main`, unsigned commits and merging with red CI are never allowed.
- Code owners are listed in `.github/CODEOWNERS`.

## Where things are

- Specs: `specs/README.md` is the registry. The method is in `vendor/leanspec/`.
- Docs: `docs/README.md`. Reference and runbooks are under `docs/`.
- Engineering notes: `engineering/`.
- Hooks and their bypasses: `.claude/README.md`.
- Contribution guide: `CONTRIBUTING.md`.
