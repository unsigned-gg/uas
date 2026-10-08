---
status: in-progress
created: 2026-10-08
tags: [dependencies, licence, deny, pgt]
priority: high
depends_on: [0001-template-architecture]
---

# 0004-dependency-and-licence-policy

## Overview

This spec sets the licence of the template and the rules for external code.

The template is proprietary. `[workspace.package]` sets
`license = "LicenseRef-Proprietary"`. The copyright holder is the operator M.
Todie / cerebral.work. Contact: chris@todie.io. Cortex drifted from this rule
with `license = "MIT"` beside a proprietary LICENSE, measured 2026-10-08.

Facts from 2026-10-08:

- `cerebral-work/pgt` is private. It has no licence file. Head is `54d90f5`.
  It uses edition 2024, so the workspace `rust-version` floor is 1.85.
- `autonice` is GPL-2.0. `zerostack` is GPL-3.0. Both are copyleft.
- `0xPlaygrounds/rig` is MIT, version 0.44.0.
- All eight research evaluations in `cerebral-work/research` are drafts.

## Design

### Licence rules

- Copyleft is denied. `deny.toml` lists `GPL-2.0`, `GPL-3.0`, `AGPL-3.0` and
  `LGPL-3.0` under `deny`. The two named cases are autonice and zerostack.
- `cargo deny check` runs in `make check` and in the CI job `cargo-deny`.
- A new licence needs an operator decision before it enters the allow list.

### External dependencies

- Every external dependency is pinned and recorded before first use.
- Each `[workspace.dependencies]` entry has a comment that says why.
- `[sources]` uses `unknown-git = "deny"`. `allow-git` lists one source:
  `https://github.com/cerebral-work/pgt`.
- A research evaluation is a draft until someone sets `approved_by`. A draft
  does not justify a dependency.

### pgt

pgt is not a dependency of any template crate. The root `Cargo.toml` holds the
line below as a comment. A repo that needs pgt removes the comment mark:

```toml
pgt-core = { git = "https://github.com/cerebral-work/pgt", rev = "54d90f5" }
```

A crate that needs pgt uses a git dependency by rev. The rev is never a branch.
`make pgt-install` installs the CLI into `.tools/bin`. That directory is
gitignored. CI mints a read token only in jobs that build a crate with this
dependency. The token comes from the reverie GitHub App pattern.

The licence gap is the CTO's to settle. Until he does, nobody
redistributes pgt code or copies it into this repo.

### rig

Decision, the operator, 2026-10-08: rig is evaluated and pinned at 0.44.0 (MIT).
The template does not include it. A repo adopts it when it needs model calls.
It calls models through the LiteLLM gateway, not vendor APIs.

## Plan

- [ ] Write `deny.toml` with the licence allow list and the four denied licences.
- [ ] Add the commented pgt example to the root `Cargo.toml`.
- [ ] Add `make pgt-install`.
- [ ] Add a why-comment to every workspace dependency.
- [ ] Ask the CTO for a pgt licence statement. Track it outside this repo.

## Test

Plant a copyleft crate, then run the gate. It must fail. Remove it and run the
gate again. It must pass.

```bash
cargo deny check licenses sources
```

Passing means exit 0 on the clean tree. A GPL-3.0 crate in the tree makes it
exit non-zero. A git source other than pgt also makes it exit non-zero.

## Notes

Open question: does pgt get a licence or stay internal-only? The answer
decides whether instantiated repos may publish a crate that depends on it.

References:

- `cerebral-work/pgt@54d90f5:Cargo.toml`
- `cerebral-work/cortex@524a97b:deny.toml`
