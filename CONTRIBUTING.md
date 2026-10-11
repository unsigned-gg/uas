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
- The promotion PR from `development` to `main` merges on green CI (spec 0003).
- Run `make check` and `make hygiene` before you push.

## Pull requests

Fill in the PR template. Put the commands you ran and their results under "Verified". Write `Part of CER-N` in the body.

## Licence

Code is licensed `Apache-2.0 OR MIT`. The BPF program under `crates/uas-ebpf/ebpf/` is `GPL-2.0-only OR MIT`. The Markdown files under `docs/`, `specs/` and `engineering/` are `CC-BY-4.0`. [NOTICE](NOTICE) maps each licence to its files. By contributing, you license your contribution under the licence of the files you change.

## Developer Certificate of Origin

Every commit carries a `Signed-off-by:` trailer whose name and email match the commit author. Add it with `git commit -s`. There is no CLA. The trailer certifies the Developer Certificate of Origin 1.1:

```text
Developer Certificate of Origin
Version 1.1

Copyright (C) 2004, 2006 The Linux Foundation and its contributors.

Everyone is permitted to copy and distribute verbatim copies of this
license document, but changing it is not allowed.


Developer's Certificate of Origin 1.1

By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I
    have the right to submit it under the open source license
    indicated in the file; or

(b) The contribution is based upon previous work that, to the best
    of my knowledge, is covered under an appropriate open source
    license and I have the right under that license to submit that
    work with modifications, whether created in whole or in part
    by me, under the same open source license (unless I am
    permitted to submit under a different license), as indicated
    in the file; or

(c) The contribution was provided directly to me by some other
    person who certified (a), (b) or (c) and I have not modified
    it.

(d) I understand and agree that this project and the contribution
    are public and that a record of the contribution (including all
    personal information I submit with it, including my sign-off) is
    maintained indefinitely and may be redistributed consistent with
    this project or the license(s) involved.
```

The sign-off does not replace the GPG signature. Commits stay signed.

## Conduct and security

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) and [SECURITY.md](SECURITY.md).
