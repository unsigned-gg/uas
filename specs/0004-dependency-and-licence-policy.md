---
status: in-progress
created: 2026-10-08
tags: [dependencies, licence, deny, pgt]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0004-dependency-and-licence-policy

## Overview

This spec sets the licence of the template and the rules for external code.

The licence position is ruled in spec 0048 D1, D2 and D10 (the operator, 2026-10-11).
The userspace crates are `Apache-2.0 OR MIT`: `[workspace.package]` sets
`license = "Apache-2.0 OR MIT"`. The BPF program package is `GPL-2.0-only OR MIT`
and its object declares `Dual MIT/GPL`. Docs and specs are `CC-BY-4.0`. The
copyright holder is `unsigned.gg`. Contact: chris@todie.io. Until 2026-10-11 the
template was proprietary (holder the operator /
cerebral.work). Cortex had drifted from that rule with `license = "MIT"` beside
a proprietary LICENSE, measured 2026-10-08.

Facts from 2026-10-08:

- `cerebral-work/pgt` is private. It has no licence file. Head is `54d90f5`.
  It uses edition 2024, so the workspace `rust-version` floor was 1.85.
  D2 supersedes that floor (2026-10-10).
- `autonice` is GPL-2.0. `zerostack` is GPL-3.0. Both are copyleft.
- `0xPlaygrounds/rig` is MIT, version 0.44.0.
- Every research evaluation is a draft: the 13 files under `repos/` at
  `cerebral-work/research@4068134` all have `status: "draft"`.

Facts from 2026-10-10, read at `origin/development` `065015a`:

- **F1 [verified]:** `agent-client-protocol` 2.2.0 pulls `futures` 0.3.34,
  `uuid` 1.28.0, `async-io` 2.6.0, `async-process` 2.5.0 and `blocking` 1.7.0
  into `uas-acp`. Command: `cargo tree -p uas-acp -e normal --locked --depth 2`.
  Each of the five is `MIT OR Apache-2.0` (`cargo metadata --locked`).
- **F2 [verified]:** `uas-acp` uses only `agent_client_protocol::schema::*`,
  has no `async` code and starts no runtime: `grep -rnE
  'async|await|block_on|spawn|agent_client_protocol::' crates/uas-acp/src`
  prints only three `schema` imports. The umbrella crate has 90 unique packages in its
  normal tree; `agent-client-protocol-schema` 1.9.1 (Apache-2.0) has 45.
  Command: `cargo tree -p <crate> -e normal --locked --prefix none | sed 's/ (\*)//'
  | sort -u | wc -l`; it counts the root crate too.
- **F3 [verified]:** declared MSRVs in the lockfile: `aya` 0.14.0 needs 1.87,
  `agent-client-protocol` 2.2.0 and `gix` 0.88.0 need 1.88, and `uuid` 1.28.0
  needs 1.89. Command: `cargo metadata --locked`, field `rust_version`.
- **F4 [verified]:** `cargo +1.88.0 check -p uas-acp --locked` exits 101:
  `uuid@1.28.0 requires rustc 1.89.0`. `cargo +1.89.0 check --workspace
  --all-targets --locked` exits 0. With resolver 2, `rust-version` does not
  steer resolution, so the lockfile sets the real floor.

## Design

### Requirements (EARS)

- **Ubiquitous:** The workspace shall declare `Apache-2.0 OR MIT` for every root package, and the BPF program package shall declare `GPL-2.0-only OR MIT` (spec 0048 D1); `deny.toml` shall still deny copyleft dependencies.
- **Ubiquitous:** The workspace shall pin every external dependency by exact version or git revision before use.
- **Event-driven:** WHEN adding an external dependency, the developer shall document rationale in workspace dependencies and update this specification.
- **State-driven:** WHILE building eBPF kernel probe objects, the build system shall declare dual MIT/GPL licensing for kernel compatibility.
- **Unwanted behavior:** IF a dependency introduces copyleft terms or unapproved git sources, THEN `cargo deny check` shall fail the build.
- **Unwanted behavior:** IF a crate that a pin table here pins, and a workspace declares directly, is not in that workspace's lockfile at the pinned version, THEN `make hygiene` shall fail (`cargo xtask pin-check`, D3). The two workspaces are the root, checked against `Cargo.lock`, and `crates/uas-ebpf/ebpf`, checked against its own `Cargo.lock`.
- **Unwanted behavior:** IF a `\| Crate \| Version \|` table row has an empty crate name, a version that is not `X.Y.Z`, or a different count of crates and versions, or a lockfile is missing, THEN `cargo xtask pin-check` shall fail and name the row or the file (D3).

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | `uas-acp` keeps the umbrella crate `agent-client-protocol` 2.2.0 for now, with its transitive pull (F1). `uas-acp` stays sync and starts no runtime (F2). The later path is the schema-only crate `agent-client-protocol-schema`: pin it here, then switch, if the pull grows at a version bump or `uas-acp` starts a runtime. | lane w25:p1, 2026-10-10 (the operator may override) |
| D2 | The workspace `rust-version` is 1.89, the highest MSRV in the lockfile (`uuid` 1.28.0, F3, F4). The wave 2 brief said 1.88 from `aya` (1.87) and ACP (1.88); F4 shows 1.88 fails. Rejected: 1.88 plus `cargo update uuid --precise <older>`, because the next `cargo update` undoes a lockfile downgrade of a transitive crate. The bump adds no crate and no licence. `toolchain-check` passes while `rust-version` is not above the pinned channel 1.94.1. | lane w25:p1, 2026-10-10 (the operator may override) |
| D3 | The pin tables bind `Cargo.lock`. `cargo xtask pin-check`, in `make hygiene`, fails when a crate in a `\| Crate \| Version \|` table that a workspace declares directly is not locked at the table version. `tokio` is pinned `=1.53.1` in the root `Cargo.toml` and re-locked: the caret `1.53.1` had let `Cargo.lock` resolve 1.53.2 since 6ec6011 [verified, at 30196f9; spec 0026 D3]. 1.53.1 is the reviewed version; a move to 1.53.2 is a reviewed upgrade that amends the row. New rows from 2026-10-10 on use an `=` requirement; older caret rows are held by the check. The check reads both lockfiles: the workspace `Cargo.lock` and `crates/uas-ebpf/ebpf/Cargo.lock` (its own workspace, aya-ebpf 0.2.1). A malformed table row or a missing lockfile fails the check; nothing is skipped in silence. Every xtask command walks up from its working directory to the first `Cargo.toml` with `[workspace]`, not from the path compiled into it: two clones that share one `CARGO_TARGET_DIR` made `cargo xtask` read the other clone [reported, critic finding, 2026-10-10]. | uas-engineer, 2026-10-10, CER-2798; foreman recommendation (the operator may override); amended for CER-2800 Known limit: a second locked copy of a declared crate at another version passes, because it normally comes from a transitive dependency at another major. |

### Licence rules

- Copyleft is denied. `deny.toml` uses the cargo-deny v2 schema, which has no
  `deny` list. A licence passes only when it is in `allow`. No GPL, AGPL or
  LGPL licence is in `allow`, so all of them fail. The two named cases are
  autonice and zerostack.
- `cargo deny check` runs in `make check` and in the CI job `cargo-deny`. `deny.toml` sets
  `[licenses.private] ignore = false`, so the gate reads this workspace's own licence too
  (spec 0048 D8).
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

### toml

Superseded 2026-10-11: the 0.8 decision of 2026-10-09 gave way to 1.1.8, the version in use; see the `toml` row of the pin table (ruled: the operator, 2026-10-11, "Ratify 1.1.8").
`xtask` also reads `deny.toml` with it for `cargo xtask advisory-check` (spec 0045 plan); the version stays 1.1.8.

### Vendored estate contracts and schemas

To integrate with other estate components without creating circular workspace
dependencies or heavy binary links, UAS uses interface schemas and contracts.
Per spec 0048 D9, `cargo xtask fetch-contracts` fetches Soma, Cortex and Saga
into the ignored `.contracts/`, each file pinned by upstream commit and sha256
in the tracked `contracts.lock`. Only `vendor/leanspec/` (MIT) stays in `vendor/`.
The table lists each source and the path it takes once fetched:

| Component | Upstream repository | Path in UAS | Licensor / Licence | Role in UAS |
|---|---|---|---|---|
| LeanSpec | `codervisor/leanspec` | `vendor/leanspec/` | MIT | LeanSpec methodology templates and skills |
| Soma OS | `unsigned-gg/soma` | `.contracts/soma/schemas/` | BSL 1.1 (unsigned-gg) | JSON schemas for containment and dispatch |
| Cortex Mesh | `cerebral-work/cortex` | `.contracts/cortex/proto/` | Proprietary (Cerebral) | Protocol Buffers for mesh RPC and messaging |
| Saga Orchestrator | `unsigned-gg/saga` | `.contracts/saga/flows/`, `.contracts/saga/contracts/` | BSL 1.1 (unsigned-gg) | Workflow definitions and domain contract types |

`contracts.lock` records the upstream commits and sha256 values; the licence column records the upstream licensing terms.


### UAS dependencies from the research

The UAS research names the projects below. Date of this list: 2026-10-08.
Source for each: `cerebral-work/research@4068134`. Every research evaluation
is a draft. The Status column holds each row's state. The epic list below approves each row it pins. The rule above applies: pin and record a version here before
the first use.

| Project | Licence (per research) | Role in UAS | Status | Source |
|---|---|---|---|---|
| gitoxide (`gix`) | MIT OR Apache-2.0 | `uas-pool`: in-process worktrees and checkpoints | approved 2026-10-09 (the operator, epic list below), pinned at 0.88.0, not yet in a Cargo.toml | `repos/gitoxide.md` line 15 (licence), lines 100-111 (role) |
| autumn-harvest | MIT OR Apache-2.0 | `uas-engine`: durable workflows | evaluated, unpinned, NOT approved | `repos/autumn-harvest.md` line 15 |
| aya | MIT OR Apache-2.0 (not in the research; `aya-rs/aya` workspace `Cargo.toml`, read 2026-10-08) | `uas-ebpf`: eBPF programs and loader | approved 2026-10-09 (the operator, epic list below; spec 0014 D2), pinned at 0.14.0, in the workspace `Cargo.toml`, used by `crates/uas-ebpf/Cargo.toml` | `repos/neutral-engineering.md` section 2.1 |
| `landlock` 0.4.7 | MIT OR Apache-2.0 | `uas-sandbox`: filesystem rulesets | approved 2026-10-09, pinned, in use (spec 0010) | `rust-native-agent-substrate.md` section 3.3 |
| `seccompiler` 0.5.0 | Apache-2.0 OR BSD-3-Clause (crates.io API, read 2026-10-11) | `uas-sandbox`: seccomp-BPF syscall filter | approved 2026-10-11, pinned, default features, no `libseccomp` link | spec 0043 D7; ruling 2026-10-11, lane w25:p1 |
| `serde_json` 1.0.151 | MIT OR Apache-2.0 (crates.io API, read 2026-10-11) | JSON encoding in `uas-acp`, `uas-cli`, `uas-deck`, `uas-ebpf`, `uas-engine` and `uas-pool`; `uas-domain` in dev-dependencies only | pinned, in use; the root `Cargo.toml` requirement is `"1"` | pre-existing use found by the PR #138 critic; `uas-pool` added it in PR #138; ruled: integrating lane w25:p1, 2026-10-11 |
| madstack (`pgray/madstack`: Axum, Maud, htmx) | MIT | `uas-deck`: operator dashboard | approved 2026-10-09 (the operator, epic list below) as its crate pins: axum 0.8.9, maud 0.27.0, tower-http 0.7.1, tokio-stream 0.1.19; htmx is not a crate; axum and maud in the root Cargo.toml since P4 (2026-10-10), tower-http and tokio-stream not yet | `repos/madstack.md` |
| rig-core 0.44 | MIT | model calls through the LiteLLM gateway, not vendor APIs; the template excludes it, a repo adopts it on need | pinned at 0.44.0, not in the workspace | `0xPlaygrounds/rig`; evaluated, decision: the operator, 2026-10-08 |
| `clap` 4.6.7 | MIT OR Apache-2.0 (`Cargo.toml` in the local registry, read 2026-10-10) | `uas-cli`, `uas-ebpf`: command-line parsing | approved 2026-10-10 for `derive` and `env`; `env` is approved by this amendment; in the workspace since before spec 0042, locked at 4.6.7 | spec 0042 D1, D2; plan 0042 P1; integrating lane w25:p1, 2026-10-10 |
| `tracing-subscriber` 0.3.23 | MIT (`Cargo.toml` in the local registry, read 2026-10-10) | `uas-cli`, `uas-ebpf`: log output | approved 2026-10-10 for `env-filter` and `json`; `json` is approved by this amendment; locked at 0.3.23 | spec 0042 D1, D2; plan 0042 P1; integrating lane w25:p1, 2026-10-10 |
| `tracing-serde` 0.2.0 | MIT (`Cargo.toml` in the local registry, read 2026-10-10) | pulled by the `json` feature of `tracing-subscriber`; no crate declares it | planned pin, approved by this amendment, not yet in `Cargo.lock` | spec 0042 D1, D2; plan 0042 P1; integrating lane w25:p1, 2026-10-10 |
| `zerocopy` 0.8.62 | BSD-2-Clause OR Apache-2.0 OR MIT (`cargo info zerocopy`, read 2026-10-11) | `uas-ebpf` only: the structs shared between the BPF object and the daemon, replacing hand transmutes | planned, no `Cargo.toml` line yet | ruled: the operator, grill sheet 2026-10-10 answer 2a, read 2026-10-11; dossier `cortex/docs/research/hyperscaler-oss-survey-2026-10-09.md` Tier 1 (sweep `microopt-libraries-2026-10-10.md` line 16) |
| `shuttle` 0.9.6 | Apache-2.0 (`cargo info shuttle`, read 2026-10-11) | dev-dependency only: the spec 0026 adapter and the `uas-pool` lease tests | planned, no `Cargo.toml` line yet | ruled: the operator, grill sheet 2026-10-10 answer 3a, read 2026-10-11; same dossier, Tier 1 |
| `bytes` 1.12.1 | MIT (`cargo info bytes`, read 2026-10-11) | `uas-acp` only, on the `MAX_FRAME_BYTES` path | conditional on a frame-copy measurement; not pinned | ruled: the operator, grill sheet 2026-10-10 answer 8a, read 2026-10-11; `repos/tokio-rs.md:72-73,133` |

### Considered, no uas pin (grill sheet 2026-10-10)

Each row: ruled: the operator, grill sheet 2026-10-10 answer <n>, read 2026-10-11. Principles (answer 1a): stars are a weak signal and manifests are the source of truth, in `cerebral-work/cerebral-docs` `agentic-systems/guides/hyperscaler-library-selection-principles.md` at 39b0919 [reported: research lane w1X:p1, mesh 2026-10-11; commit verified on GitHub by w25:p1]. The commit touches 23 files: it adds that guide, changes one line of `linux-agent-runtime-control-taxonomy.md` and adds a `Verdict` line to each of 21 `repos/*.md` dossiers [verified 2026-10-11: `gh api repos/cerebral-work/cerebral-docs/commits/39b0919`].

| Library | Answer | Verdict |
|---|---|---|
| `nom`, `rkyv` | 8a | Rejected: serde JSON-RPC is the contract (`communication/designs/rust-subsystem-crate-topology.md:23,53`; `agentic-systems/findings/marathon-crdt-p2p-and-hyperlight-vmm.md:25`) |
| `mmap-sync`, SHM transport | 4a | Cortex spec 0009 owns it; no uas pin. rustix `net` and `mm` features wait for a uas spec that needs them |
| `shellflip` | 5a | Watch. Adopt only when `uas-ebpfd` or the deck needs zero-downtime restart with fd passing |
| `foundations` | 6a | Rejected: uas owns its seccomp and landlock jails |
| io_uring, `tokio-uring`, `heph` | 7a | Closed by spec 0019 D2 (OS threads) |
| `kube-rs`, `rust-spiffe` | 9c | Not for uas. the operator's candidate is saga on top of uas, relayed to the saga lead [reported: w25:p1 mesh message 07da62cd, 2026-10-11] |
| `parseable`, `pingora`, `wirefilter`, `lol-html` | none | Not for uas (paas surfaces). lane-decided: w25:p1, 2026-10-11, on 9a's reasoning; not ruled |

The research dossier pass and its two citation errors are with the research lane (answer 10a).

The aya licence is not in the research; the `aya-rs/aya` workspace declares `MIT OR Apache-2.0` (read 2026-10-08), to be re-read at the pin. An eBPF program that calls GPL-only kernel helpers must declare a GPL-compatible licence (`Documentation/bpf/bpf_licensing.rst`). the operator, 2026-10-09: the BPF program object declares `Dual MIT/GPL` in its `license` section, as the aya template does; only that object carries it. Amended 2026-10-11 by spec 0048 D1 (the operator): the workspace and the userspace loader are `Apache-2.0 OR MIT`, the program package is `GPL-2.0-only OR MIT`, and `deny.toml` still denies GPL crates.

### UAS epic dependency list (approved 2026-10-09)

Status: APPROVED. the operator ruled to approve the dependency list once, up
front, for the parked epic run, and approved this whole table with its
conditions on 2026-10-09 by interview in the UAS lane (w25:p1). No crate
enters a `Cargo.toml` until the row is approved and the ticket that needs it
starts. Each pin is the newest version published on or before 2026-09-25 (14
days before research). Versions, licences and MSRV come from crates.io
metadata, read 2026-10-09.

| Crate | Version | Licence | Used by | Phase | Conditions |
|---|---|---|---|---|---|
| `landlock` | 0.4.7 | MIT OR Apache-2.0 | uas-sandbox | 2 | none |
| `rustix` | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | uas-sandbox, uas-pool | 2, 3 | default-features off; `std`, `process`, `thread`, `fs`, `mount`. In use since spec 0011 (2026-10-09) with `std`, `thread`, `process`, `fs` |
| `aya` | 0.14.0 | MIT OR Apache-2.0 | uas-ebpf (loader) | 2 | MSRV 1.87 |
| `aya-ebpf` | 0.2.1 | MIT OR Apache-2.0 | kernel-side BPF program, outside the workspace build | 2 | nightly, rust-src, bpf-linker; program package is `GPL-2.0-only OR MIT`; the object still declares `Dual MIT/GPL` |
| `aya-log`, `aya-log-ebpf` | 0.3.0, 0.2.0 | MIT OR Apache-2.0 | uas-ebpf | 2 | only if the BPF program logs |
| `seccompiler` | 0.5.0 | Apache-2.0 OR BSD-3-Clause | uas-sandbox | 2 | default features; no `libseccomp` link (spec 0043 D7; ruling 2026-10-11, lane w25:p1) |
| `libc` | 0.2.190 | MIT OR Apache-2.0 | uas-sandbox | 2 | direct dependency: seccompiler keys rules by syscall number and rustix exports none (plan 0043 R1). Locked at 0.2.190 (spec 0043 D7; ruling 2026-10-11, lane w25:p1) |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 | uas-acp, uas-cli, uas-deck, uas-ebpf, uas-engine, uas-pool; uas-domain (dev only) | in use | root requirement `"1"`, a caret held by `pin-check` (D3). Pre-existing use found by the PR #138 critic; `uas-pool` added it in PR #138 (ruled: integrating lane w25:p1, 2026-10-11) |
| `toml` | 1.1.8+spec-1.1.0 | MIT OR Apache-2.0 | uas-cli, uas-engine, xtask | in use | root requirement `"1.1"`, a caret held by `pin-check` (D3); the cell carries the lock's build metadata because pin-check compares strings. Ratified 2026-10-11 (ruled: the operator, 2026-10-11, lane w25:p1 (AskUserQuestion), "Ratify 1.1.8"), superseding the 0.8 decision of 2026-10-09. Parses `.saga/flows/*.toml` and, for `cargo xtask advisory-check`, `deny.toml` (spec 0045) |
| `gix` | 0.88.0 | MIT OR Apache-2.0 | uas-pool | 3 | default-features off, minimal features; MSRV 1.88; fallback 0.87.1 |
| `agent-client-protocol` | 2.2.0 | Apache-2.0 | uas-acp | 3 | MSRV 1.88; 3.x is under 14 days old. In use since spec 0016 (2026-10-10); transitive pull in D1 |
| `tokio` | 1.53.1 | MIT | uas-router, uas-deck, uas-storage; uas-engine behind feature `cortex-grpc` (spec 0026; UAS lane amendment, the operator corrects it in place) | 4, 5, 6 | default-features off; `rt-multi-thread`, `macros`, `net`, `time`, `sync`. The uas-pool runtime stays open (spec 0001 gap 1). In use since spec 0013 (2026-10-09) in uas-router, plus `io-util` (a feature of the same crate, needed by `AsyncReadExt` and `AsyncWriteExt`; UAS lane amendment, the operator corrects it in place) |
| `sha2` | 0.11.0 | MIT OR Apache-2.0 | uas-ebpf, uas-router, xtask | 2, 4 | none |
| `reqwest` | 0.13.5 | MIT OR Apache-2.0 | uas-router, uas-deck | 4, 5 | default-features off; `rustls` (aws-lc-rs provider, needs cmake), `http2`, `json`, `stream` |
| `futures-util` | 0.3.34 | MIT OR Apache-2.0 | uas-router, uas-deck | 4, 5 | none |
| `axum` | 0.8.9 | MIT | uas-deck | 5 | default features off, `http1` and `tokio` (P4, 2026-10-10); `ws` waits for D2 |
| `tower-http` | 0.7.1 | MIT | uas-deck | 5 | `fs`, `trace`, `timeout`, `compression-gzip` |
| `tokio-stream` | 0.1.19 | MIT | uas-deck | 5 | none |
| `maud` | 0.27.0 | MIT OR Apache-2.0 | uas-deck | 5 | `axum`; last release 2025-02-02 (stale, accepted with the MAD stack) |
| `similar` | 3.2.0 | Apache-2.0 | uas-deck | 5 | none |
| `aws-sdk-s3` | 1.150.0 | Apache-2.0 | uas-storage | 6 | default-features off; `sigv4a`, `http-1x`, `default-https-client`, `rt-tokio`, `behavior-version-latest`. The default `rustls` feature pulls the legacy hyper 0.14 / rustls 0.21 stack with 4 advisories. MSRV 1.94.1 |
| `ed25519-dalek` | 3.0.0 | BSD-3-Clause | identity | 7 | Phase 7 waits for the Director of Engineering |
| `jsonschema` | 0.58.0 | MIT | identity | 7 | default-features off (no remote `$ref`); pulls `fluent-uri` (MIT-0); Phase 7 waits for the Director of Engineering |
| `governor` | 0.10.4 | MIT | identity | 7 | Phase 7 waits for the Director of Engineering |
| `a2a-lf` | 0.3.1 | Apache-2.0 | uas-engine (A2A envelopes) | 7 | protocol conformance not verified; Phase 7 waits for the Director of Engineering |
| `criterion` | 0.8.2 | Apache-2.0 OR MIT | benches (dev only) | 3, 8 | default-features off; `cargo_bench_support` |
| `clap` | 4.6.7 | MIT OR Apache-2.0 | uas-cli, uas-ebpf | 2 | `derive`, `env`; `env` approved by this amendment (spec 0042 D1, D2; plan 0042 P1; integrating lane w25:p1, 2026-10-10) |
| `tracing-subscriber` | 0.3.23 | MIT | uas-cli, uas-ebpf | 2 | `env-filter`, `json`; `json` approved by this amendment (spec 0042 D1, D2; plan 0042 P1; integrating lane w25:p1, 2026-10-10) |
| `tracing-serde` | 0.2.0 | MIT | none (transitive) | 2 | planned pin: pulled by the `json` feature of `tracing-subscriber`; not yet in `Cargo.lock`; approved by this amendment (spec 0042 D2; plan 0042 P1; integrating lane w25:p1, 2026-10-10) |

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
  aws-sdk-s3 needs 1.94.1, which equals the pinned toolchain. Amended
  2026-10-10: this repo's lockfile needs 1.89 (`uuid` 1.28.0, F3, D2).

### Cortex pool-member transport (spec 0026)

Status: pinned by the lane (uas-engineer w10:pM), 2026-10-10. This is not the
approved epic list above. No crate enters a `Cargo.toml` until spec 0026 P1 starts.
the operator ruled 0026 D1 [reported, 2026-10-10]: `tonic` and `prost` 0.14 go here; the other
rows are lane pins (0026 D3). Each row enters a `Cargo.toml` with an `=` requirement (D3). The versions are the ones
cortex@c7fdb85 uses (0026 F6). Licences read from each crate's `Cargo.toml` in the
local cargo registry, 2026-10-10. MSRV of the tonic crates is 1.88, under D2's 1.89.

| Crate | Version | Licence | Used by | Conditions |
|---|---|---|---|---|
| `tonic` | 0.14.6 | MIT | uas-engine, feature `cortex-grpc` | default-features off; `channel`, `codegen`; `server` in dev-dependencies only |
| `tonic-prost` | 0.14.6 | MIT | uas-engine, feature `cortex-grpc` | runtime glue for the generated code |
| `prost` | 0.14.4 | Apache-2.0 | uas-engine, feature `cortex-grpc` | none |
| `prost-types` | 0.14.4 | Apache-2.0 | uas-engine, feature `cortex-grpc` | `Struct` and `Timestamp` in `lane.proto` and `peer.proto`; MSRV 1.85 |
| `tonic-prost-build` | 0.14.6 | MIT | uas-engine `build.rs` | default-features off; optional build-dependency, enabled by `cortex-grpc` |
| `protox` | 0.9.1 | MIT OR Apache-2.0 | uas-engine `build.rs` | optional build-dependency, enabled by `cortex-grpc`; no system `protoc`; 0.10.0 not taken (0026 D3) |

`build.rs` does nothing unless `CARGO_FEATURE_CORTEX_GRPC` is set, so a build
without the feature pulls none of these crates. Check [verified, 2026-10-10T16:26Z]:
a throwaway project with these pins, `tokio` 1.53.1 (features as its row) and tonic
`server` in dev ran `cargo deny check` with this repo's `deny.toml`: "advisories ok,
bans ok, licenses ok, sources ok", 106 packages.
Warnings: duplicate `hashbrown`, `logos` (3 crates) and `syn`, all through `protox`.

### CI tools

A tool that a workflow downloads at run time is an external dependency too.
Pin it by version and sha256 and record it here.

| Tool | Version | Licence | Use | Pin |
|---|---|---|---|---|
| `gh` (cli/cli) | 2.101.0, published 2026-09-15 | MIT | `release.yml`: the release-plz action calls `gh` in its git-config step. The `ci-rust` image has no `gh`. | sha256 for `linux_amd64` and `linux_arm64` in `release.yml`. Checked 2026-10-08 against `gh_2.101.0_checksums.txt` and a local `sha256sum` of each tarball. |
| `cargo-audit` | 0.22.1, published 2026-02-04 | Apache-2.0 OR MIT [verified 2026-10-11: `cargo info cargo-audit@0.22.1`] | the weekly advisory detector of spec 0045 D1: `.github/workflows/audit.yml` and `make audit`. | sha256 for `cargo-audit-x86_64-unknown-linux-gnu-v0.22.1.tgz` `1890badd5f15831a9af4b074399fcd21e6f7c0fe42c84e9254cdffc9f813765c`, rustsec/rustsec tag `cargo-audit/v0.22.1` [verified 2026-10-11: the release API digest and a local `sha256sum` of the downloaded tarball match]. No workflow checks this sha256 yet. `audit.yml` installs through `taiki-e/install-action` at `f7e5d7c` and names no version. That action's manifest resolves `latest` to 0.22.2 [verified 2026-10-11: `manifests/cargo-audit.json` at `f7e5d7c`]. Spec 0045 owns the install path. Cited: spec 0045 D1. |
| `bpf-linker` (aya-rs/bpf-linker) | 0.11.1 (tag v0.11.1) | MIT OR Apache-2.0 [verified 2026-10-10, `gh api repos/aya-rs/bpf-linker/contents?ref=v0.11.1`: `LICENSE-MIT` and `LICENSE-APACHE`; `Cargo.toml` `license = "MIT OR Apache-2.0"`] | `ebpf-live` job: installed per run by `taiki-e/install-action` (pinned by SHA, `f7e5d7c`). It is a static release binary, so no LLVM build runs. | Musl `x86_64` tarball `sha256:e058a6aecc9e65fa4c977b298a8e4b738424d7629769fd352eed409fb57e16e8` [verified 2026-10-10: our `sha256sum` of the tarball equals `x86_64_linux_musl` in the install-action manifest `manifests/bpf-linker.json` at `f7e5d7c`]. |
| Rust nightly (rustup channel) | `nightly-2026-10-06` | MIT OR Apache-2.0 [verified 2026-10-10: `COPYRIGHT.html` in the toolchain's `share/doc/rust` names MIT and Apache 2.0] | `ebpf-live` drift check and `make ebpf-build`, which build `uas-ebpf.o`. Channel in `crates/uas-ebpf/ebpf/rust-toolchain.toml` and `EBPF_TOOLCHAIN` in the Makefile. | rustc commit `ea137335b78829b4514bf1b4c16302f74fab8581` [verified 2026-10-10: `rustup run nightly-2026-10-06 rustc -vV`, `1.101.0-nightly (ea137335b 2026-10-05)`]. |
| `cargo-semver-checks` (obi1kenobi) | 0.51.0 | Apache-2.0 OR MIT (`cargo info`, read 2026-10-11) | the semver gate of spec 0048 D13: pull-request CI on each published crate | sha256 for `x86_64-unknown-linux-gnu.tar.gz` `cacadafaa5cd27ae6d05a74a103266917ebab4c5e43c898360e177d4fbf06a65`, published 2026-10-03 [verified 2026-10-11: the release page digest for v0.51.0 and a local `sha256sum` of the downloaded tarball match]. Used by `.github/workflows/semver.yml`. Ruled: the operator, lane w25:p1, 2026-10-11 (AskUserQuestion), "RELEASING.md, pre-1.0 semver, cargo-semver-checks gate" |
| `gitleaks` | 8.30.1, published 2026-03-21 | MIT [verified 2026-10-11: the `LICENSE` file of gitleaks/gitleaks at tag `v8.30.1`, read with `gh api repos/gitleaks/gitleaks/contents/LICENSE?ref=v8.30.1`, is the MIT License] | the secret scan: `.github/workflows/ci.yml` job `secrets` and the `lefthook.yml` pre-commit hook. `ci.yml` pins it; the `lefthook.yml` install message names it; the hook does not check the binary (spec 0055 D14). | sha256 for `gitleaks_8.30.1_linux_x64.tar.gz` `551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb` [verified 2026-10-11: the release API digest for v8.30.1 and a local `sha256sum` of the downloaded tarball match]. It equals `GITLEAKS_SHA256` in `ci.yml`, which checks it with `sha256sum -c`. Cited: spec 0055 D14. |
| `cargo-cyclonedx` | 0.5.9, published 2026-03-19 | Apache-2.0 [verified 2026-10-11: `cargo info cargo-cyclonedx@0.5.9`] | the SBOM: `make sbom` and `.github/workflows/sbom.yml`. Format and spec version are spec 0045 D4. | sha256 for `cargo-cyclonedx-x86_64-unknown-linux-gnu.tar.xz` `fb8dbee9f182173e062a64a387b21a0badc6fab8b2abf9294973f012972bf6d8`, CycloneDX/cyclonedx-rust-cargo tag `cargo-cyclonedx-0.5.9` [verified 2026-10-11: the release API digest and a local `sha256sum` of the downloaded tarball match]. No workflow checks this sha256 yet. `sbom.yml` installs through `taiki-e/install-action` at `f7e5d7c` and names no version. That action's manifest resolves `latest` to 0.5.9 and fetches the musl tarball, sha256 `9bd3e599314f50810c9d98b8b68a617ff9d3cc20873968d90b29d121f6b226ff` [verified 2026-10-11: `manifests/cargo-cyclonedx.json` at `f7e5d7c`, the release API digest and a local `sha256sum` match]. Spec 0045 owns the install path. Cited: specs 0045 D4 and 0055 D8. |

The `gh` step is temporary. Remove it when the `ci-rust` image ships `gh`.

### Copyleft exclusions

- `autonice` (neutral-engineering) is GPL-2.0. The research uses it as the
  model for `uas-ebpf`. It is reference only. Never vendor it. Never depend
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

D2 check: both commands exit 0. The control `cargo +1.88.0 check -p uas-acp
--locked` exits non-zero while `uuid` 1.28.0 is in the lockfile.

```bash
cargo +1.89.0 check --workspace --all-targets --locked -j 2
make toolchain-check
```

## Notes

Open question: does pgt get a licence or stay internal-only? The answer decides whether instantiated repos may publish a crate that depends on it.

References: `cerebral-work/pgt@54d90f5:Cargo.toml`; `cerebral-work/cortex@524a97b:deny.toml`.
