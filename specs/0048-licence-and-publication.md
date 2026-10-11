---
status: in-progress
created: 2026-10-11
tags: [licence, publication, ebpf, crates-io, dco, export]
priority: high
depends_on: [0003-branch-model-and-promotion, 0004-dependency-and-licence-policy, 0005-ci-cache-and-runners, 0037-repository-migration-to-uas, 0045-security-disclosure-and-advisory-policy]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0048-licence-and-publication

> **Status**: in-progress, **Priority**: high, **Created**: 2026-10-11

## Overview

the operator framed uas as "the containerd of agent harnesses" and ruled its licence. The userspace
crates publish under `Apache-2.0 OR MIT`. The kernel-side BPF program sources and object stay
`Dual MIT/GPL`, SPDX `GPL-2.0-only OR MIT` [reported: ruling in lane w25:p1, 2026-10-11 about
00:35Z, relayed in the integrating lane's brief]. About five minutes later he ruled the route: "ok
let's keep it private for a while longer but let's build it to be an open source repo"
[reported: lane w25:p1, 2026-10-11 about 00:40Z, relayed verbatim]. The repository stays private
[reported: cortex lane w10:p0 relay of the operator, mesh ead30203, 2026-10-10 about 23:05Z], and it
is built to open-source standard now. At about 01:05Z he ruled four more points: replace the
vendored contracts with a pinned fetch now and publish a fresh repository later (D9), the
copyright holder `unsigned.gg` (D2), CC-BY-4.0 for docs and specs (D10), and how `main`'s stray
commit is absorbed (Notes) [reported: lane w25:p1, 2026-10-11, relayed].

Today the workspace is proprietary and spec 0004 requires that. The crates.io placeholder already
says `MIT OR Apache-2.0` (spec 0045 D8). This spec sets the licence layout, the kernel boundary,
the open-source build-out of this repository, the contribution rule, and the vendored private
contracts.

### Facts

Measured 2026-10-11 on ceres at development `340475b`.

- **F1 [verified]:** `LICENSE` has 16 lines. Line 1 reads "Copyright (c) 2026 unsigned.gg. All rights reserved."; line 3 reads "PROPRIETARY SOFTWARE LICENSE". GitHub reports the licence as `NOASSERTION`. Commands: `head -3 LICENSE; wc -l LICENSE`; `gh api repos/unsigned-gg/uas --jq .license.spdx_id`.
- **F2 [verified]:** `Cargo.toml` `[workspace.package]` sets `publish = false` and `license = "LicenseRef-Proprietary"` (lines 11 and 12). The members are `crates/*` and `xtask`: 11 packages. Each sets `license.workspace = true` and `publish.workspace = true`; none sets its own licence. Command: `grep -E '^(name|license|publish)' crates/*/Cargo.toml xtask/Cargo.toml`.
- **F3 [verified]:** The BPF program package `uas-ebpf-ebpf` is its own workspace. `crates/uas-ebpf/ebpf/Cargo.toml` has a `[workspace]` table and its own `Cargo.lock`, and the root glob `crates/*` does not reach it. It sets `publish = false` and `license = "MIT OR Apache-2.0"`. That disagrees with its object (F4) and with spec 0004 (F8). Command: `cat crates/uas-ebpf/ebpf/Cargo.toml`.
- **F4 [verified]:** `crates/uas-ebpf/ebpf/src/main.rs` lines 328 to 330 put `static LICENSE: [u8; 13] = *b"Dual MIT/GPL\0"` in the `license` link section. The committed object has a 13-byte `license` section (`objdump -h crates/uas-ebpf/ebpf/uas-ebpf.o`), and `strings crates/uas-ebpf/ebpf/uas-ebpf.o | grep -i gpl` prints `Dual MIT/GPL`. Linux v6.6 `Documentation/process/license-rules.rst` lines 456 to 458 read the string as "dual licensed under a GPL v2 variant or MIT license choice". `GPL-2.0-only OR MIT` is the ruled SPDX form. `include/linux/license.h` lists the string as GPL-compatible.
- **F5 [verified]:** By name, the program calls `bpf_get_current_comm`, `bpf_get_current_pid_tgid`, `bpf_get_current_uid_gid`, `bpf_get_ns_current_pid_tgid`, `bpf_ktime_get_ns` and `bpf_probe_read_kernel_str_bytes` (`grep -oE '\bbpf_[a-z_]+' crates/uas-ebpf/ebpf/src/main.rs | sort | uniq -c`). That grep misses `ctx.read_at` (lines 243 and 285), which calls aya-ebpf 0.2.1's `bpf_probe_read_kernel` (`src/programs/tracepoint.rs` line 19, `src/helpers.rs` line 211, cargo registry); `bpf_probe_read_kernel_str_bytes` calls `bpf_probe_read_kernel_str` (`src/helpers.rs` line 558). The object settles the list: `llvm-objdump -d crates/uas-ebpf/ebpf/uas-ebpf.o | grep -oE 'call 0x[0-9a-f]+' | sort | uniq -c`, with the 1.94.1 toolchain's llvm-tools, prints helper ids 1, 2, 3, 5, 14, 15, 16, 113, 115, 120, 131 and 132. v6.6 `include/uapi/linux/bpf.h` (lines 5648 to 5779) names them map lookup, update and delete, `ktime_get_ns`, `get_current_pid_tgid`, `get_current_uid_gid`, `get_current_comm`, `probe_read_kernel`, `probe_read_kernel_str`, `get_ns_current_pid_tgid`, `ringbuf_reserve` and `ringbuf_submit`. In Linux v6.6 (ceres runs 6.6.87), two are GPL-only: `bpf_probe_read_kernel` (`.gpl_only = true` at `kernel/trace/bpf_trace.c` line 238; id 113, called twice) and `bpf_probe_read_kernel_str` (line 273; id 115, called once). `bpf_ktime_get_ns` is `.gpl_only = false` (`kernel/bpf/helpers.c` line 178), as are the comm, pid_tgid, uid_gid and ns pid helpers (lines 270, 229, 249, 617) and the three map helpers (lines 45, 61, 78). The ring buffer protos set no flag. The verifier rejects a GPL-only call from a non-GPL program (`kernel/bpf/verifier.c` lines 9558 to 9562). The kernel files came from `raw.githubusercontent.com/torvalds/linux/v6.6/`; per helper: `grep -n -A3 '<helper>_proto = {' <file> | grep gpl_only`. An earlier version of this fact named one GPL-only helper; the critic round found `read_at`.
- **F6 [verified]:** The program holds three tracepoint programs: `#[tracepoint]` at lines 214, 267 and 302, for `sched_process_exec`, `sched_process_fork` and `sched_process_exit`. It holds no LSM, struct_ops or kfunc program. `kernel/bpf/bpf_lsm.c` lines 100 to 103 reject an LSM program whose licence is not GPL-compatible. `Documentation/bpf/bpf_licensing.rst` says LSM and struct_ops programs, and programs that call a kfunc, must be GPL-compatible even when they call no GPL-only helper.
- **F7 [verified]:** `crates/uas-ebpf/src/lib.rs` line 34 embeds the object: `pub const BPF_OBJECT: &[u8] = include_bytes!("../ebpf/uas-ebpf.o")`. `crates/uas-ebpf/src/loader.rs` line 64 passes it to `Ebpf::load`. So the userspace binary carries the object as bytes, and the kernel loads it through `bpf(2)` at run time. `bpf_licensing.rst` says proprietary applications and GPL BPF programs "in the same package can co-exist because they are separate executable processes". `cargo package --list -p uas-ebpf` lists no file under `ebpf/`. Cargo skips a subdirectory that holds a `Cargo.toml` [reported: Cargo Book, manifest `include` section], so `uas-ebpf` cannot be published as it stands.
- **F8 [verified]:** Spec 0004 line 55 requires `LicenseRef-Proprietary`. Line 58 requires dual MIT/GPL for BPF objects. Lines 159 to 162 record the operator's 2026-10-09 position: the program object declares `Dual MIT/GPL`, and the loader and every other crate stay proprietary. Line 179 pins `aya-ebpf` 0.2.1 with that note. Command: `grep -n -E 'Dual MIT/GPL|[Pp]roprietary' specs/0004-*.md`.
- **F9 [verified]:** Licences on the kernel path. `aya` 0.14.0, `aya-obj` 0.3.0, `landlock` 0.4.7 and `libc` 0.2.190 are `MIT OR Apache-2.0`. `rustix` 1.1.5 is `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`. Command: `cargo metadata --format-version 1 --locked`. `aya-ebpf` 0.2.1 is `MIT OR Apache-2.0` (the same command in `crates/uas-ebpf/ebpf`). From the crates.io API (`/api/v1/crates/<name>`): `seccompiler` 0.5.0 is `Apache-2.0 OR BSD-3-Clause`; it is not in `Cargo.lock` at `340475b` and is locked at 0.5.0 on `2399dda`, where libbpf is still absent; `bpf-linker` 0.11.1, a build tool and not a dependency, is `MIT OR Apache-2.0`. `grep -rn libbpf Cargo.lock crates/uas-ebpf/ebpf/Cargo.lock` prints nothing; control: `grep -c '^name = "aya"' Cargo.lock` prints 1.
- **F10 [verified]:** The `deny.toml` allow list has MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Zlib and CC0-1.0, and no GPL. `[licenses.private]` sets `ignore = true`, with the comment "This repo is proprietary and unpublished. Do not check its own licence." `make deny` runs `cargo deny check` at the root only (Makefile line 41). The table below was measured with cargo-deny 0.19.0 on a `git archive` copy of `340475b`, by `cargo deny --offline check licenses`. No allow-list change is needed: an `OR` expression passes on an allowed arm.

| Copy | Exit | Result |
|---|---|---|
| today, `ignore = false` | 4 | 11 `error[rejected]` on `LicenseRef-Proprietary` |
| workspace `Apache-2.0 OR MIT`, `ignore = false` | 0 | `licenses ok` |
| the same, one crate `GPL-2.0-only` | 4 | `licenses FAILED` |
| the same, `ignore = true` | 0 | `licenses ok`: the GPL crate passes unseen |
| BPF workspace `GPL-2.0-only OR MIT`, `--manifest-path` | 0 | `licenses ok` |
| BPF workspace `GPL-2.0-only` | 4 | `licenses FAILED` |

- **F11 [verified]:** No `LICENSE-APACHE`, `LICENSE-MIT` or `NOTICE` exists. The tracked licence files are `LICENSE`, `vendor/leanspec/LICENSE` and `docs/IP_NOTICE.md` (`git ls-files | grep -iE 'licen[cs]e|notice|copying'`). An earlier version of this fact used `(^|/)notice`, which misses `IP_NOTICE.md` (F19). No tracked `.rs` or `.sh` file outside `vendor/` carries `SPDX-License-Identifier`: `git ls-files -z -- '*.rs' '*.sh' ':!vendor' | xargs -0 grep -L 'SPDX-License-Identifier' | wc -l` prints 121 at `340475b` and 127 at `07d0b52`, after development was merged into this branch.
- **F12 [verified]:** `vendor/` holds four directories. Their READMEs state: `leanspec` MIT (codervisor, with `LICENSE`); `cortex` "Proprietary (Cerebral)"; `soma` and `saga` "Business Source License 1.1 (unsigned-gg)". Cortex, soma and saga entered history in `7c17f69` on 2026-10-08 (`git log --diff-filter=A -- vendor/cortex vendor/soma vendor/saga`). Only `crates/uas-engine/build.rs` line 9 reads one of them, `vendor/cortex/proto`, and only under the feature `cortex-grpc`. `make check` builds default features (Makefile lines 23 to 38). Command: `git grep -n -E 'vendor/(cortex|soma|saga)' -- crates xtask Makefile .github`; at `7d97fa2` it still finds only that line. The READMEs pin the upstream commits: cortex `a63d858`, soma `1558012`, saga `1e4be86`.
- **F13 [verified]:** crates.io `uas` 0.1.0 has licence `MIT OR Apache-2.0` and repository `https://github.com/unsigned-gg/uas`. Command: `curl -s https://crates.io/api/v1/crates/uas | jq -r '.versions[0].license, .crate.repository'`. Anonymous `curl -s -o /dev/null -w '%{http_code}'` on that URL prints 404; control: `https://github.com/containerd/containerd` prints 200. That expression has the same two arms as the ruled `Apache-2.0 OR MIT`. Spec 0045 F10 records the rest.
- **F14 [verified]:** `gh api repos/unsigned-gg/uas --jq .private` prints `true`. `git ls-remote origin 'refs/pull/*/head' | wc -l` prints 139 at 00:42Z and 142 at 00:54Z; the relay counted 125 at 23:05Z. The regex `[a-z0-9-]+\.tail[0-9a-f]+\.ts\.net` matches one line each: the OTLP endpoint at `blackwall.toml` line 32, spec 0013 line 38 and spec 0017 line 27 (`git grep -c -E <regex>`). `docs/migration/` holds 10 files from spec 0037: settings exports, `cutover.md` and `uas-impact-list.md`. Every pull-request job in `ci.yml`, `codeql.yml` and `labeler.yml` runs on the self-hosted group `ent-default` behind the guard `github.event.pull_request.head.repo.full_name == github.repository`, which skips fork pull requests (`grep -n -E 'runs-on|head.repo.full_name' .github/workflows/*.yml`).
- **F15 [verified]:** containerd is `Apache-2.0` (`gh api repos/containerd/containerd --jq .license.spdx_id`). Its contribution guide, `containerd/project` `CONTRIBUTING.md`, asks for a `Signed-off-by:` line on every commit under the Developer Certificate of Origin (DCO, line 189). `grep -n -w CLA` on that file prints nothing; control: `grep -n 'Developer Certificate'` prints line 189.
- **F16 [verified]:** The pattern `\.ts\.net|/(home|Users)/[a-z]+/|/roo(t)/|~/(projects|\.agents|\.claude|vault|\.local|\.config|keys)|<surfaces>(s)` matches 45 lines in 25 tracked files at `cf45219` and again at `07d0b52`, none under `vendor/` (`git grep -l -E <pattern> | wc -l`; `git grep -n -E <pattern> | wc -l`). By part: home paths 10 lines in 8 files, home-relative paths 19 in 14, the research surfaces 13 in 10, hostnames 3 in 3. The files include specs, `.claude/agents/`, `.github/CODEOWNERS`, `blackwall.toml`, `.agents/skills/custody-run/SKILL.md` and `crates/uas-ebpf/uas-ebpfd.service` line 6 (`Documentation=` with a home path). Every home-path match is an operator workstation path, not a test fixture (`git grep -n -o -E '/home/[a-z]+/[^ ]{0,30}'`). Paths under `/Users` and `/root` match nothing today; the trailing slash spares spec 0010 line 108, which names `/root` as a directory. The pattern is written so it does not match itself.
- **F17 [verified]:** With an allow list of only `MIT`, `Apache-2.0` and `BSD-3-Clause`, on the F10 copy with the D1 licences, `cargo deny --offline check licenses` exits 4 with 4 rejections: Zlib for `foldhash` 0.1.5 and 0.2.0 and `zlib-rs` 0.6.8, and `(MIT OR Apache-2.0) AND Unicode-3.0` for `unicode-ident` 1.0.26. The BPF workspace exits 4 on `unicode-ident`.
- **F18 [verified]:** `CONTRIBUTING.md` (45 lines) names no DCO: `grep -c -i -E 'signed-off|DCO|certificate of origin' CONTRIBUTING.md` prints 0; control: `grep -c -i commit CONTRIBUTING.md` prints 2. `SECURITY.md` exists; spec 0045 owns its text.
- **F19 [verified]:** Measured at `7d97fa2`. `git grep -n -E 'the operator M\. Todie / cerebral\.work|All rights reserved' -- ':!specs' ':!vendor' ':!engineering' ':!CHANGELOG.md'` prints `LICENSE` line 1 and `docs/IP_NOTICE.md` lines 5 and 7. `docs/IP_NOTICE.md` (31 lines, added in `3a2e2d0` on 2026-10-08) calls the repository "proprietary and confidential". `Cargo.toml` line 13 sets `authors = ["the operator <chris@todie.io>"]`, and the 11 root packages inherit it with `authors.workspace = true`; the BPF package sets no `authors` (`git grep -n -E '^(authors|license)' -- '*Cargo.toml'`).
- **F20 [verified]:** Measured at `7d97fa2`. `git ls-files 'docs/*.md' 'specs/*.md' 'engineering/*.md'` lists 65 files (9, 48 and 8), and none carries `SPDX-License-Identifier: CC-BY-4.0` in its first ten lines. spec-check finds the H1 anywhere in the body (`xtask/src/spec_check.rs` lines 320 to 340). On a `git archive` copy with `<!-- SPDX-License-Identifier: CC-BY-4.0 -->` on the line after each spec's frontmatter, `cargo xtask spec-check` passes; spec 0025 then reaches 301 lines and warns.
- **F21 [verified]:** `b220833` merges PR #119 (`docs/spec-0038-worker-slot-limits`) into `main` at 2026-10-10T23:30:10Z (`gh pr view 119 --json baseRefName,mergedAt`). It is an ancestor of `origin/main` and not of `origin/development` (`git merge-base --is-ancestor`); `git rev-list --count origin/development..origin/main` prints 8.
- **F22 [verified]:** Measured 2026-10-11 at the head of this branch. `CODE_OF_CONDUCT.md` exists (Contributor Covenant 2.1, enforcement contact `chris@todie.io`, lines 62-63 and 117-119) and `.github/CODEOWNERS` exists (`* @todie`; other humans named in comments). `MAINTAINERS.md`, `RELEASING.md` and a README support disclaimer do not exist, and no workflow runs `cargo-semver-checks`. `release-plz.toml` sets `publish = false` and `semver_check = false`. Commands: `ls CODE_OF_CONDUCT.md MAINTAINERS.md RELEASING.md .github/CODEOWNERS`; `grep -rn semver .github/workflows release-plz.toml`; `grep -n -i -E 'stability|support' README.md` prints nothing.
- **F23 [reported: oss-roadmaps digest 2026-10-11]:** Practice at other projects. Meta ships `CODE_OF_CONDUCT.md` and a `MAINTAINERS` file at the repo root; Cloudflare ships a code of conduct and `CODEOWNERS`, and quiche's `RELEASING.md` has `cargo-semver-checks` decide the bump of pre-1.0 crates; Google requires the text "This is not an officially supported Google product", a `LICENSE` in every vendored directory, and a tracked approval before launch; Cloudflare's launch was a blog post; PyTorch and React moved to a foundation years after launch. Digest sections 1 to 4 and 6.

## Design

Two licences, one boundary. Userspace code is permissive. The BPF program is dual GPL and MIT,
because the kernel demands a GPL-compatible licence for two helpers it calls. The program reaches
userspace only as bytes loaded at run time. The repository is built to open-source standard now
and stays private; the vendored contracts decide its public form.

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Every root-workspace package (the `uas-*` crates and `xtask`) is licensed `Apache-2.0 OR MIT`. The BPF program, its source under `crates/uas-ebpf/ebpf/` and the object `uas-ebpf.o`, is licensed `GPL-2.0-only OR MIT`, and its `license` section keeps the kernel string `Dual MIT/GPL` (F4). This replaces spec 0004 line 55 and its 2026-10-09 position (F8). `deny.toml` still denies copyleft dependencies. | The kernel needs a GPL-compatible licence for `bpf_probe_read_kernel` and `bpf_probe_read_kernel_str` (F5). The MIT arm keeps the object usable by a permissive loader. | ruled: the operator, lane w25:p1, 2026-10-11 about 00:35Z [reported: integrating lane brief] |
| D2 | Layout and holder. The root holds `LICENSE-APACHE` and `LICENSE-MIT`; the proprietary `LICENSE` and `docs/IP_NOTICE.md` go (F11, F19). `crates/uas-ebpf/ebpf/LICENSE` holds the GPL-2.0 and MIT texts and offers either. `[workspace.package]` sets `license = "Apache-2.0 OR MIT"`, and every root package keeps `license.workspace = true` (F2). The BPF package cannot inherit (F3) and sets `license = "GPL-2.0-only OR MIT"`. Every tracked `.rs` and `.sh` file outside `vendor/` carries an SPDX line in its first two lines: `GPL-2.0-only OR MIT` under `crates/uas-ebpf/ebpf/`, `Apache-2.0 OR MIT` elsewhere. The copyright holder is `unsigned.gg`: in `NOTICE`, in the copyright lines of `LICENSE-MIT` and `crates/uas-ebpf/ebpf/LICENSE`, and in `authors = ["unsigned.gg"]` at `Cargo.toml` line 13, which the root packages inherit; the BPF package gains the same `authors`. `NOTICE` also maps each licence to its files: code, the BPF program, docs and specs (D10), and `vendor/` (F12). | The files land in this repository now (D4). The holder replaces "unsigned.gg" (F19). | ruled: the holder, the operator, lane w25:p1, 2026-10-11. Lane-decided: the layout, integrating lane brief, 2026-10-11; the `vendor/` clause and the `authors` value, uas-engineer (F12, F19) |
| D3 | Kernel boundary. `uas-ebpf` embeds the object as bytes and the kernel loads it through `bpf(2)` at run time (F7). No symbol links a userspace crate to the program, and no crate links libbpf (F9). The kernel's BPF licensing document treats the two as separate executable processes (F7), and the embedded copy is available under its MIT arm, so no userspace crate takes GPL terms. Any future LSM or struct_ops program, or one that calls a kfunc, declares `Dual MIT/GPL` (F6). | Two GPL-only helpers are in use, `bpf_probe_read_kernel` and `bpf_probe_read_kernel_str`; `bpf_ktime_get_ns` is not GPL-only in v6.6 (F5). The conclusion holds: the object already declares `Dual MIT/GPL`. | lane-decided: integrating lane brief, 2026-10-11; embedding and helper facts by uas-engineer, 2026-10-11 |
| D4 | Built to open-source standard now, while private. The repository's visibility stays private (REQ-0048-09). Packages 1 to 3 land in this repository now: the licence files, the manifest fields, the SPDX lines, `CONTRIBUTING.md` with the DCO, `SECURITY.md` per spec 0045, a tree with no tailnet hostname and no operator-specific path, and the contract fetch. The public form is a fresh clean-tree repository, later (D9); creating it is the operator's act (spec 0037 D1). This answers the timing question the first draft held as D9: "proprietary until the cut" is superseded. | His words: "ok let's keep it private for a while longer but let's build it to be an open source repo". This supersedes the export-later framing (Notes). A history rewrite here stays barred by spec 0037 D5. | ruled: the operator, lane w25:p1, 2026-10-11 about 00:40Z [reported: integrating lane relay, verbatim]; visibility: cortex lane w10:p0, mesh ead30203; the fresh repository: ruled, the operator, lane w25:p1, 2026-10-11 |
| D5 | Contributions. `CONTRIBUTING.md` states the DCO now: every commit carries a `Signed-off-by:` trailer matching its author. No CLA. Commits stay GPG-signed (AGENTS.md rule 5). A DCO check becomes a required check when the repository takes its public form; making it required earlier is the operator's call. | containerd's model (F15). Each contributor certifies the right to submit under the file's licence, with no contract. Required now would change every lane's commit command before anyone outside can contribute. | lane-decided: integrating lane brief, 2026-10-11; the timing of the required check by uas-engineer, 2026-10-11 (the operator may override) |
| D6 | crates.io `uas`. At its next publish the crate carries the D1 expression; today's `MIT OR Apache-2.0` has the same arms (F13). Its `repository` names the public form: this URL after a flip, the new one after a cut. Today that URL returns 404 to an anonymous reader (F13). Its README names the reporting address of spec 0045 D3. This closes spec 0045 D8: the crate is the publication vehicle (spec 0037 D13), not a name hold. Publishing stays the operator's act. | The workspace, not the crate, was out of line (F2). A reporter who follows the link reaches nothing. | ruled: the operator, 2026-10-11, lane w25:p1 (AskUserQuestion), "Confirm, the crate is the vehicle". Basis: spec 0037 D13; integrating lane brief, 2026-10-11; README clause by uas-engineer, from D8's first option |
| D7 | Tree hygiene, now. Tracked files hold no tailnet hostname (F14) and no operator-specific path (F16). Specs and agent files describe the thing instead of its path; the `blackwall.toml` endpoint moves to an untracked local setting. `make hygiene` gains `oss-check`, which fails on the F16 pattern. Operator names in decision attributions stay (Notes). `vendor/` is D9's. | The ruling names hostnames and paths. `hygiene` is a required check (spec 0045 F3), so a new path cannot land. | ruled: the list, the operator via the relay of 2026-10-11 about 00:40Z. Lane-decided: the check and its pattern, uas-engineer, 2026-10-11 |
| D8 | The licence gate checks this workspace's own licence. `deny.toml` sets `[licenses.private] ignore = false` and drops the "proprietary and unpublished" comment. The allow list does not change, so GPL stays out (F10). `make deny` also runs `cargo deny --manifest-path crates/uas-ebpf/ebpf/Cargo.toml check --config deny.toml licenses`. | Today a GPL workspace crate passes unseen, and the BPF workspace is never read (F10). F3 is a wrong field that passed. A list of only Apache-2.0, MIT and BSD-3-Clause fails on today's dependencies (F17); narrowing the list is a separate licence decision. | lane-decided: uas-engineer, 2026-10-11 (F10, F17) |
| D9 | Vendored contracts: option (a). `vendor/cortex/`, `vendor/soma/` and `vendor/saga/` leave the tree now (F12). `cargo xtask fetch-contracts` fetches them into the ignored `.contracts/`, each file pinned by upstream commit and sha256 in a tracked `contracts.lock`, starting from the README pins (F12). The `cortex-grpc` gRPC code is regenerated at build time from the fetched protos: `uas-engine`'s `build.rs` reads `.contracts/cortex/proto` and, when it is absent, fails with the fetch command in its message. The default build needs no fetch (F12). `vendor/leanspec/` (MIT) stays. History and the pull-request refs still hold the old files (F12, F14), so publication is a fresh clean-tree repository later, not a flip. | A tree that builds a feature only from private tracked files is not built to open-source standard (D4). No option clears history, and spec 0037 D5 bars a rewrite. | ruled: the operator, lane w25:p1, 2026-10-11 (option (a) and the fresh repository). Lane-decided: the fetch command, the lock file and the `build.rs` path, uas-engineer, 2026-10-11 |
| D10 | Docs licence. Every `*.md` file under `docs/`, `specs/` and `engineering/` is `CC-BY-4.0`, with the legal text in a root `LICENSE-DOCS`. Each carries `<!-- SPDX-License-Identifier: CC-BY-4.0 -->` on line 1; a spec carries it on the line after its frontmatter, because spec-check requires `---` on line 1 (F20). Code stays `Apache-2.0 OR MIT`. Other non-code files outside `vendor/` take `Apache-2.0 OR MIT` through `NOTICE`. | CC-BY-4.0 is written for prose; Apache-2.0 and MIT for code. The placement keeps spec-check green (F20). | ruled: the licence and the three directories, the operator, lane w25:p1, 2026-10-11. Lane-decided: the tag placement and the default for other files, uas-engineer, 2026-10-11 |
| D11 | `CODE_OF_CONDUCT.md` is the Contributor Covenant 2.1, and its enforcement contact is the address in spec 0045 D3. It joins the REQ-0048-03 file list. The file exists and already holds both (F22). | A public project needs a stated conduct rule and a contact; Meta and Cloudflare ship one (F23). | ruled: the operator, lane w25:p1, 2026-10-11 about 02:35Z (AskUserQuestion), "Contributor Covenant 2.1" |
| D12 | `.github/CODEOWNERS` routes review. A new `MAINTAINERS.md` names humans and their areas, with the BPF program and the sandbox called out, and names no team, bot or alias. | The DCO check (D5) needs someone who can waive it; Meta keeps a `MAINTAINERS` file and Cloudflare a `CODEOWNERS` (F23). | ruled: the operator, lane w25:p1, 2026-10-11 about 02:35Z, "CODEOWNERS plus MAINTAINERS.md" |
| D13 | `RELEASING.md` states pre-1.0 semver (a breaking change bumps the minor), the CHANGELOG one-line-per-PR rule, and that release-plz tags as today. CI runs `cargo-semver-checks` on each published crate and fails a breaking change without the minor bump. No crate publishes today (F2, F22), so the job has nothing to compare until the first publish. The tool is a CI download and takes a row in spec 0004's CI tools table (version and sha256). | quiche's model (F23). A semver gate catches an accidental break before a user does. | ruled: the operator, lane w25:p1, 2026-10-11 about 02:35Z, "RELEASING.md, pre-1.0 semver, cargo-semver-checks gate" |
| D14 | Launch is a checklist, then a soft launch. Plan 0048 holds one go/no-go checklist: `make hygiene` and `oss-check` green, a secret scan of the whole tree, licence files and SPDX complete, the docs build green, the operator's explicit sign-off on the operator names of D7 at the cut, and then the operator's go. The repository turns public with no announcement until docs and CI are green there. The announcement text is a parked decision. | Google gates launch on an approval step; a quiet flip leaves time to fix what a first reader finds (F23). | ruled: the operator, lane w25:p1, 2026-10-11 about 02:35Z, "Checklist then soft launch"; the D7 sign-off item lane-decided, integrating lane w25:p1, 2026-10-11 |
| D15 | The README states that the project is pre-1.0 and makes no stability or support promise, and that example code is not for production. | Google requires such a statement; Cloudflare's quiche and pingora state one (F23). | lane-decided: integrating lane w25:p1, 2026-10-11 |
| D16 | `oss-check` also fails when a directory under `vendor/` has no `LICENSE` file. After D9 only `vendor/leanspec/` remains, and it has one (F12). | Google's rule for vendored code (F23). Low cost. | lane-decided: integrating lane w25:p1, 2026-10-11 |

### Requirements (EARS)

- **[REQ-0048-01]** (Ubiquitous): The root `Cargo.toml` shall set `license = "Apache-2.0 OR MIT"` in `[workspace.package]`, and every root-workspace package shall inherit it with `license.workspace = true`.
- **[REQ-0048-02]** (Ubiquitous): The package `crates/uas-ebpf/ebpf` shall set `license = "GPL-2.0-only OR MIT"`, and its object shall hold `Dual MIT/GPL` in its `license` section.
- **[REQ-0048-03]** (Ubiquitous): The repository root shall hold `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-DOCS`, `NOTICE`, `CODE_OF_CONDUCT.md` and a `CONTRIBUTING.md` that states the DCO sign-off rule, and no `LICENSE` or `docs/IP_NOTICE.md`; `crates/uas-ebpf/ebpf/LICENSE` shall hold the GPL-2.0 and MIT texts; `NOTICE` shall name `unsigned.gg` as the copyright holder and map each licence to its files.
- **[REQ-0048-04]** (Ubiquitous): Every tracked `.rs` and `.sh` file outside `vendor/` shall carry an `SPDX-License-Identifier` line in its first two lines, `GPL-2.0-only OR MIT` under `crates/uas-ebpf/ebpf/` and `Apache-2.0 OR MIT` elsewhere, and every tracked `*.md` file under `docs/`, `specs/` and `engineering/` shall carry `SPDX-License-Identifier: CC-BY-4.0` in its first ten lines.
- **[REQ-0048-05]** (Unwanted): IF a root-workspace package or the BPF package declares a licence expression with no arm in the `deny.toml` allow list, THEN `make deny` shall exit non-zero and name the package.
- **[REQ-0048-06]** (Ubiquitous): No userspace crate shall link libbpf or a GPL-licensed library; userspace shall reach the BPF program only as object bytes loaded through the `bpf` system call.
- **[REQ-0048-07]** (Optional feature): WHERE a BPF program of type LSM or struct_ops, or one that calls a kfunc, is added, the program shall declare `Dual MIT/GPL` and its package shall stay `GPL-2.0-only OR MIT`.
- **[REQ-0048-08]** (Unwanted): IF a tracked file holds a match for the F16 pattern, which covers tailnet hostnames, home directories under `/home`, `/Users` and `/root`, home-relative estate paths and the research surfaces, or lies under `vendor/cortex/`, `vendor/soma/` or `vendor/saga/`, THEN `make hygiene` shall exit non-zero and name the file.
- **[REQ-0048-09]** (State-driven): WHILE the operator has not created the fresh public repository, this repository shall stay private, and no lane shall change its visibility or create a public copy.
- **[REQ-0048-10]** (Event-driven): WHEN the fresh public repository is created, its history shall start with one commit holding the tree of `main`, `make check` shall pass on that commit, and a required check shall fail any pull request that holds a commit with no `Signed-off-by:` trailer matching its author.
- **[REQ-0048-11]** (Unwanted): IF a file fetched by `cargo xtask fetch-contracts` does not match the commit and sha256 recorded in `contracts.lock`, THEN the fetch shall exit non-zero, name the file, and leave it out of `.contracts/`.
- **[REQ-0048-12]** (Event-driven): WHEN the crates.io `uas` crate is next published after the public form exists, its licence shall have the arms `Apache-2.0` and `MIT` only, and its `repository` shall name a URL an anonymous reader can open.
- **[REQ-0048-13]** (Ubiquitous): `CODE_OF_CONDUCT.md` shall be the Contributor Covenant 2.1, and its enforcement contact shall be the reporting address of spec 0045 D3.
- **[REQ-0048-14]** (Ubiquitous): The repository shall hold `.github/CODEOWNERS` and a `MAINTAINERS.md` that names humans and areas, calls out the BPF program and the sandbox, and names no team or bot.
- **[REQ-0048-15]** (Ubiquitous): `RELEASING.md` shall state pre-1.0 semver with a minor bump for a breaking change, the one-line-per-PR CHANGELOG rule, and that release-plz tags.
- **[REQ-0048-16]** (Event-driven): WHEN a pull request is opened against `development` or `main`, CI shall run `cargo-semver-checks` on each published crate and shall fail the job on a breaking change that lacks the minor bump of REQ-0048-15.
- **[REQ-0048-17]** (State-driven): WHILE `engineering/plans/0048-launch-checklist.md` holds an unticked item or no go from the operator, the repository shall stay private; WHILE docs or required CI are red on the public repository, no announcement shall be made.
- **[REQ-0048-18]** (Ubiquitous): `README.md` shall state that the project is pre-1.0, makes no stability or support promise, and that example code is not for production.
- **[REQ-0048-19]** (Event-driven): WHEN `make oss-check` runs, it shall exit non-zero and name any directory under `vendor/` that has no `LICENSE` file.

## Plan

Package 1, licence layout, now, one PR into `development` (D1, D2, D5, D8, D10). Done when the
manifest, licence-file, holder, DCO and both SPDX legs of the Test command pass; each is red today
by design (F2, F11, F18, F19, F20). Spec 0004 line numbers below are at `7d97fa2`:

- [ ] First in the PR, spec 0004: replace the line 55 requirement with D1; amend the 2026-10-09 position (lines 160 to 163) and the `aya-ebpf` row (line 180) so the program package is `GPL-2.0-only OR MIT`; rewrite lines 15 to 18 ("The template is proprietary", with the holder) and line 158 ("This repo is proprietary") to D1 and D2. Copyleft stays denied for dependencies. the operator confirmed D6 on 2026-10-11; spec 0045 D8 points at it. The policy changes before the files (rule 12's order).
- [ ] Add `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-DOCS` (the CC-BY-4.0 text), `NOTICE` and `crates/uas-ebpf/ebpf/LICENSE`; remove `LICENSE` and `docs/IP_NOTICE.md`.
- [ ] Holder `unsigned.gg` (D2): `NOTICE`; the copyright lines of `LICENSE-MIT` and `crates/uas-ebpf/ebpf/LICENSE`; `authors` at `Cargo.toml` line 13; a new `authors` in `crates/uas-ebpf/ebpf/Cargo.toml`.
- [ ] Set the workspace licence and the BPF package licence (REQ-0048-01, 02).
- [ ] Add the SPDX line to the 127 code files of F11 (the count at `07d0b52`; recount at the PR) and the CC-BY-4.0 tag to the 65 Markdown files of F20, and a `spdx-check` target in `make hygiene` that checks both. Spec 0025 then reaches 301 lines and warns (F20).
- [ ] `deny.toml`: `ignore = false`; `make deny`: the BPF workspace leg (D8).
- [ ] `CONTRIBUTING.md`: a DCO section with the certificate text and `git commit -s` (D5).
- [ ] One CHANGELOG line under `[Unreleased]`.

Package 2, tree hygiene, now, one PR into `development` (D7). Done when the hostname and path leg of
the Test command passes:

- [ ] Replace the three hostnames of F14 and the 42 path lines of F16. Specs and agent files name the thing, not the path: "the research surfaces", "the estate SOP on custody", "the shared checkout".
- [ ] `blackwall.toml`: move the OTLP endpoint to an untracked local setting. Whether blackwall reads one is not checked (Notes).
- [ ] `uas-ebpfd.service`: `Documentation=` points at the spec by its repository path, not a home path.
- [ ] An `oss-check` target in `make hygiene` that runs `git grep -n -E` with the F16 pattern and fails on any match.
- [ ] `SECURITY.md` per spec 0045 D3. That spec's Plan owns the item; it is listed here so the open-source checklist is whole.

Package 3, contract fetch, now, one PR into `development` (D9). Done when the vendor leg of the Test
command passes:

- [ ] First, spec 0004's vendored table (lines 123 to 128) and the `vendor/` row of `AGENTS.md` (line 27) describe the fetch.
- [ ] `cargo xtask fetch-contracts` and `contracts.lock` (repository, path, commit and sha256 per file, from the F12 pins); `.contracts/` in `.gitignore`.
- [ ] `crates/uas-engine/build.rs` reads `.contracts/cortex/proto`; a missing fetch fails the `cortex-grpc` build and names the command.
- [ ] Remove `vendor/cortex/`, `vendor/soma/` and `vendor/saga/`; `oss-check` fails if any returns (REQ-0048-08).
- [ ] A CI job that builds `cortex-grpc` needs read access to `cerebral-work/cortex`. That credential is an operator-gated secret; until it exists, CI builds default features only, as today.

Package 4, day-one files, now, one PR into `development` (D11 to D13, D15, D16). Done when the
day-one leg of the Test command passes:

- [ ] `MAINTAINERS.md` (D12); `.github/CODEOWNERS` keeps its routes; `CODE_OF_CONDUCT.md` is checked against REQ-0048-13.
- [ ] `RELEASING.md` (D13); a `cargo-semver-checks` CI job and its spec 0004 CI tools row; the README disclaimer (D15).
- [ ] `oss-check` fails on a `vendor/` directory with no `LICENSE` (D16).
- [ ] Write `engineering/plans/0048-launch-checklist.md` (D14); its go item is the operator's.

The public form, the operator's acts:

- [ ] Create the fresh public repository from the tree of `main` in one commit (REQ-0048-10); make the DCO check required (D5); enable private vulnerability reporting (spec 0045 D3).
- [ ] Publish `uas` per D6. Announce only after the checklist of D14 and green docs and CI on the public repository; the text is a parked decision.

## Test

| Requirement | Negative control |
|---|---|
| REQ-0048-01, 05 | On a `git archive` copy with `ignore = false`: today's tree exits 4 with 11 rejections; the D1 licence exits 0; one crate at `GPL-2.0-only` exits 4 (F10). The rejection names the package: `uas-domain v0.1.0` and its `crates/uas-domain/Cargo.toml` line 8 (measured at `07d0b52`). Goes red if `ignore = true` returns: the GPL crate then passes (F10). The manifest leg below exits 1 today, 0 on a copy with the D1 licences, and 1 when one root crate sets its own licence (measured at `07d0b52`). |
| REQ-0048-02, 05 | `cargo deny --manifest-path crates/uas-ebpf/ebpf/Cargo.toml check --config deny.toml licenses` exits 0 at `GPL-2.0-only OR MIT` and 4 at `GPL-2.0-only`, naming `uas-ebpf-ebpf v0.1.0` (F10; measured again at `07d0b52`). `strings crates/uas-ebpf/ebpf/uas-ebpf.o \| grep -cx 'Dual MIT/GPL'` prints 1. A changed `LICENSE` static, rebuilt and committed, makes it print 0; left unbuilt, it fails the `ebpf-live` drift step. Red today: the manifest says `MIT OR Apache-2.0` (F3). |
| REQ-0048-03 | `test -f` on the five licence files, `CODE_OF_CONDUCT.md` and `test ! -e LICENSE -a ! -e docs/IP_NOTICE.md` exit 0; `grep -c unsigned.gg NOTICE` prints at least 1; the F19 holder grep prints nothing; `grep -c 'Signed-off-by' CONTRIBUTING.md` prints at least 1. Red today: no licence file exists (F11), the holder grep prints 3 lines (F19), and the DCO grep prints 0 (F18). |
| REQ-0048-04 | The code and Markdown SPDX legs of the command below print nothing. Red today: 127 code files at `07d0b52` (F11) and 65 Markdown files at `7d97fa2` (F20). Guard: remove the tag from one file and it is listed; put `Apache-2.0 OR MIT` in `ebpf/src/main.rs` and `spdx-check` fails. |
| REQ-0048-06 | `grep -rn libbpf Cargo.lock crates/uas-ebpf/ebpf/Cargo.lock` prints nothing; control: `grep -c '^name = "aya"' Cargo.lock` prints 1 (F9). |
| REQ-0048-07 | Not exercisable: no LSM, struct_ops or kfunc program exists (F6). The kernel is the control: `bpf_lsm.c` rejects a non-GPL LSM program at load. |
| REQ-0048-08 | `git grep -n -E` with the F16 pattern prints nothing, and `ls -d vendor/cortex vendor/soma vendor/saga` lists nothing. Positive control: 25 files and 45 lines at `07d0b52` (F16), and 3 paths at `7d97fa2`. Guard: put a home path in a scratch tracked file; `make hygiene` fails and names it. |
| REQ-0048-09 | `gh api repos/unsigned-gg/uas --jq .private` prints `true` (F14). Red if it ever prints `false`: D9 rules out a flip. |
| REQ-0048-10 | On the fresh repository: `git rev-list --count` on its first push prints 1, `make check JOBS=2` exits 0 there, and a pull request with one commit lacking the trailer fails the DCO check. Not exercisable until it exists. |
| REQ-0048-11 | With one sha256 in a scratch `contracts.lock` changed by one character, `cargo xtask fetch-contracts` exits non-zero and names the file; with the true value it exits 0. Goes red if the hash compare is removed. |
| REQ-0048-12 | `curl -s -o /dev/null -w '%{http_code}'` on the crate's `repository` URL prints 200. Red today: 404 (F13); control: the containerd URL prints 200. |
| REQ-0048-13 | `grep -c 'version 2.1' CODE_OF_CONDUCT.md` prints at least 1 and the address of `SECURITY.md` appears in the file. Holds today (F22). Goes red if the contact line is deleted. |
| REQ-0048-14 | `test -f MAINTAINERS.md -a -f .github/CODEOWNERS` and `grep -c -i -E 'ebpf|sandbox' MAINTAINERS.md` prints at least 2. Red today: no `MAINTAINERS.md` (F22). |
| REQ-0048-15 | `test -f RELEASING.md` and `grep -c -i -E 'pre-1.0|minor|release-plz' RELEASING.md` prints at least 3. Red today (F22). |
| REQ-0048-16 | On a scratch copy of a crate with a pristine copy as `--baseline-root`: unchanged, `cargo semver-checks` exits 0; with one `pub fn` removed it exits non-zero and names the lint. Goes red if the CI job is removed: `grep -rn semver-checks .github/workflows` prints nothing. |
| REQ-0048-17 | `test -f engineering/plans/0048-launch-checklist.md` and `grep -c '^- \[' ` on it prints at least 6, the last item being the operator's go. Red today: no file. The visibility leg is REQ-0048-09. |
| REQ-0048-18 | `grep -c -i -E 'pre-1.0|no stability|not for production' README.md` prints at least 3. Red today (F22). |
| REQ-0048-19 | In a scratch copy, `mkdir vendor/x` with a file but no `LICENSE`: `make oss-check` exits non-zero and names `vendor/x`; with `vendor/leanspec` alone it exits 0 (positive control). Goes red if the leg is removed. |

```bash
make deny hygiene JOBS=2 && \
  grep -qx 'license = "Apache-2.0 OR MIT"' Cargo.toml && \
  grep -qx 'license = "GPL-2.0-only OR MIT"' crates/uas-ebpf/ebpf/Cargo.toml && \
  test -z "$(grep -L 'license.workspace = true' crates/*/Cargo.toml xtask/Cargo.toml)" && \
  test -f LICENSE-APACHE -a -f LICENSE-MIT -a -f LICENSE-DOCS -a -f NOTICE -a -f crates/uas-ebpf/ebpf/LICENSE && \
  test ! -e LICENSE -a ! -e docs/IP_NOTICE.md && grep -q 'unsigned.gg' NOTICE && \
  ! git grep -q -E 'the operator M\. Todie / cerebral\.work|All rights reserved' -- ':!specs' ':!vendor' ':!engineering' ':!CHANGELOG.md' && \
  grep -q 'Signed-off-by' CONTRIBUTING.md && \
  test -z "$(git ls-files -z -- '*.rs' '*.sh' ':!vendor' | xargs -0 grep -L 'SPDX-License-Identifier')" && \
  test -z "$(git ls-files 'docs/*.md' 'specs/*.md' 'engineering/*.md' | while read -r f; do head -n 10 "$f" | grep -q 'SPDX-License-Identifier: CC-BY-4.0' || echo "$f"; done)" && \
  test ! -e vendor/cortex -a ! -e vendor/soma -a ! -e vendor/saga && \
  test -f CODE_OF_CONDUCT.md -a -f MAINTAINERS.md -a -f RELEASING.md -a -f .github/CODEOWNERS && \
  ! git grep -n -E '\.ts\.net|/(home|Users)/[a-z]+/|/roo(t)/|~/(projects|\.agents|\.claude|vault|\.local|\.config|keys)|<surfaces>(s)' && \
  strings crates/uas-ebpf/ebpf/uas-ebpf.o | grep -qx 'Dual MIT/GPL'
```

Passing means exit 0 after packages 1 to 4. The chain stops at its first red leg. Measured at
`7d97fa2`: `make deny hygiene` and the `strings` leg pass; the manifest, licence-file, holder, DCO,
both SPDX, vendor and hostname/path legs are red. Package 1 turns the first six green, package 3
the vendor leg, package 2 the hostname/path leg.

## Notes

- Superseded at drafting time: the first brief framed the public tree as a clean-tree export,
  later, from a repository kept proprietary. the operator's ruling of 2026-10-11 about 00:40Z
  replaced that: build to open-source standard now, stay private (D4). The export step is gone:
  D9 was ruled as option (a), and the fresh repository starts from the tree of `main`.
- Rulings needed: the date of the fresh public repository and its name; whether `uas-operator`
  (spec 0038) publishes separately. Each is the operator's. D6 is ruled (2026-10-11).
- `main` holds `b220833`, the merge of PR #119, which is not on `development` (F21). It is not
  reverted: the next promotion of `development` to `main` absorbs it (spec 0003) [ruled:
  the operator, lane w25:p1, 2026-10-11].
- Kept, not removed: operator names in decision attributions. The first brief listed them for
  the export; the ruling names hostnames and paths only. Removing them is the operator's call.
- No counsel has reviewed D3. It rests on the kernel's own document and the object's MIT arm
  [asserted].
- Open: `uas-ebpf` cannot publish as packaged (F7); which crates publish is spec 0039's subject.
  Whether blackwall reads its OTLP endpoint from an untracked setting is not checked. A CI
  credential for `cortex-grpc` is an operator-gated secret (Plan, package 3).
- Open: every pull-request job runs on self-hosted runners behind a same-repo guard (F14). The
  runners of the public form are spec 0005's subject; the guards stay.
- Brief corrections: `bpf_ktime_get_ns` is not GPL-only in v6.6 (F5); pull-request refs number
  139 to 142, not 125 (F14); no allow-list change is needed, and the narrower example list fails
  (F10, F17); the BPF package declares `MIT OR Apache-2.0` (F3); `vendor/leanspec` is MIT and
  stays; replacing the vendored contracts does not clear them from history (D9).
- Parked: a foundation or neutral home. PyTorch and React moved years after launch and kept
  licence and contributor rules (F23). Not decided here.
- Parked: the announcement text and timing (D14). the operator's call after the checklist.
- Open: the docs build command of the D14 checklist is not named by any spec; P17 names it.
- Out of scope: trademark use of the name `uas`, an SBOM licence field (spec 0045 D4), and
  relicensing the vendored contracts.
