# AGENTS.md - cas

This file is the canonical rule set for people and agents. Every rule is stated once, here.
`CLAUDE.md` only points to this file.

## What this is

The estate Rust monorepo template, `cerebral-work/cas`. It builds and tests itself.
Start a new repo from it with `make new-repo NAME=<name>`. The skill `.agents/skills/new-repo/SKILL.md` gives the steps.
After you run `make new-repo`, this file describes your new repo. Edit it to match.

## Layout

| Path | Content |
|---|---|
| `crates/cas-domain` | Library. Entities and ports. Zero I/O. |
| `crates/cas-cli` | Binary `cas`. Parses arguments and calls the domain. |
| `xtask/` | Repo tasks in Rust: `spec-check` and `toolchain-check`. The package name stays `xtask`. |
| `specs/` | LeanSpec specs and the registry `specs/README.md`. |
| `vendor/leanspec/` | Vendored LeanSpec method (MIT). Do not edit. |
| `.agents/skills/` | Skills for agents. |
| `.claude/` | Claude Code settings and hooks. |
| `docs/`, `engineering/` | Docs for people. Engineering notes. |

Dependency direction: crates depend only downward. The CLI depends on the domain. The domain depends on no other crate.
The domain crate has zero I/O. It must not use `std::net`, `std::fs`, `std::process` or `tokio`. `make smoke` checks this.

## Rules that bind

1. **LeanSpec first.** A new concept or an architecture change starts as a spec in `specs/`. Run `make spec-check`. It must pass before you open the PR.
2. **Use `-j 2` on shared workstations.** The Makefile sets `JOBS ?= 2` and passes it to every cargo call. Do not raise it on a shared host. CI is the one exception: it calls make with `JOBS=10`. Spec 0002 gives the reason.
3. **Evidence tiers.** Label every claim `verified`, `reported`, `heuristic` or `asserted`. Only `verified` satisfies a gate. A `verified` claim names the command that proved it.
4. **Branch model.** Work happens on feature branches cut from `development`. A PR into `development` merges on green CI. `main` is the release branch. Spec 0003 has the details.
5. **Sign every commit.** Never skip signing.
6. **Use Conventional Commits.** Do not put a ticket ID in the subject. Put `Part of CER-N` in the PR body.
7. **Merge with merge commits only.** Never squash.
8. **Write the changelog.** Add one line under `## [Unreleased]` in `CHANGELOG.md` for each user-visible PR. Never edit a released section.
9. **Never approve the promotion PR.** The promotion PR carries `development` to `main`. Agents do not approve it and do not click merge. The operator does.
10. **Never push to `main`.** The hook `guard-main-push.sh` blocks it.
11. **No secrets in the repo.** `make` and lefthook run gitleaks.
12. **Pin every external dependency** and record it in spec 0004 before first use. Copyleft licences are denied by `deny.toml`.
13. **Write prose in short sentences.** Use active voice. Use ASCII only in files.

## Commands

Run `make help` for the full list. These targets are the contract.

| Target | What it does |
|---|---|
| `make build` | Build the workspace. |
| `make fmt` / `make fmt-check` | Format, or check formatting. |
| `make clippy` | Clippy with warnings denied. |
| `make test` | Run tests with nextest. |
| `make doc-test` | Run doc tests. |
| `make deny` | Check licences, advisories and sources. |
| `make check` | `fmt-check`, `clippy`, `test`, `doc-test`, `deny`. Run before you push. |
| `make ci` | What the CI `rust` job runs. |
| `make hygiene` | `typos`, `machete`, `spec-check`, `toolchain-check`. |
| `make spec-check` | Validate `specs/`. |
| `make toolchain-check` | Compare `rust-toolchain.toml` with the installed Rust. |
| `make smoke` | Build the CLI, run `cas --version`, check the domain has no I/O. |
| `make release-notes VERSION=x.y.z` | Roll `[Unreleased]` into a version heading. |
| `make new-repo NAME=<name>` | Rename the template into a new repo. |
| `make template-check` | Check that the template and a renamed copy both build. |

## Skills

Skills are in `.agents/skills/<name>/SKILL.md`. Read the skill before you start that kind of work.

| Skill | Use it when | Path |
|---|---|---|
| `spec-driven-dev` | You add a concept, change the architecture, or start a feature. | `.agents/skills/spec-driven-dev/SKILL.md` |
| `release` | You cut a version. | `.agents/skills/release/SKILL.md` |
| `ci-triage` | A CI run is red. | `.agents/skills/ci-triage/SKILL.md` |
| `new-repo` | You make a repo from this template. | `.agents/skills/new-repo/SKILL.md` |

## Approval routes

- A lane merges its own feature PR into `development` when CI is green.
- The promotion PR from `development` to `main` needs the operator's go.
- Secrets, Terraform, DNS and platform apps need the operator. Prepare the change and stop.
- Force-push to `main`, unsigned commits and merging with red CI are never allowed.
- Code owners are listed in `.github/CODEOWNERS`.

## Where things are

- Specs: `specs/README.md` is the registry. The method is in `vendor/leanspec/`.
- Docs: `docs/README.md`. Reference and runbooks are under `docs/`.
- Engineering notes: `engineering/`.
- Hooks and their bypasses: `.claude/README.md`.
- Contribution guide: `CONTRIBUTING.md`.
