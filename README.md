# rust-monorepo-template

The estate Rust monorepo template. It builds and tests itself.

**Status:** under construction. Version 0.1.0. Private. Proprietary licence (see `LICENSE`).

## What you get

- A Cargo workspace: `crates/skel-domain` (zero I/O) and `crates/skel-cli`, plus `xtask/`.
- A `Makefile` with one target for each gate. Run `make help`.
- LeanSpec specs in `specs/`, checked by `make spec-check`.
- CI, moon tasks, lefthook, gitleaks and cargo-deny.
- `AGENTS.md`, skills in `.agents/skills/`, and Claude Code hooks in `.claude/`.

## Start a repo from it

Option 1: use the GitHub template.

1. Open this repo on GitHub. Choose "Use this template".
2. Clone the new repo. Then run:

```bash
make new-repo NAME=<name>
```

Option 2: use cargo-generate.

```bash
cargo generate --git git@github.com:cerebral-work/rust-monorepo-template.git --name <name>
cd <name>
git add -A && git commit -S -m "chore: initial commit from template"  # new-repo needs a committed tree
make new-repo NAME=<name>
```

Then run `make check`. Read the diff before you commit.

## Where things are

| Need | Go to |
|---|---|
| The rules for people and agents | [AGENTS.md](AGENTS.md) |
| The specs and their registry | [specs/README.md](specs/README.md) |
| Docs | [docs/README.md](docs/README.md) |
| How to contribute | [CONTRIBUTING.md](CONTRIBUTING.md) |
| All make targets | `make help` |
