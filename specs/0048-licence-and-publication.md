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

the operator framed uas as "the containerd of agent harnesses" and ruled its licence. The userspace
crates publish under `Apache-2.0 OR MIT`. The kernel-side BPF program sources and object stay
`Dual MIT/GPL`, SPDX `GPL-2.0-only OR MIT` [reported: ruling in lane w25:p1, 2026-10-11 about
00:35Z, relayed in the integrating lane's brief]. The ruling does not change visibility. The
repository stays private, and a public cut is a new repository with a clean tree, not a
visibility flip [reported: cortex lane w10:p0 relay of the operator, mesh ead30203, 2026-10-10 about
23:05Z].

Today the workspace is proprietary and spec 0004 requires that. The crates.io placeholder already
says `MIT OR Apache-2.0` (spec 0045 D8). This spec sets the licence layout, the kernel boundary,
the route to a public tree, the contribution rule, and what stays out of that tree.

### Facts

Measured 2026-10-11 on ceres at development `340475b`.

- **F1 [verified]:** `LICENSE` has 16 lines. Line 1 reads "Copyright (c) 2026 unsigned.gg. All rights reserved."; line 3 reads "PROPRIETARY SOFTWARE LICENSE". GitHub reports the licence as `NOASSERTION`. Commands: `head -3 LICENSE; wc -l LICENSE`; `gh api repos/unsigned-gg/uas --jq .license.spdx_id`.
- **F2 [verified]:** `Cargo.toml` `[workspace.package]` sets `publish = false` and `license = "LicenseRef-Proprietary"` (lines 11 and 12). The members are `crates/*` and `xtask`: 11 packages. Each sets `license.workspace = true` and `publish.workspace = true`; none sets its own licence. Command: `grep -E '^(name|license|publish)' crates/*/Cargo.toml xtask/Cargo.toml`.
- **F3 [verified]:** The BPF program package `uas-ebpf-ebpf` is its own workspace. `crates/uas-ebpf/ebpf/Cargo.toml` has a `[workspace]` table and its own `Cargo.lock`, and the root glob `crates/*` does not reach it. It sets `publish = false` and `license = "MIT OR Apache-2.0"`. That disagrees with its object (F4) and with spec 0004 (F8). Command: `cat crates/uas-ebpf/ebpf/Cargo.toml`.
- **F4 [verified]:** `crates/uas-ebpf/ebpf/src/main.rs` lines 328 to 330 put `static LICENSE: [u8; 13] = *b"Dual MIT/GPL\0"` in the `license` link section. The committed object has a 13-byte `license` section (`objdump -h crates/uas-ebpf/ebpf/uas-ebpf.o`), and `strings crates/uas-ebpf/ebpf/uas-ebpf.o | grep -i gpl` prints `Dual MIT/GPL`. Linux v6.6 `Documentation/process/license-rules.rst` lines 456 to 458 read the string as "dual licensed under a GPL v2 variant or MIT license choice". `GPL-2.0-only OR MIT` is the ruled SPDX form. `include/linux/license.h` lists the string as GPL-compatible.
- **F5 [verified]:** The program calls `bpf_get_current_comm`, `bpf_get_current_pid_tgid`, `bpf_get_current_uid_gid`, `bpf_get_ns_current_pid_tgid`, `bpf_ktime_get_ns` and `bpf_probe_read_kernel_str_bytes`, plus map and ring buffer helpers through aya (`grep -oE '\bbpf_[a-z_]+' crates/uas-ebpf/ebpf/src/main.rs | sort | uniq -c`). The aya-ebpf 0.2.1 wrapper `bpf_probe_read_kernel_str_bytes` calls `bpf_probe_read_kernel_str` (`src/helpers.rs` line 558, cargo registry). In Linux v6.6 (ceres runs 6.6.87), that helper is the only GPL-only one in use: `.gpl_only = true` at `kernel/trace/bpf_trace.c` line 273. `bpf_ktime_get_ns` is `.gpl_only = false` (`kernel/bpf/helpers.c` line 178), as are the comm, pid_tgid, uid_gid and ns pid helpers (lines 270, 229, 249, 617). The ring buffer protos set no flag. The verifier rejects a GPL-only call from a non-GPL program (`kernel/bpf/verifier.c` lines 9558 to 9562). The files came from `raw.githubusercontent.com/torvalds/linux/v6.6/`; per helper: `grep -n -A3 '<helper>_proto = {' <file> | grep gpl_only`.
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

- **F11 [verified]:** No `LICENSE-APACHE`, `LICENSE-MIT` or `NOTICE` exists. The tracked licence files are `LICENSE` and `vendor/leanspec/LICENSE` (`git ls-files | grep -iE '(^|/)(licen[cs]e|notice|copying)'`). None of the 121 tracked `.rs` and `.sh` files outside `vendor/` carries `SPDX-License-Identifier`: `git ls-files -z -- '*.rs' '*.sh' ':!vendor' | xargs -0 grep -L 'SPDX-License-Identifier' | wc -l` prints 121.
- **F12 [verified]:** `vendor/` holds four directories. Their READMEs state: `leanspec` MIT (codervisor, with `LICENSE`); `cortex` "Proprietary (Cerebral)"; `soma` and `saga` "Business Source License 1.1 (unsigned-gg)". Cortex, soma and saga entered history in `7c17f69` on 2026-10-08 (`git log --diff-filter=A -- vendor/cortex vendor/soma vendor/saga`). Only `crates/uas-engine/build.rs` line 9 reads one of them, `vendor/cortex/proto`, and only under the feature `cortex-grpc`. `make check` builds default features (Makefile lines 23 to 38). Command: `git grep -n -E 'vendor/(cortex|soma|saga)' -- crates xtask Makefile .github`.
- **F13 [verified]:** crates.io `uas` 0.1.0 has licence `MIT OR Apache-2.0` and repository `https://github.com/unsigned-gg/uas`. Command: `curl -s https://crates.io/api/v1/crates/uas | jq -r '.versions[0].license, .crate.repository'`. That expression has the same two arms as the ruled `Apache-2.0 OR MIT`. Spec 0045 F10 records the rest.
- **F14 [verified]:** `gh api repos/unsigned-gg/uas --jq .private` prints `true`. `git ls-remote origin 'refs/pull/*/head' | wc -l` prints 139; the relay counted 125 at 23:05Z. The regex `[a-z0-9-]+\.tail[0-9a-f]+\.ts\.net` matches one line each in `blackwall.toml`, spec 0013 and spec 0017 (`git grep -c -E <regex>`). `docs/migration/` holds 10 files from spec 0037: settings exports, `cutover.md` and `uas-impact-list.md`. Every pull-request job in `ci.yml`, `codeql.yml` and `labeler.yml` runs on the self-hosted group `ent-default` behind the guard `github.event.pull_request.head.repo.full_name == github.repository`, which skips fork pull requests (`grep -n -E 'runs-on|head.repo.full_name' .github/workflows/*.yml`).
- **F15 [verified]:** containerd is `Apache-2.0` (`gh api repos/containerd/containerd --jq .license.spdx_id`). Its contribution guide, `containerd/project` `CONTRIBUTING.md`, asks for a `Signed-off-by:` line on every commit under the Developer Certificate of Origin (DCO, line 189). `grep -n -w CLA` on that file prints nothing; control: `grep -n 'Developer Certificate'` prints line 189.

## Design

Two licences, one boundary. Userspace code is permissive. The BPF program is dual GPL and MIT,
because the kernel demands a GPL-compatible licence for one helper it calls. The program reaches
userspace only as bytes loaded at run time. The public tree is exported, not flipped.

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Every root-workspace package (the `uas-*` crates and `xtask`) is licensed `Apache-2.0 OR MIT`. The BPF program, its source under `crates/uas-ebpf/ebpf/` and the object `uas-ebpf.o`, is licensed `GPL-2.0-only OR MIT`, and its `license` section keeps the kernel string `Dual MIT/GPL` (F4). This replaces spec 0004 line 55 and its 2026-10-09 position (F8). `deny.toml` still denies copyleft dependencies. | The kernel needs a GPL-compatible licence for `bpf_probe_read_kernel_str` (F5). The MIT arm keeps the object usable by a permissive loader. | ruled: the operator, lane w25:p1, 2026-10-11 about 00:35Z [reported: integrating lane brief] |
| D2 | Layout. The root holds `LICENSE-APACHE` and `LICENSE-MIT`; the proprietary `LICENSE` goes. `crates/uas-ebpf/ebpf/LICENSE` holds the GPL-2.0 and MIT texts and offers either. `[workspace.package]` sets `license = "Apache-2.0 OR MIT"`, and every root package keeps `license.workspace = true` (F2). The BPF package cannot inherit (F3) and sets `license = "GPL-2.0-only OR MIT"`. Every tracked `.rs` and `.sh` file outside `vendor/` carries an SPDX line in its first two lines: `GPL-2.0-only OR MIT` under `crates/uas-ebpf/ebpf/`, `Apache-2.0 OR MIT` elsewhere. A root `NOTICE` names the copyright holder as `LICENSE` line 1 does today and says `vendor/` keeps its own licences (F12). | The files land in this repo in the Plan order; D9 holds the timing question. | layout: integrating lane brief, 2026-10-11; `vendor/` clause: uas-engineer, 2026-10-11 (F12) |
| D3 | Kernel boundary. `uas-ebpf` embeds the object as bytes and the kernel loads it through `bpf(2)` at run time (F7). No symbol links a userspace crate to the program, and no crate links libbpf (F9). The kernel's BPF licensing document treats the two as separate executable processes (F7), and the embedded copy is available under its MIT arm, so no userspace crate takes GPL terms. Any future LSM or struct_ops program, or one that calls a kfunc, declares `Dual MIT/GPL` (F6). | The one GPL-only helper in use is `bpf_probe_read_kernel_str`; `bpf_ktime_get_ns` is not GPL-only in v6.6 (F5). | integrating lane brief, 2026-10-11; embedding and helper facts: uas-engineer, 2026-10-11 |
| D4 | Publication route. The public tree is a new repository with a fresh history, written by the export step (Plan, package 3). This repository's visibility does not change: its history holds the vendored contracts (F12) and 139 pull-request refs (F14). It stays private, and the cut does not change it. Creating the public repository and its first push are the operator's acts, as spec 0037 D1 keeps visibility with him. | A flip publishes every commit and pull-request ref. A history rewrite here breaks spec 0037 D5. | ruled: the operator [reported: cortex lane w10:p0, mesh ead30203, 2026-10-10 about 23:05Z]; spec 0037 D1, D5 |
| D5 | Contributions. The public repository requires a DCO sign-off on every commit of a pull request: a `Signed-off-by:` trailer that matches the commit author, checked by a required CI job. No CLA. Commits stay GPG-signed (AGENTS.md rule 5). This private repo does not add the trailer rule. | containerd's model (F15). Each contributor certifies the right to submit under the file's licence, with no contract. | integrating lane brief, 2026-10-11; the required job: uas-engineer, 2026-10-11 |
| D6 | crates.io `uas`. At its next publish the crate carries the D1 expression; today's `MIT OR Apache-2.0` has the same arms (F13). Its `repository` moves to the public repository once that exists. Its README names the reporting address of spec 0045 D3. This closes spec 0045 D8: the crate is the publication vehicle (spec 0037 D13), not a name hold. Publishing stays the operator's act. | The workspace, not the crate, was out of line (F2). A reporter who follows the private link reaches nothing. | integrating lane brief, 2026-10-11; spec 0037 D13; README clause: uas-engineer, from spec 0045 D8's first option |
| D7 | The export step leaves these out or replaces them, and fails on any it misses: (a) `vendor/cortex/`, `vendor/soma/` and `vendor/saga/`, and the `cortex-grpc` feature of `uas-engine` with the code behind it, since its `build.rs` reads `vendor/cortex/proto` (F12); `vendor/leanspec/` (MIT) stays with its `LICENSE`; (b) tailnet hostnames, replaced by a placeholder host (F14); (c) operator names in decision attributions, replaced by a role, while the copyright line, the `authors` field and the `SECURITY.md` contact stay; (d) `docs/migration/`, the spec 0037 settings export. | Each item is private to the estate or licensed against publication. A default build needs none of (a) (F12). | integrating lane brief, 2026-10-11; `leanspec` and `cortex-grpc`: uas-engineer, 2026-10-11 (F12) |
| D8 | The licence gate checks this workspace's own licence. `deny.toml` sets `[licenses.private] ignore = false` and drops the "proprietary and unpublished" comment. The allow list does not change, so GPL stays out (F10). `make deny` also runs `cargo deny --manifest-path crates/uas-ebpf/ebpf/Cargo.toml check --config deny.toml licenses`. | Today a GPL workspace crate passes unseen, and the BPF workspace is never read (F10). F3 is a wrong field that passed. The brief named a new allow list; F10 shows none is needed. | uas-engineer, 2026-10-11 (F10) |
| D9 | Timing of D2 and D8. The brief's Plan lands them in this private repo before the export tooling. If the operator wants the licence files to wait for the cut, those Plan items move behind package 3 and land in the export tree only. | The ruling, as relayed, fixes the licence and the visibility, not the day the files change. | the operator to rule; this spec follows the brief's Plan order until then |

### Requirements (EARS)

- **[REQ-0048-01]** (Ubiquitous): The root `Cargo.toml` shall set `license = "Apache-2.0 OR MIT"` in `[workspace.package]`, and every root-workspace package shall inherit it with `license.workspace = true`.
- **[REQ-0048-02]** (Ubiquitous): The package `crates/uas-ebpf/ebpf` shall set `license = "GPL-2.0-only OR MIT"`, and its object shall hold `Dual MIT/GPL` in its `license` section.
- **[REQ-0048-03]** (Ubiquitous): The repository root shall hold `LICENSE-APACHE`, `LICENSE-MIT` and `NOTICE`; `crates/uas-ebpf/ebpf/LICENSE` shall hold the GPL-2.0 and MIT texts; `NOTICE` shall name the copyright holder and say that `vendor/` keeps its own licences.
- **[REQ-0048-04]** (Ubiquitous): Every tracked `.rs` and `.sh` file outside `vendor/` shall carry an `SPDX-License-Identifier` line in its first two lines, `GPL-2.0-only OR MIT` under `crates/uas-ebpf/ebpf/` and `Apache-2.0 OR MIT` elsewhere.
- **[REQ-0048-05]** (Unwanted): IF a root-workspace package or the BPF package declares a licence expression with no arm in the `deny.toml` allow list, THEN `make deny` shall exit non-zero and name the package.
- **[REQ-0048-06]** (Ubiquitous): No userspace crate shall link libbpf or a GPL-licensed library; userspace shall reach the BPF program only as object bytes loaded through the `bpf` system call.
- **[REQ-0048-07]** (Optional feature): WHERE a BPF program of type LSM or struct_ops, or one that calls a kfunc, is added, the program shall declare `Dual MIT/GPL` and its package shall stay `GPL-2.0-only OR MIT`.
- **[REQ-0048-08]** (State-driven): WHILE the public repository does not exist, this repository shall stay private, and no lane shall change its visibility or create the public repository.
- **[REQ-0048-09]** (Event-driven): WHEN the export step runs, it shall write a tree with a single-commit history that holds no `vendor/cortex/`, `vendor/soma/`, `vendor/saga/`, `docs/migration/` or `cortex-grpc` feature and no tailnet hostname, and `make check` shall pass on that tree.
- **[REQ-0048-10]** (Unwanted): IF the export tree holds any item D7 names, THEN the export step shall exit non-zero, name the file, and push nothing.
- **[REQ-0048-11]** (Event-driven): WHEN a pull request opens on the public repository, a required check shall fail on any commit with no `Signed-off-by:` trailer that matches its author.
- **[REQ-0048-12]** (Event-driven): WHEN the crates.io `uas` crate is next published, its licence shall have the arms `Apache-2.0` and `MIT` only, and its `repository` shall name the public repository once that exists.

## Plan

Package 1, spec amendments, before any file changes (rule 12's order):

- [ ] Spec 0004: replace the line 55 requirement with D1; amend the 2026-10-09 position (lines 159 to 162) and the `aya-ebpf` row (line 179) so the program package is `GPL-2.0-only OR MIT`. Copyleft stays denied for dependencies.
- [ ] Spec 0045: D8 points at this spec's D6.

Package 2, licence files and gate, one PR into `development` (D1, D2, D8):

- [ ] Add `LICENSE-APACHE`, `LICENSE-MIT`, `NOTICE` and `crates/uas-ebpf/ebpf/LICENSE`; remove `LICENSE`.
- [ ] Set the workspace licence and the BPF package licence (REQ-0048-01, 02).
- [ ] Add the SPDX line to the 121 files of F11. Add a `spdx-check` target to `make hygiene` that fails on a missing line or the wrong expression for the path.
- [ ] `deny.toml`: `ignore = false`; `make deny`: the BPF workspace leg (D8).
- [ ] One CHANGELOG line under `[Unreleased]`.

Package 3, export tooling, a later PR (D4, D7):

- [ ] `cargo xtask export --out <dir>`: `git archive` of `main` into `<dir>`; remove the D7 paths and the `cortex-grpc` feature; replace hostnames and attributions; `git init` with one commit; run the D7 check and `make check`. It pushes nothing.
- [ ] A DCO workflow for the public repository, written into the export tree only (D5).

The cut, the operator's acts:

- [ ] Create the public repository and push the export; set its required checks, the DCO job among them; enable private vulnerability reporting (spec 0045 D3).
- [ ] Publish `uas` per D6.

## Test

| Requirement | Negative control |
|---|---|
| REQ-0048-01, 05 | On a `git archive` copy with `ignore = false`: today's tree exits 4 with 11 rejections; the D1 licence exits 0; one crate at `GPL-2.0-only` exits 4 (F10, measured 2026-10-11). Goes red if `ignore = true` returns: the GPL crate then passes (F10). |
| REQ-0048-02, 05 | `cargo deny --manifest-path crates/uas-ebpf/ebpf/Cargo.toml check --config deny.toml licenses` exits 0 at `GPL-2.0-only OR MIT` and 4 at `GPL-2.0-only` (F10). `strings crates/uas-ebpf/ebpf/uas-ebpf.o \| grep -cx 'Dual MIT/GPL'` prints 1. A changed `LICENSE` static, rebuilt and committed, makes it print 0; left unbuilt, it fails the `ebpf-live` drift step. Red today: the manifest says `MIT OR Apache-2.0` (F3). |
| REQ-0048-03 | `test -f` on the four files exits 0. Red today: none exists (F11). |
| REQ-0048-04 | The SPDX leg of the command below prints nothing. Red today: it lists 121 files (F11). Guard: remove the line from one file and it lists that file; put `Apache-2.0 OR MIT` in `ebpf/src/main.rs` and `spdx-check` fails. |
| REQ-0048-06 | `grep -rn libbpf Cargo.lock crates/uas-ebpf/ebpf/Cargo.lock` prints nothing; control: `grep -c '^name = "aya"' Cargo.lock` prints 1 (F9). |
| REQ-0048-07 | Not exercisable: no LSM, struct_ops or kfunc program exists (F6). The kernel is the control: `bpf_lsm.c` rejects a non-GPL LSM program at load. |
| REQ-0048-08 | `gh api repos/unsigned-gg/uas --jq .private` prints `true` (F14). Red if it prints `false` before the public repository exists. |
| REQ-0048-09, 10 | On the export tree: `ls -d vendor/cortex vendor/soma vendor/saga docs/migration` lists nothing; `grep -rlE '[a-z0-9-]+\.tail[0-9a-f]+\.ts\.net' .` prints nothing; `git rev-list --count HEAD` prints 1; `make check JOBS=2` exits 0. Positive control, this tree on 2026-10-11: the `ls` lists 4 paths and the grep 3 files. Guard: plant one hostname in the export input; the export exits non-zero. |
| REQ-0048-11 | On the public repository, a pull request with one commit lacking the trailer fails the DCO check. Not exercisable until the repository exists. |
| REQ-0048-12 | `curl -s https://crates.io/api/v1/crates/uas \| jq -r .crate.repository` prints the public URL. Red today: it prints this private repository (F13, F14). |

```bash
make deny hygiene JOBS=2 && \
  test -f LICENSE-APACHE -a -f LICENSE-MIT -a -f NOTICE -a -f crates/uas-ebpf/ebpf/LICENSE && \
  test -z "$(git ls-files -z -- '*.rs' '*.sh' ':!vendor' | xargs -0 grep -L 'SPDX-License-Identifier')" && \
  strings crates/uas-ebpf/ebpf/uas-ebpf.o | grep -qx 'Dual MIT/GPL'
```

Passing means exit 0 after package 2. Today it fails at the `test -f` leg (F11).

## Notes

- The ruling does not decide: the date of the public cut; the name of the public repository;
  whether `uas-operator` (spec 0038) publishes separately; the copyright holder line for the
  public tree (`LICENSE` names "unsigned.gg" while the repo sits in
  `unsigned-gg`); whether docs and specs take the root licence or another one. Each is the operator's.
- No counsel has reviewed D3. It rests on the kernel's own document and the object's MIT arm
  [asserted].
- Open: `uas-ebpf` cannot publish as packaged (F7). Which crates publish is spec 0039's subject.
- Open: today every pull-request job runs on self-hosted runners behind a same-repo guard (F14).
  The runners of the public repository are spec 0005's subject; the export keeps the guards.
- Brief corrections: `bpf_ktime_get_ns` is not GPL-only in v6.6 (F5); the repo has 139
  pull-request refs, not 125 (F14); no allow-list change is needed (F10); the BPF package
  declares `MIT OR Apache-2.0`, not an inherited licence (F3); `vendor/leanspec` is MIT and stays.
- Out of scope: trademark use of the name `uas`, an SBOM licence field (spec 0045 D4), and
  relicensing the vendored contracts.
