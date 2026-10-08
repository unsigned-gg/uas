---
status: in-progress
created: 2026-10-08
tags: [template, architecture, workspace]
priority: high
---

# 0001-template-architecture

## Overview

This repo is the estate Rust monorepo template. It is a GitHub template repo
and it builds on itself. A new repo starts from it, then runs `make new-repo`.

The estate had no Rust template on 2026-10-08. Only `cf-pwa-template` and
`vst-template` existed. Each Rust repo copied files from cortex by hand. That
copying caused drift. Cortex 2026-10-08 shows `license = "MIT"` beside a
proprietary LICENSE, a CI rust job without `-j`, and a `CLAUDE.md` that
contradicts `AGENTS.md`.

This spec fixes the layout. It also fixes the rule for how crates depend on
each other, and the way the template renames itself.

## Design

### Layout

| Path | Content |
|---|---|
| `crates/cas-domain` | Library. Zero I/O. Entities and repository traits. |
| `crates/cas-cli` | Binary named `cas`. `build.rs` stamps `git describe`. |
| `xtask/` | Package `xtask`. Holds `spec-check` and `toolchain-check`. Never renamed. |
| `specs/` | LeanSpec registry. See spec 0002 for the gates that read it. |
| `vendor/leanspec/` | Vendored LeanSpec docs. Not edited. Not renamed. |
| `.agents/skills/` | Skills. `AGENTS.md` is the canonical rule file. |
| `Makefile` | Single entry point for every gate. |

The workspace is virtual. Members are `crates/*` and `xtask`. `Cargo.lock` is
committed. `.cargo/config.toml` defines the alias `xtask = "run -q -p xtask --"`.

### Dependency direction

Crates depend only downward. The domain crate is the lowest layer. It has no
I/O. The CLI crate depends on the domain crate. The domain crate never depends
on the CLI crate.

`make smoke` enforces the zero-I/O rule. It fails when
`crates/cas-domain/src` contains `std::net`, `std::fs`, `tokio` or
`std::process`.

### Rename mechanism

Operator decision, the operator, interview 2026-10-08: "just use a makefile".
The rename logic lives only in the Makefile target `new-repo`. No script file
holds a second copy.

`make new-repo NAME=<kebab> [ORG=cerebral-work]` does these steps:

1. Check that `NAME` matches `^[a-z][a-z0-9-]*$`.
2. Refuse to run on a dirty tree.
3. Run `git mv` on both crate directories.
4. Rewrite tracked text files. Skip `vendor/`, `Cargo.lock` and binary files.
5. Delete `.github/workflows/template-check.yml`.
6. Regenerate `Cargo.lock` with `cargo update -w`.
7. Print `git status --short`. It does not commit.

`cargo generate` is the second entry path. `cargo-generate.toml` calls the
same make target in a post-hook. One implementation serves both paths.

### What an instantiated repo changes

- It renames the `cas` crates to its own prefix.
- It loses the `template-check` workflow.
- It adds its own specs from number 0006 upward. It keeps 0001 to 0005 and
  edits them to match the new repo.
- It keeps `vendor/leanspec/` as it is.

### Decisions of record

| Decision | Who | When |
|---|---|---|
| Template is a buildable GitHub template and a `cargo-generate.toml` | the operator | 2026-10-08 |
| `AGENTS.md` is canonical. `CLAUDE.md` is a pointer. | the operator | 2026-10-08 |
| Rename logic lives in the Makefile | the operator | 2026-10-08 |
| Crate prefix is `cas` | the operator | 2026-10-08 |

## Plan

- [ ] Workspace files: `Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml`.
- [ ] Crates `cas-domain` and `cas-cli`, plus `xtask`.
- [ ] Makefile with `new-repo` and `template-check`.
- [ ] `cargo-generate.toml` that delegates to `make new-repo`.
- [ ] Land on `development`. Promote to `main` by the rule in spec 0003.
- [ ] Set `is_template = true` after the first green CI run.

## Test

Run the self-check. It builds the template, runs the gates, then renames a
copy to `aaa-probe` and builds that copy. The name sorts before every
dependency, so the rename reorders imports and `cargo fmt --all --check` proves
that `make new-repo` reformats them.

```bash
make template-check
```

Passing means four things. `make check` and `make smoke` pass. The probe copy
is rustfmt-clean. The probe copy builds. The command `grep -rIn --exclude-dir={target,.git,vendor} cas .` finds
nothing in the probe copy.

## Notes

Alternatives rejected:

- A `scripts/new-repo.sh` file. It would hold a second rename implementation.
- Only `cargo generate`. The GitHub "Use this template" button would not work.

Open question: pgt is not a dependency of any template crate. Spec 0004 records
it as a commented example.

References:

- `cerebral-work/cortex@524a97b:specs/0008-branch-model-and-promotion.md`
- `cerebral-work/cortex@524a97b:Cargo.toml`
- `codervisor/leanspec@ee122d6:skills/leanspec/SKILL.md`
