---
status: in-progress
created: 2026-10-08
tags: [dependencies, licence, deny, pgt]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout]
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
- Every research evaluation is a draft: the 13 files under `repos/` at
  `cerebral-work/research@4068134` all have `status: "draft"`.

## Design

### Requirements (EARS)

- **Ubiquitous:** The workspace shall declare a proprietary licence (`LicenseRef-Proprietary`) and deny copyleft licences via `deny.toml`.
- **Ubiquitous:** The workspace shall pin every external dependency by exact version or git revision before use.
- **Event-driven:** WHEN adding an external dependency, the developer shall document rationale in workspace dependencies and update this specification.
- **State-driven:** WHILE building eBPF kernel probe objects, the build system shall declare dual MIT/GPL licensing for kernel compatibility.
- **Unwanted behavior:** IF a dependency introduces copyleft terms or unapproved git sources, THEN `cargo deny check` shall fail the build.

### Licence rules

- Copyleft is denied. `deny.toml` uses the cargo-deny v2 schema, which has no
  `deny` list. A licence passes only when it is in `allow`. No GPL, AGPL or
  LGPL licence is in `allow`, so all of them fail. The two named cases are
  autonice and zerostack.
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

### toml

Decision, the operator, 2026-10-09: `toml` is pinned at 0.8 (MIT OR Apache-2.0).
Used in `cas-engine` and `cas-cli` to parse `.saga/flows/*.toml` macro-orchestration
definitions and workflow templates.

### Vendored estate contracts and schemas

To integrate with other estate components without creating circular workspace
dependencies or heavy binary links, CAS vendors interface schemas and contracts:

| Component | Upstream repository | Path in CAS | Licensor / Licence | Role in CAS |
|---|---|---|---|---|
| LeanSpec | `codervisor/leanspec` | `vendor/leanspec/` | MIT | LeanSpec methodology templates and skills |
| Soma OS | `unsigned-gg/soma-os` | `vendor/soma/schemas/` | BSL 1.1 (unsigned-gg) | JSON schemas for containment and dispatch |
| Cortex Mesh | `cerebral-work/cortex` | `vendor/cortex/proto/` | Proprietary (Cerebral) | Protocol Buffers for mesh RPC and messaging |
| Saga Orchestrator | `unsigned-gg/saga` | `vendor/saga/flows/`, `vendor/saga/contracts/` | BSL 1.1 (unsigned-gg) | Workflow definitions and domain contract types |

Each directory includes a README or VENDORED note recording upstream commits, dates, and licensing terms.


### CAS dependencies from the research

The CAS research names the projects below. Date of this list: 2026-10-08.
Source for each: `cerebral-work/research@4068134`. Every research evaluation
is a draft. The Status column holds each row's state. The epic list below approves each row it pins. The rule above applies: pin and record a version here before
the first use.

| Project | Licence (per research) | Role in CAS | Status | Source |
|---|---|---|---|---|
| gitoxide (`gix`) | MIT OR Apache-2.0 | `cas-pool`: in-process worktrees and checkpoints | approved 2026-10-09 (the operator, epic list below), pinned at 0.88.0, not yet in a Cargo.toml | `repos/gitoxide.md` line 15 (licence), lines 100-111 (role) |
| autumn-harvest | MIT OR Apache-2.0 | `cas-engine`: durable workflows | evaluated, unpinned, NOT approved | `repos/autumn-harvest.md` line 15 |
| aya | MIT OR Apache-2.0 (not in the research; `aya-rs/aya` workspace `Cargo.toml`, read 2026-10-08) | `cas-ebpf`: eBPF programs and loader | approved 2026-10-09 (the operator, epic list below; spec 0014 D2), pinned at 0.14.0, in the workspace `Cargo.toml`, used by `crates/cas-ebpf/Cargo.toml` | `repos/neutral-engineering.md` section 2.1 |
| `landlock` 0.4.7 | MIT OR Apache-2.0 | `cas-sandbox`: filesystem rulesets | approved 2026-10-09, pinned, in use (spec 0010) | `rust-native-agent-substrate.md` section 3.3 |
| madstack (`pgray/madstack`: Axum, Maud, htmx) | MIT | `cas-deck`: operator dashboard | approved 2026-10-09 (the operator, epic list below) as its crate pins: axum 0.8.9, maud 0.27.0, tower-http 0.7.1, tokio-stream 0.1.19; htmx is not a crate; not yet in a Cargo.toml | `repos/madstack.md` |
| rig-core 0.44 | MIT | model calls through the LiteLLM gateway, on need | pinned at 0.44.0 by the decision above, not in the workspace | `0xPlaygrounds/rig` |

The research does not state the aya licence. The `aya-rs/aya` workspace
`Cargo.toml` declares `MIT OR Apache-2.0` (read 2026-10-08). Re-read it at the
pin. A separate question remains: an eBPF program that calls GPL-only kernel
helpers must declare a GPL-compatible licence in its `license` section (Linux
kernel `Documentation/bpf/bpf_licensing.rst`). This repo is proprietary.

Position, decided by the operator on 2026-10-09: the kernel-side BPF program
object declares `Dual MIT/GPL` in its `license` section, as the aya template
does. Only that program object carries it. The userspace loader and every
other crate stay proprietary, and `deny.toml` still denies GPL crates.

### CAS epic dependency list (approved 2026-10-09)

Status: APPROVED. the operator ruled to approve the dependency list once, up
front, for the parked epic run, and approved this whole table with its
conditions on 2026-10-09 by interview in the CAS lane (w25:p1). No crate
enters a `Cargo.toml` until the row is approved and the ticket that needs it
starts. Each pin is the newest version published on or before 2026-09-25 (14
days before research). Versions, licences and MSRV come from crates.io
metadata, read 2026-10-09.

| Crate | Version | Licence | Used by | Phase | Conditions |
|---|---|---|---|---|---|
| `landlock` | 0.4.7 | MIT OR Apache-2.0 | cas-sandbox | 2 | none |
| `rustix` | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | cas-sandbox, cas-pool | 2, 3 | default-features off; `std`, `process`, `thread`, `fs`, `mount`. In use since spec 0011 (2026-10-09) with `std`, `thread`, `process`, `fs` |
| `aya` | 0.14.0 | MIT OR Apache-2.0 | cas-ebpf (loader) | 2 | MSRV 1.87 |
| `aya-ebpf` | 0.2.1 | MIT OR Apache-2.0 | kernel-side BPF program, outside the workspace build | 2 | nightly, rust-src, bpf-linker; program object declares `Dual MIT/GPL` |
| `aya-log`, `aya-log-ebpf` | 0.3.0, 0.2.0 | MIT OR Apache-2.0 | cas-ebpf | 2 | only if the BPF program logs |
| `libc` | 0.2.169 | MIT OR Apache-2.0 | none | 2 | pinned, NOT in the workspace: `rustix` 1.1.5 (already approved) provides `setpgid` and `setpriority_pgrp` without `unsafe`, so cas-ebpf uses it instead (lane Claude, 2026-10-09) |
| `gix` | 0.88.0 | MIT OR Apache-2.0 | cas-pool | 3 | default-features off, minimal features; MSRV 1.88; fallback 0.87.1 |
| `agent-client-protocol` | 2.2.0 | Apache-2.0 | cas-acp | 3 | MSRV 1.88; 3.x is under 14 days old |
| `tokio` | 1.53.1 | MIT | cas-router, cas-deck, cas-storage | 4, 5, 6 | default-features off; `rt-multi-thread`, `macros`, `net`, `time`, `sync`. The cas-pool runtime stays open (spec 0001 gap 1). In use since spec 0013 (2026-10-09) in cas-router, plus `io-util` (a feature of the same crate, needed by `AsyncReadExt` and `AsyncWriteExt`; CAS lane amendment, the operator corrects it in place) |
| `sha2` | 0.11.0 | MIT OR Apache-2.0 | cas-ebpf, cas-router | 2, 4 | none |
| `reqwest` | 0.13.5 | MIT OR Apache-2.0 | cas-router, cas-deck | 4, 5 | default-features off; `rustls` (aws-lc-rs provider, needs cmake), `http2`, `json`, `stream` |
| `futures-util` | 0.3.34 | MIT OR Apache-2.0 | cas-router, cas-deck | 4, 5 | none |
| `axum` | 0.8.9 | MIT | cas-deck | 5 | `ws` covers WebSocket |
| `tower-http` | 0.7.1 | MIT | cas-deck | 5 | `fs`, `trace`, `timeout`, `compression-gzip` |
| `tokio-stream` | 0.1.19 | MIT | cas-deck | 5 | none |
| `maud` | 0.27.0 | MIT OR Apache-2.0 | cas-deck | 5 | `axum`; last release 2025-02-02 (stale, accepted with the MAD stack) |
| `similar` | 3.2.0 | Apache-2.0 | cas-deck | 5 | none |
| `aws-sdk-s3` | 1.150.0 | Apache-2.0 | cas-storage | 6 | default-features off; `sigv4a`, `http-1x`, `default-https-client`, `rt-tokio`, `behavior-version-latest`. The default `rustls` feature pulls the legacy hyper 0.14 / rustls 0.21 stack with 4 advisories. MSRV 1.94.1 |
| `ed25519-dalek` | 3.0.0 | BSD-3-Clause | identity | 7 | Phase 7 waits for the Director of Engineering |
| `jsonschema` | 0.58.0 | MIT | identity | 7 | default-features off (no remote `$ref`); pulls `fluent-uri` (MIT-0); Phase 7 waits for the Director of Engineering |
| `governor` | 0.10.4 | MIT | identity | 7 | Phase 7 waits for the Director of Engineering |
| `a2a-lf` | 0.3.1 | Apache-2.0 | cas-engine (A2A envelopes) | 7 | protocol conformance not verified; Phase 7 waits for the Director of Engineering |
| `criterion` | 0.8.2 | Apache-2.0 OR MIT | benches (dev only) | 3, 8 | default-features off; `cargo_bench_support` |

Not listed, with the reason:
- `syntect` (deck highlighting): pulls `bincode`, unmaintained (RUSTSEC-2025-0141).
- Phase 8 CRDT (`yrs` 0.28.0 pulls `smallstr`, unmaintained, RUSTSEC-2026-0215; `automerge` 0.12.0 needs Rust 1.90): chosen in the DeltaDB spec.
- minikv client: the only crate found is a one-author project; the ticket owner must confirm what minikv means.

Check of the whole set (`cargo deny check` with this repo's deny.toml, throwaway
project, 522 packages, 2026-10-09):
- With the aws-sdk-s3 features above: no vulnerability, no yanked crate.
- All targets: rejected licences `MIT-0` (fluent-uri, via jsonschema) and
  `CDLA-Permissive-2.0` (webpki-root-certs, wasm32 only). Linux targets only:
  `MIT-0` remains.
- Unmaintained: `bincode` (syntect) and `smallstr` (yrs). Both are excluded above.
- The set needs a Rust floor of 1.88 (aya, gix, ACP, transitive icu crates);
  aws-sdk-s3 needs 1.94.1, which equals the pinned toolchain.

### CI tools

A tool that a workflow downloads at run time is an external dependency too.
Pin it by version and sha256 and record it here.

| Tool | Version | Licence | Use | Pin |
|---|---|---|---|---|
| `gh` (cli/cli) | 2.101.0, published 2026-09-15 | MIT | `release.yml`: the release-plz action calls `gh` in its git-config step. The `ci-rust` image has no `gh`. | sha256 for `linux_amd64` and `linux_arm64` in `release.yml`. Checked 2026-10-08 against `gh_2.101.0_checksums.txt` and a local `sha256sum` of each tarball. |
| `bpf-linker` (aya-rs/bpf-linker) | 0.11.1 (tag v0.11.1) | MIT OR Apache-2.0 [verified 2026-10-10, `gh api repos/aya-rs/bpf-linker/contents?ref=v0.11.1`: `LICENSE-MIT` and `LICENSE-APACHE`; `Cargo.toml` `license = "MIT OR Apache-2.0"`] | `ebpf-live` job: installed per run by `taiki-e/install-action` (pinned by SHA, `f7e5d7c`). It is a static release binary, so no LLVM build runs. | Musl `x86_64` tarball `sha256:e058a6aecc9e65fa4c977b298a8e4b738424d7629769fd352eed409fb57e16e8` [verified 2026-10-10: our `sha256sum` of the tarball equals `x86_64_linux_musl` in the install-action manifest `manifests/bpf-linker.json` at `f7e5d7c`]. |
| Rust nightly (rustup channel) | `nightly-2026-10-06` | MIT OR Apache-2.0 [verified 2026-10-10: `COPYRIGHT.html` in the toolchain's `share/doc/rust` names MIT and Apache 2.0] | `ebpf-live` drift check and `make ebpf-build`, which build `cas-ebpf.o`. Channel in `crates/cas-ebpf/ebpf/rust-toolchain.toml` and `EBPF_TOOLCHAIN` in the Makefile. | rustc commit `ea137335b78829b4514bf1b4c16302f74fab8581` [verified 2026-10-10: `rustup run nightly-2026-10-06 rustc -vV`, `1.101.0-nightly (ea137335b 2026-10-05)`]. |

The `gh` step is temporary. Remove it when the `ci-rust` image ships `gh`.

### Copyleft exclusions

- `autonice` (neutral-engineering) is GPL-2.0. The research uses it as the
  model for `cas-ebpf`. It is reference only. Never vendor it. Never depend
  on it. Re-implement the idea on aya.
- `zerostack` is GPL-3.0. It is reference only. Never vendor it. Never depend
  on it.

Neither licence is in the `deny.toml` allow list. A dependency that pulls either one in fails
`cargo deny check`.

## Plan

- [x] Write `deny.toml` with the licence allow list. Copyleft fails by omission (v2 schema).
- [x] Add the commented pgt example to the root `Cargo.toml`.
- [x] Add `make pgt-install`.
- [x] Add a why-comment to every workspace dependency.
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
