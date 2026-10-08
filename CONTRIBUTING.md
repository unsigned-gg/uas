# Contributing

The full rule set is in [AGENTS.md](AGENTS.md). This page is the short version for people.

## Set up

1. Install `rustup`. It reads `rust-toolchain.toml` and installs the pinned Rust on first use.
2. Install the tools: `cargo-nextest`, `cargo-deny`, `typos-cli`, `cargo-machete`, `gitleaks`, `lefthook`.
3. Install the git hooks:

```bash
lefthook install
```

4. Optional: cache compiles with sccache. Add this to your own `~/.cargo/config.toml`, not to the repo:

```toml
[build]
rustc-wrapper = "sccache"
```

5. Check that the build works:

```bash
make check
```

## Rules in one page

- A new concept or an architecture change starts as a spec in `specs/`. Run `make spec-check`.
- Use `make` targets. On a shared workstation they pass `-j 2`. Do not raise it.
- Cut your branch from `development`. Open the PR into `development`.
- Sign every commit. Use Conventional Commits. Do not put a ticket ID in the subject.
- Merge with a merge commit. Never squash.
- Add one line under `## [Unreleased]` in `CHANGELOG.md` for each user-visible change.
- The promotion PR from `development` to `main` is the operator's decision.
- Run `make check` and `make hygiene` before you push.

## Pull requests

Fill in the PR template. Put the commands you ran and their results under "Verified". Write `Part of CER-N` in the body.

## Conduct and security

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) and [SECURITY.md](SECURITY.md).
