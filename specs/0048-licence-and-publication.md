---
status: planned
created: 2026-10-11
tags: [licence, publication, ebpf, crates-io, dco, export]
priority: high
depends_on: [0003-branch-model-and-promotion, 0004-dependency-and-licence-policy, 0005-ci-cache-and-runners, 0037-repository-migration-to-uas, 0045-security-disclosure-and-advisory-policy]
---

# 0048-licence-and-publication

> **Status**: planned, **Priority**: high, **Created**: 2026-10-11

## Overview

Christian framed uas as "the containerd of agent harnesses" and ruled its licence. The userspace
crates publish under `Apache-2.0 OR MIT`. The kernel-side BPF program sources and object stay
`Dual MIT/GPL`, SPDX `GPL-2.0-only OR MIT` [reported: ruling in lane w25:p1, 2026-10-11 about
00:35Z, relayed in the integrating lane's brief]. About five minutes later he ruled the route: "ok
let's keep it private for a while longer but let's build it to be an open source repo"
[reported: lane w25:p1, 2026-10-11 about 00:40Z, relayed verbatim]. The repository stays private
[reported: cortex lane w10:p0 relay of Christian, mesh ead30203, 2026-10-10 about 23:05Z], and it
is built to open-source standard now.

Today the workspace is proprietary and spec 0004 requires that. The crates.io placeholder already
says `MIT OR Apache-2.0` (spec 0045 D8). This spec sets the licence layout, the kernel boundary,
the open-source build-out of this repository, the contribution rule, and the one open question:
the vendored private contracts.

### Facts

Measured 2026-10-11 on ceres at development `340475b`.

- **F1 [verified]:** `LICENSE` has 16 lines. Line 1 reads "Copyright (c) 2026 Christian M. Todie / cerebral.work. All rights reserved."; line 3 reads "PROPRIETARY SOFTWARE LICENSE". GitHub reports the licence as `NOASSERTION`. Commands: `head -3 LICENSE; wc -l LICENSE`; `gh api repos/unsigned-gg/uas --jq .license.spdx_id`.
- **F2 [verified]:** `Cargo.toml` `[workspace.package]` sets `publish = false` and `license = "LicenseRef-Proprietary"` (lines 11 and 12). The members are `crates/*` and `xtask`: 11 packages. Each sets `license.workspace = true` and `publish.workspace = true`; none sets its own licence. Command: `grep -E '^(name|license|publish)' crates/*/Cargo.toml xtask/Cargo.toml`.
- **F3 [verified]:** The BPF program package `uas-ebpf-ebpf` is its own workspace. `crates/uas-ebpf/ebpf/Cargo.toml` has a `[workspace]` table and its own `Cargo.lock`, and the root glob `crates/*` does not reach it. It sets `publish = false` and `license = "MIT OR Apache-2.0"`. That disagrees with its object (F4) and with spec 0004 (F8). Command: `cat crates/uas-ebpf/ebpf/Cargo.toml`.
- **F4 [verified]:** `crates/uas-ebpf/ebpf/src/main.rs` lines 328 to 330 put `static LICENSE: [u8; 13] = *b"Dual MIT/GPL\0"` in the `license` link section. The committed object has a 13-byte `license` section (`objdump -h crates/uas-ebpf/ebpf/uas-ebpf.o`), and `strings crates/uas-ebpf/ebpf/uas-ebpf.o | grep -i gpl` prints `Dual MIT/GPL`. Linux v6.6 `Documentation/process/license-rules.rst` lines 456 to 458 read the string as "dual licensed under a GPL v2 variant or MIT license choice". `GPL-2.0-only OR MIT` is the ruled SPDX form. `include/linux/license.h` lists the string as GPL-compatible.
- **F5 [verified]:** By name, the program calls `bpf_get_current_comm`, `bpf_get_current_pid_tgid`, `bpf_get_current_uid_gid`, `bpf_get_ns_current_pid_tgid`, `bpf_ktime_get_ns` and `bpf_probe_read_kernel_str_bytes` (`grep -oE '\bbpf_[a-z_]+' crates/uas-ebpf/ebpf/src/main.rs | sort | uniq -c`). That grep misses `ctx.read_at` (lines 243 and 285), which calls aya-ebpf 0.2.1's `bpf_probe_read_kernel` (`src/programs/tracepoint.rs` line 19, `src/helpers.rs` line 211, cargo registry); `bpf_probe_read_kernel_str_bytes` calls `bpf_probe_read_kernel_str` (`src/helpers.rs` line 558). The object settles the list: `llvm-objdump -d crates/uas-ebpf/ebpf/uas-ebpf.o | grep -oE 'call 0x[0-9a-f]+' | sort | uniq -c`, with the 1.94.1 toolchain's llvm-tools, prints helper ids 1, 2, 3, 5, 14, 15, 16, 113, 115, 120, 131 and 132. v6.6 `include/uapi/linux/bpf.h` (lines 5648 to 5779) names them map lookup, update and delete, `ktime_get_ns`, `get_current_pid_tgid`, `get_current_uid_gid`, `get_current_comm`, `probe_read_kernel`, `probe_read_kernel_str`, `get_ns_current_pid_tgid`, `ringbuf_reserve` and `ringbuf_submit`. In Linux v6.6 (ceres runs 6.6.87), two are GPL-only: `bpf_probe_read_kernel` (`.gpl_only = true` at `kernel/trace/bpf_trace.c` line 238; id 113, called twice) and `bpf_probe_read_kernel_str` (line 273; id 115, called once). `bpf_ktime_get_ns` is `.gpl_only = false` (`kernel/bpf/helpers.c` line 178), as are the comm, pid_tgid, uid_gid and ns pid helpers (lines 270, 229, 249, 617) and the three map helpers (lines 45, 61, 78). The ring buffer protos set no flag. The verifier rejects a GPL-only call from a non-GPL program (`kernel/bpf/verifier.c` lines 9558 to 9562). The kernel files came from `raw.githubusercontent.com/torvalds/linux/v6.6/`; per helper: `grep -n -A3 '<helper>_proto = {' <file> | grep gpl_only`. An earlier version of this fact named one GPL-only helper; the critic round found `read_at`.
- **F6 [verified]:** The program holds three tracepoint programs: `#[tracepoint]` at lines 214, 267 and 302, for `sched_process_exec`, `sched_process_fork` and `sched_process_exit`. It holds no LSM, struct_ops or kfunc program. `kernel/bpf/bpf_lsm.c` lines 100 to 103 reject an LSM program whose licence is not GPL-compatible. `Documentation/bpf/bpf_licensing.rst` says LSM and struct_ops programs, and programs that call a kfunc, must be GPL-compatible even when they call no GPL-only helper.
- **F7 [verified]:** `crates/uas-ebpf/src/lib.rs` line 34 embeds the object: `pub const BPF_OBJECT: &[u8] = include_bytes!("../ebpf/uas-ebpf.o")`. `crates/uas-ebpf/src/loader.rs` line 64 passes it to `Ebpf::load`. So the userspace binary carries the object as bytes, and the kernel loads it through `bpf(2)` at run time. `bpf_licensing.rst` says proprietary applications and GPL BPF programs "in the same package can co-exist because they are separate executable processes". `cargo package --list -p uas-ebpf` lists no file under `ebpf/`. Cargo skips a subdirectory that holds a `Cargo.toml` [reported: Cargo Book, manifest `include` section], so `uas-ebpf` cannot be published as it stands.
- **F8 [verified]:** Spec 0004 line 55 requires `LicenseRef-Proprietary`. Line 58 requires dual MIT/GPL for BPF objects. Lines 159 to 162 record Christian's 2026-10-09 position: the program object declares `Dual MIT/GPL`, and the loader and every other crate stay proprietary. Line 179 pins `aya-ebpf` 0.2.1 with that note. Command: `grep -n -E 'Dual MIT/GPL|[Pp]roprietary' specs/0004-*.md`.
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

- **F11 [verified]:** No `LICENSE-APACHE`, `LICENSE-MIT` or `NOTICE` exists. The tracked licence files are `LICENSE` and `vendor/leanspec/LICENSE` (`git ls-files | grep -iE '(^|/)(licen[cs]e|notice|copying)'`). No tracked `.rs` or `.sh` file outside `vendor/` carries `SPDX-License-Identifier`: `git ls-files -z -- '*.rs' '*.sh' ':!vendor' | xargs -0 grep -L 'SPDX-License-Identifier' | wc -l` prints 121 at `340475b` and 127 at `07d0b52`, after development was merged into this branch.
- **F12 [verified]:** `vendor/` holds four directories. Their READMEs state: `leanspec` MIT (codervisor, with `LICENSE`); `cortex` "Proprietary (Cerebral)"; `soma` and `saga` "Business Source License 1.1 (unsigned-gg)". Cortex, soma and saga entered history in `7c17f69` on 2026-10-08 (`git log --diff-filter=A -- vendor/cortex vendor/soma vendor/saga`). Only `crates/uas-engine/build.rs` line 9 reads one of them, `vendor/cortex/proto`, and only under the feature `cortex-grpc`. `make check` builds default features (Makefile lines 23 to 38). Command: `git grep -n -E 'vendor/(cortex|soma|saga)' -- crates xtask Makefile .github`.
- **F13 [verified]:** crates.io `uas` 0.1.0 has licence `MIT OR Apache-2.0` and repository `https://github.com/unsigned-gg/uas`. Command: `curl -s https://crates.io/api/v1/crates/uas | jq -r '.versions[0].license, .crate.repository'`. Anonymous `curl -s -o /dev/null -w '%{http_code}'` on that URL prints 404; control: `https://github.com/containerd/containerd` prints 200. That expression has the same two arms as the ruled `Apache-2.0 OR MIT`. Spec 0045 F10 records the rest.
- **F14 [verified]:** `gh api repos/unsigned-gg/uas --jq .private` prints `true`. `git ls-remote origin 'refs/pull/*/head' | wc -l` prints 139 at 00:42Z and 142 at 00:54Z; the relay counted 125 at 23:05Z. The regex `[a-z0-9-]+\.tail[0-9a-f]+\.ts\.net` matches one line each: the OTLP endpoint at `blackwall.toml` line 32, spec 0013 line 38 and spec 0017 line 27 (`git grep -c -E <regex>`). `docs/migration/` holds 10 files from spec 0037: settings exports, `cutover.md` and `uas-impact-list.md`. Every pull-request job in `ci.yml`, `codeql.yml` and `labeler.yml` runs on the self-hosted group `ent-default` behind the guard `github.event.pull_request.head.repo.full_name == github.repository`, which skips fork pull requests (`grep -n -E 'runs-on|head.repo.full_name' .github/workflows/*.yml`).
- **F15 [verified]:** containerd is `Apache-2.0` (`gh api repos/containerd/containerd --jq .license.spdx_id`). Its contribution guide, `containerd/project` `CONTRIBUTING.md`, asks for a `Signed-off-by:` line on every commit under the Developer Certificate of Origin (DCO, line 189). `grep -n -w CLA` on that file prints nothing; control: `grep -n 'Developer Certificate'` prints line 189.
- **F16 [verified]:** The pattern `\.ts\.net|/(home|Users)/[a-z]+/|/roo(t)/|~/(projects|\.agents|\.claude|vault|\.local|\.config|keys)|/tmp/research-surface(s)` matches 45 lines in 25 tracked files at `cf45219` and again at `07d0b52`, none under `vendor/` (`git grep -l -E <pattern> | wc -l`; `git grep -n -E <pattern> | wc -l`). By part: home paths 10 lines in 8 files, home-relative paths 19 in 14, the research surfaces 13 in 10, hostnames 3 in 3. The files include specs, `.claude/agents/`, `.github/CODEOWNERS`, `blackwall.toml`, `.agents/skills/custody-run/SKILL.md` and `crates/uas-ebpf/uas-ebpfd.service` line 6 (`Documentation=` with a home path). Every home-path match is an operator workstation path, not a test fixture (`git grep -n -o -E '/home/[a-z]+/[^ ]{0,30}'`). Paths under `/Users` and `/root` match nothing today; the trailing slash spares spec 0010 line 108, which names `/root` as a directory. The pattern is written so it does not match itself.
- **F17 [verified]:** With an allow list of only `MIT`, `Apache-2.0` and `BSD-3-Clause`, on the F10 copy with the D1 licences, `cargo deny --offline check licenses` exits 4 with 4 rejections: Zlib for `foldhash` 0.1.5 and 0.2.0 and `zlib-rs` 0.6.8, and `(MIT OR Apache-2.0) AND Unicode-3.0` for `unicode-ident` 1.0.26. The BPF workspace exits 4 on `unicode-ident`.
- **F18 [verified]:** `CONTRIBUTING.md` (45 lines) names no DCO: `grep -c -i -E 'signed-off|DCO|certificate of origin' CONTRIBUTING.md` prints 0; control: `grep -c -i commit CONTRIBUTING.md` prints 2. `SECURITY.md` exists; spec 0045 owns its text.

## Design

Two licences, one boundary. Userspace code is permissive. The BPF program is dual GPL and MIT,
because the kernel demands a GPL-compatible licence for two helpers it calls. The program reaches
userspace only as bytes loaded at run time. The repository is built to open-source standard now
and stays private; the vendored contracts decide its public form.

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Every root-workspace package (the `uas-*` crates and `xtask`) is licensed `Apache-2.0 OR MIT`. The BPF program, its source under `crates/uas-ebpf/ebpf/` and the object `uas-ebpf.o`, is licensed `GPL-2.0-only OR MIT`, and its `license` section keeps the kernel string `Dual MIT/GPL` (F4). This replaces spec 0004 line 55 and its 2026-10-09 position (F8). `deny.toml` still denies copyleft dependencies. | The kernel needs a GPL-compatible licence for `bpf_probe_read_kernel` and `bpf_probe_read_kernel_str` (F5). The MIT arm keeps the object usable by a permissive loader. | ruled: Christian, lane w25:p1, 2026-10-11 about 00:35Z [reported: integrating lane brief] |
| D2 | Layout. The root holds `LICENSE-APACHE` and `LICENSE-MIT`; the proprietary `LICENSE` goes. `crates/uas-ebpf/ebpf/LICENSE` holds the GPL-2.0 and MIT texts and offers either. `[workspace.package]` sets `license = "Apache-2.0 OR MIT"`, and every root package keeps `license.workspace = true` (F2). The BPF package cannot inherit (F3) and sets `license = "GPL-2.0-only OR MIT"`. Every tracked `.rs` and `.sh` file outside `vendor/` carries an SPDX line in its first two lines: `GPL-2.0-only OR MIT` under `crates/uas-ebpf/ebpf/`, `Apache-2.0 OR MIT` elsewhere. A root `NOTICE` names the copyright holder as `LICENSE` line 1 does today, "Christian M. Todie / cerebral.work" [proposed]; says that docs, specs and the other non-code files take the root `Apache-2.0 OR MIT` [proposed]; and says `vendor/` keeps its own licences (F12). | The files land in this repository now (D4). | lane-decided: the layout, integrating lane brief, 2026-10-11; the `vendor/` clause, uas-engineer (F12). Proposed, Christian to confirm: the holder line and the docs licence |
| D3 | Kernel boundary. `uas-ebpf` embeds the object as bytes and the kernel loads it through `bpf(2)` at run time (F7). No symbol links a userspace crate to the program, and no crate links libbpf (F9). The kernel's BPF licensing document treats the two as separate executable processes (F7), and the embedded copy is available under its MIT arm, so no userspace crate takes GPL terms. Any future LSM or struct_ops program, or one that calls a kfunc, declares `Dual MIT/GPL` (F6). | Two GPL-only helpers are in use, `bpf_probe_read_kernel` and `bpf_probe_read_kernel_str`; `bpf_ktime_get_ns` is not GPL-only in v6.6 (F5). The conclusion holds: the object already declares `Dual MIT/GPL`. | lane-decided: integrating lane brief, 2026-10-11; embedding and helper facts by uas-engineer, 2026-10-11 |
| D4 | Built to open-source standard now, while private. The repository's visibility stays private (REQ-0048-09). Packages 1 and 2 land in this repository now: the licence files, the manifest fields, the SPDX lines, `CONTRIBUTING.md` with the DCO, `SECURITY.md` per spec 0045, and a tree with no tailnet hostname and no operator-specific path. The public form, a visibility flip of this repository or a fresh clean-tree repository, turns on D9. Either act is Christian's (spec 0037 D1). This answers the timing question the first draft held as D9: "proprietary until the cut" is superseded. | His words: "ok let's keep it private for a while longer but let's build it to be an open source repo". This supersedes the export-later framing (Notes). A history rewrite here stays barred by spec 0037 D5. | ruled: Christian, lane w25:p1, 2026-10-11 about 00:40Z [reported: integrating lane relay, verbatim]; visibility: cortex lane w10:p0, mesh ead30203 |
| D5 | Contributions. `CONTRIBUTING.md` states the DCO now: every commit carries a `Signed-off-by:` trailer matching its author. No CLA. Commits stay GPG-signed (AGENTS.md rule 5). A DCO check becomes a required check when the repository takes its public form; making it required earlier is Christian's call. | containerd's model (F15). Each contributor certifies the right to submit under the file's licence, with no contract. Required now would change every lane's commit command before anyone outside can contribute. | lane-decided: integrating lane brief, 2026-10-11; the timing of the required check by uas-engineer, 2026-10-11 (Christian may override) |
| D6 | crates.io `uas`. At its next publish the crate carries the D1 expression; today's `MIT OR Apache-2.0` has the same arms (F13). Its `repository` names the public form: this URL after a flip, the new one after a cut. Today that URL returns 404 to an anonymous reader (F13). Its README names the reporting address of spec 0045 D3. If Christian confirms, this closes spec 0045 D8: the crate is the publication vehicle (spec 0037 D13), not a name hold. Publishing stays Christian's act. | The workspace, not the crate, was out of line (F2). A reporter who follows the link reaches nothing. | proposed, Christian to rule: spec 0045 D8 is his row. Basis: spec 0037 D13; integrating lane brief, 2026-10-11; README clause by uas-engineer, from D8's first option |
| D7 | Tree hygiene, now. Tracked files hold no tailnet hostname (F14) and no operator-specific path (F16). Specs and agent files describe the thing instead of its path; the `blackwall.toml` endpoint moves to an untracked local setting. `make hygiene` gains `oss-check`, which fails on the F16 pattern. Operator names in decision attributions stay (Notes). `vendor/` is D9's. | The ruling names hostnames and paths. `hygiene` is a required check (spec 0045 F3), so a new path cannot land. | ruled: the list, Christian via the relay of 2026-10-11 about 00:40Z. Lane-decided: the check and its pattern, uas-engineer, 2026-10-11 |
| D8 | The licence gate checks this workspace's own licence. `deny.toml` sets `[licenses.private] ignore = false` and drops the "proprietary and unpublished" comment. The allow list does not change, so GPL stays out (F10). `make deny` also runs `cargo deny --manifest-path crates/uas-ebpf/ebpf/Cargo.toml check --config deny.toml licenses`. | Today a GPL workspace crate passes unseen, and the BPF workspace is never read (F10). F3 is a wrong field that passed. A list of only Apache-2.0, MIT and BSD-3-Clause fails on today's dependencies (F17); narrowing the list is a separate licence decision. | lane-decided: uas-engineer, 2026-10-11 (F10, F17) |
| D9 | **Proposed, Christian to rule.** The vendored contracts under `vendor/cortex/`, `vendor/soma/` and `vendor/saga/` (F12). (a) Replace them with published artifacts or a fetch step pinned by upstream commit and sha256, and point `cortex-grpc`'s `build.rs` at the fetched copy. The tree is then open-source clean. History and the pull-request refs still hold the files from `7c17f69` (F12, F14), so the public form stays a fresh clean-tree repository unless Christian accepts that a flip publishes them. (b) Keep them, and accept that the public form is a fresh clean-tree repository made by an export step that strips them and the `cortex-grpc` feature. Recommended: (a), because a tree that builds a feature only from private files is not built to open-source standard (D4). | Neither option clears history, and spec 0037 D5 bars a rewrite. | proposed: uas-engineer, 2026-10-11, on the integrating lane's framing; Christian to rule |

### Requirements (EARS)

- **[REQ-0048-01]** (Ubiquitous): The root `Cargo.toml` shall set `license = "Apache-2.0 OR MIT"` in `[workspace.package]`, and every root-workspace package shall inherit it with `license.workspace = true`.
- **[REQ-0048-02]** (Ubiquitous): The package `crates/uas-ebpf/ebpf` shall set `license = "GPL-2.0-only OR MIT"`, and its object shall hold `Dual MIT/GPL` in its `license` section.
- **[REQ-0048-03]** (Ubiquitous): The repository root shall hold `LICENSE-APACHE`, `LICENSE-MIT`, `NOTICE` and a `CONTRIBUTING.md` that states the DCO sign-off rule; `crates/uas-ebpf/ebpf/LICENSE` shall hold the GPL-2.0 and MIT texts; `NOTICE` shall name the copyright holder and say that `vendor/` keeps its own licences.
- **[REQ-0048-04]** (Ubiquitous): Every tracked `.rs` and `.sh` file outside `vendor/` shall carry an `SPDX-License-Identifier` line in its first two lines, `GPL-2.0-only OR MIT` under `crates/uas-ebpf/ebpf/` and `Apache-2.0 OR MIT` elsewhere.
- **[REQ-0048-05]** (Unwanted): IF a root-workspace package or the BPF package declares a licence expression with no arm in the `deny.toml` allow list, THEN `make deny` shall exit non-zero and name the package.
- **[REQ-0048-06]** (Ubiquitous): No userspace crate shall link libbpf or a GPL-licensed library; userspace shall reach the BPF program only as object bytes loaded through the `bpf` system call.
- **[REQ-0048-07]** (Optional feature): WHERE a BPF program of type LSM or struct_ops, or one that calls a kfunc, is added, the program shall declare `Dual MIT/GPL` and its package shall stay `GPL-2.0-only OR MIT`.
- **[REQ-0048-08]** (Unwanted): IF a tracked file holds a match for the F16 pattern, which covers tailnet hostnames, home directories under `/home`, `/Users` and `/root`, home-relative estate paths and the research surfaces, THEN `make hygiene` shall exit non-zero and name the file and line.
- **[REQ-0048-09]** (State-driven): WHILE Christian has not given the repository its public form, the repository shall stay private, and no lane shall change its visibility or create a public copy.
- **[REQ-0048-10]** (Event-driven): WHEN the repository takes its public form, a required check shall fail any pull request that holds a commit with no `Signed-off-by:` trailer matching its author.
- **[REQ-0048-11]** (Optional feature): WHERE the public form is a fresh repository, the export step shall write a single-commit tree from `main` that holds none of the D9 vendored contracts, `make check` shall pass on it, and the step shall exit non-zero and push nothing if one remains.
- **[REQ-0048-12]** (Event-driven): WHEN the crates.io `uas` crate is next published after the public form exists, its licence shall have the arms `Apache-2.0` and `MIT` only, and its `repository` shall name a URL an anonymous reader can open.

## Plan

Package 1, licence layout, now, one PR into `development` (D1, D2, D5, D8). Done when the
manifest, licence-file, DCO and SPDX legs of the Test command pass; each is red today by design
(F2, F11, F18):

- [ ] First in the PR, spec 0004: replace the line 55 requirement with D1; amend the 2026-10-09 position (lines 159 to 162) and the `aya-ebpf` row (line 179) so the program package is `GPL-2.0-only OR MIT`; rewrite lines 15 to 18 ("The template is proprietary") and line 157 ("This repo is proprietary") to D1. Copyleft stays denied for dependencies. Only once Christian confirms D6, spec 0045 D8 points at it. The policy changes before the files (rule 12's order).
- [ ] Add `LICENSE-APACHE`, `LICENSE-MIT`, `NOTICE` and `crates/uas-ebpf/ebpf/LICENSE`; remove `LICENSE`.
- [ ] Set the workspace licence and the BPF package licence (REQ-0048-01, 02).
- [ ] Add the SPDX line to the 127 files of F11 (the count at `07d0b52`; recount at the PR), and a `spdx-check` target in `make hygiene` that fails on a missing line or the wrong expression for the path.
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

Package 3, after Christian rules D9:

- [ ] Option (a): a fetch step pinned by upstream commit and sha256 for the three vendored contracts; `cortex-grpc`'s `build.rs` reads the fetched copy; the tracked copies go.
- [ ] Option (b): `cargo xtask export --out <dir>` writes the single-commit tree of REQ-0048-11 and runs `make check` on it. It pushes nothing.

The public form, Christian's acts:

- [ ] Flip this repository or create the fresh one (D9); make the DCO check required (D5); enable private vulnerability reporting (spec 0045 D3).
- [ ] Publish `uas` per D6.

## Test

| Requirement | Negative control |
|---|---|
| REQ-0048-01, 05 | On a `git archive` copy with `ignore = false`: today's tree exits 4 with 11 rejections; the D1 licence exits 0; one crate at `GPL-2.0-only` exits 4 (F10). The rejection names the package: `uas-domain v0.1.0` and its `crates/uas-domain/Cargo.toml` line 8 (measured at `07d0b52`). Goes red if `ignore = true` returns: the GPL crate then passes (F10). The manifest leg below exits 1 today, 0 on a copy with the D1 licences, and 1 when one root crate sets its own licence (measured at `07d0b52`). |
| REQ-0048-02, 05 | `cargo deny --manifest-path crates/uas-ebpf/ebpf/Cargo.toml check --config deny.toml licenses` exits 0 at `GPL-2.0-only OR MIT` and 4 at `GPL-2.0-only`, naming `uas-ebpf-ebpf v0.1.0` (F10; measured again at `07d0b52`). `strings crates/uas-ebpf/ebpf/uas-ebpf.o \| grep -cx 'Dual MIT/GPL'` prints 1. A changed `LICENSE` static, rebuilt and committed, makes it print 0; left unbuilt, it fails the `ebpf-live` drift step. Red today: the manifest says `MIT OR Apache-2.0` (F3). |
| REQ-0048-03 | `test -f` on the four licence files exits 0, and `grep -c 'Signed-off-by' CONTRIBUTING.md` prints at least 1. Red today: no licence file exists (F11) and the grep prints 0 (F18). |
| REQ-0048-04 | The SPDX leg of the command below prints nothing. Red today: it lists 127 files at `07d0b52` (F11). Guard: remove the line from one file and it lists that file; put `Apache-2.0 OR MIT` in `ebpf/src/main.rs` and `spdx-check` fails. |
| REQ-0048-06 | `grep -rn libbpf Cargo.lock crates/uas-ebpf/ebpf/Cargo.lock` prints nothing; control: `grep -c '^name = "aya"' Cargo.lock` prints 1 (F9). |
| REQ-0048-07 | Not exercisable: no LSM, struct_ops or kfunc program exists (F6). The kernel is the control: `bpf_lsm.c` rejects a non-GPL LSM program at load. |
| REQ-0048-08 | `git grep -n -E` with the F16 pattern prints nothing. Positive control at `07d0b52`: 25 files, 45 lines (F16). Guard: put a home path in a scratch tracked file; `make hygiene` fails and names it. |
| REQ-0048-09 | `gh api repos/unsigned-gg/uas --jq .private` prints `true` (F14). Red if it prints `false` before Christian acts. |
| REQ-0048-10 | On the public form, a pull request with one commit lacking the trailer fails the DCO check. Not exercisable until then. |
| REQ-0048-11 | Only under D9 option (b). On the export tree: `ls -d vendor/cortex vendor/soma vendor/saga` lists nothing, `git rev-list --count HEAD` prints 1, `make check JOBS=2` exits 0. Positive control, this tree: the `ls` lists 3 paths. |
| REQ-0048-12 | `curl -s -o /dev/null -w '%{http_code}'` on the crate's `repository` URL prints 200. Red today: 404 (F13); control: the containerd URL prints 200. |

```bash
make deny hygiene JOBS=2 && \
  grep -qx 'license = "Apache-2.0 OR MIT"' Cargo.toml && \
  grep -qx 'license = "GPL-2.0-only OR MIT"' crates/uas-ebpf/ebpf/Cargo.toml && \
  test -z "$(grep -L 'license.workspace = true' crates/*/Cargo.toml xtask/Cargo.toml)" && \
  test -f LICENSE-APACHE -a -f LICENSE-MIT -a -f NOTICE -a -f crates/uas-ebpf/ebpf/LICENSE && \
  grep -q 'Signed-off-by' CONTRIBUTING.md && \
  test -z "$(git ls-files -z -- '*.rs' '*.sh' ':!vendor' | xargs -0 grep -L 'SPDX-License-Identifier')" && \
  ! git grep -n -E '\.ts\.net|/(home|Users)/[a-z]+/|/roo(t)/|~/(projects|\.agents|\.claude|vault|\.local|\.config|keys)|/tmp/research-surface(s)' && \
  strings crates/uas-ebpf/ebpf/uas-ebpf.o | grep -qx 'Dual MIT/GPL'
```

Passing means exit 0 after packages 1 and 2. The chain stops at its first red leg. Measured at
`07d0b52`: `make deny hygiene` and the `strings` leg pass; the manifest, `test -f`, DCO, SPDX
and hostname/path legs are red. Package 1 turns the first four of those green, package 2 the last.

## Notes

- Superseded at drafting time: the first brief framed the public tree as a clean-tree export,
  later, from a repository kept proprietary. Christian's ruling of 2026-10-11 about 00:40Z
  replaced that: build to open-source standard now, stay private (D4). The export step survives
  only as D9 option (b).
- Rulings needed: when the repository goes public; the name of a fresh repository, if D9 needs
  one; whether `uas-operator` (spec 0038) publishes separately. Each is Christian's.
- Proposed, for his confirmation (D2): the `NOTICE` holder line taken from `LICENSE` line 1,
  though the repository sits in `unsigned-gg`; docs and specs under the root `Apache-2.0 OR MIT`.
- Kept, not removed: operator names in decision attributions. The first brief listed them for
  the export; the ruling names hostnames and paths only. Removing them is Christian's call.
- No counsel has reviewed D3. It rests on the kernel's own document and the object's MIT arm
  [asserted].
- Open: `uas-ebpf` cannot publish as packaged (F7); which crates publish is spec 0039's subject.
  Whether blackwall reads its OTLP endpoint from an untracked setting is not checked.
- Open: every pull-request job runs on self-hosted runners behind a same-repo guard (F14). The
  runners of the public form are spec 0005's subject; the guards stay.
- Brief corrections: `bpf_ktime_get_ns` is not GPL-only in v6.6 (F5); pull-request refs number
  139 to 142, not 125 (F14); no allow-list change is needed, and the narrower example list fails
  (F10, F17); the BPF package declares `MIT OR Apache-2.0` (F3); `vendor/leanspec` is MIT and
  stays; replacing the vendored contracts does not clear them from history (D9).
- Out of scope: trademark use of the name `uas`, an SBOM licence field (spec 0045 D4), and
  relicensing the vendored contracts.
