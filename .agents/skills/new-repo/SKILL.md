---
name: new-repo
description: Use when you make a new Rust monorepo from this template. Covers GitHub Use this template or cargo generate, then make new-repo NAME=<name>, and the checks to run before the first commit.
---

# New repo from the template

The rename logic is in the `Makefile` target `new-repo`. There is one implementation.

## Choose a name

- Use kebab-case. It must match `^[a-z][a-z0-9-]*$`. Example: `cas`.
- The name becomes the crate prefix: `<name>-domain` and `<name>-cli`. The binary becomes `<name>`.

## Path A: GitHub template

1. On GitHub, open `cerebral-work/rust-monorepo-template` and use "Use this template". Create the repo private.
2. Clone it. Check out `main`.
3. Run:

```bash
make new-repo NAME=<name>
```

Set `ORG=<org>` if the repo is not under `cerebral-work`.

## Path B: cargo generate

```bash
cargo generate --git git@github.com:cerebral-work/rust-monorepo-template.git --name <name>
cd <name>
# cargo generate runs `git init` but does not commit. `make new-repo` renames
# with `git mv` and refuses an uncommitted tree, so commit first.
git add -A && git commit -S -m "chore: initial commit from rust-monorepo-template"
make new-repo NAME=<name>
```

## What `make new-repo` does

- Checks the name. Refuses on a dirty tree.
- Renames `crates/skel-domain` and `crates/skel-cli`.
- Replaces the word `skel` (whole word only), `skel_`, `SKEL_` and `rust-monorepo-template` in tracked files. It skips `vendor/`, `Cargo.lock` and this skill, which stays as the record of how the repo was made.
- Deletes `.github/workflows/template-check.yml`.
- Regenerates `Cargo.lock`.
- Prints `git status --short`. It does not commit.

## After the rename

1. Read the diff with `git diff` and `git status --short`.
2. Search for leftovers. The command must print nothing:

```bash
grep -rIn -E '\bskel\b|skel_|SKEL_' --exclude-dir=target --exclude-dir=.git --exclude-dir=vendor . | grep -v '^\./\.agents/skills/new-repo/'
```

3. Run the gates:

```bash
make check
make hygiene
make smoke
```

4. Edit `README.md`, `AGENTS.md` and `CHANGELOG.md` so they describe the new repo. Add a spec for the first real design in `specs/` (see the `spec-driven-dev` skill).
5. Commit with a signed Conventional Commit.

## Operator steps

These need the operator. Prepare them and stop:
- Branch protection on `main`. Spec 0003 has the table.
- Required checks: `secrets`, `rust`, `cargo-deny`, `hygiene`.
- Secrets for CI, such as the sccache bucket.
- A registry row in `cerebral-work/terrarium`, if the repo is a registered node.

## Do not

- Do not run `make new-repo` on a dirty tree.
- Do not edit `vendor/leanspec/`.
- Do not push to `main`.
