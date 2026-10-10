---
status: planned
created: 2026-10-10
tags: [pool, cache, cargo, sccache, rust-analyzer, landlock, disk, phase-3]
priority: high
depends_on: [0002-verification-gates-and-evidence, 0005-ci-cache-and-runners, 0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0020-build-job-cap, 0028-branchless-crdt-worktrees, 0036-slot-arbitration-across-processes, 0041-slot-memory-pids-and-disk-limits]
---

# 0047-slot-build-cache-retention

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

The research design promises each warm slot "a warm `rust-analyzer` / LSP language server
cache" (F8). The code does the opposite. Spec 0015's `Reclaiming` runs `reset_clean`, which
deletes every entry of the worktree except `.git` (F1). A slot's `target/` dies at every
reclaim, so the next lease in that slot builds every dependency from cold. Spec 0041 D3
relies on that deletion to recover disk. No spec owns a build cache
[reported: research lane w1X:p1, mesh 5871db01, gap 2; integrating lane w25:p1, 2026-10-10].

This spec moves each slot's cargo output to a cache directory outside the worktree, which
`Reclaiming` keeps. A per-slot quota replaces reclaim as the disk bound. sccache is an
optional extra, off by default. Two facts narrow the research premise. A reset rewrites every
source file, so workspace members still recompile, incrementally, after a reclaim (F9).
rust-analyzer keeps no on-disk cache of its own that the pool can name (F10).

### Facts

Measured 2026-10-10 on ceres (WSL2 kernel 6.6.87.2) at development `6e7510e`.

- **F1 [verified]:** `reset_clean` loops over the worktree, skips `.git`, and calls `remove_dir_all` or `remove_file` on every other entry (`crates/uas-pool/src/worktree.rs` lines 190 to 208). Command: `sed -n 190,208p crates/uas-pool/src/worktree.rs`. Spec 0041 F7 reads the same lines.
- **F2 [verified]:** No code in `uas-pool` or `uas-sandbox` sets `CARGO_TARGET_DIR` or `RUSTC_WRAPPER` or names sccache. Command: `grep -rnE 'CARGO_TARGET_DIR|RUSTC_WRAPPER|sccache' crates/uas-pool/src crates/uas-sandbox/src` prints nothing (exit 1); the control `grep -rn reset_clean crates/uas-pool/src` prints line 183.
- **F3 [verified]:** sccache 0.15.0 is installed at `~/.cargo/bin/sccache`. Commands: `sccache --version`, `which sccache`. The binary holds the strings `SCCACHE_DIR`, `SCCACHE_CACHE_SIZE` and `SCCACHE_SERVER_UDS` (`grep -c -a -F <name> ~/.cargo/bin/sccache` prints 1 or more for each).
- **F4 [reported]:** Spec 0005 uses sccache only in CI, with a Cloudflare R2 bucket (spec 0005 lines 26 and 98 to 105). No spec names a local sccache.
- **F5 [reported]:** Spec 0010 lines 112 to 114 pass `~/.cargo/bin`, `~/.cargo/registry` and `~/.rustup` into the ruleset, never `~/.cargo` whole, and say the pool owns that list. Spec 0010 line 87: `allow_read_write` grants every access the effective ABI offers. Line 100 gives the workspace root every access. D7 (line 59) gives scratch and device paths write without execute and lets only the workspace and the system paths execute.
- **F6 [reported]:** Spec 0041 F5: a clean checkout is 3.4 MiB; one debug build of the CLI makes `target` 246 MiB. Spec 0041 D3: no per-slot disk quota, a `min_free_bytes` floor of 20 GiB. Re-measured today: the checkout is 3.4 MiB (`du -sh --exclude=.git --exclude=target .`), the shared checkout's `target` is 7.5 GiB (`du -sh target`), and `/` has 298 GiB free of 1007 GiB (`df -h /`).
- **F7 [reported]:** Spec 0028 line 15 states that physical worktrees cause disk exhaustion and memory starvation on ceres; lines 17 and 20 make Phase 8 replace them with in-memory delta streams for transient tasks. Spec 0028 F5, the source of that premise, is itself `[asserted]`.
- **F8 [verified]:** Research section 3.1, line 87, lists "A warm `rust-analyzer` / LSP language server cache" in every pre-provisioned slot. Command: `sed -n 79,95p /home/ctodie/projects/science/research/research/agentic-systems/designs/rust-native-agent-substrate.md`. The directory is not a git checkout, so the citation carries no sha.
- **F9 [verified]:** Retention measured. In a scratch worktree with `CARGO_TARGET_DIR` outside it, `cargo build -p uas-domain -j 2`, then a reset like `reset_clean` (`find . -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} + && git checkout -- .`), then `cargo build -p uas-domain -j 2 -v`: all nine registry packages print `Fresh`; `uas-domain` prints `Dirty ... the file crates/uas-domain/src/slot.rs has changed` and recompiles. A second reset cycle, run the same way, finished its rebuild in 0.12 s. Checkout gives every source file a new mtime, and cargo judges path packages by mtime.
- **F10 [verified]:** rust-analyzer reads no cache-directory variable. Command: `grep -c -a -F RA_CACHE_DIR <binary>` prints 0 for rust-analyzer 1.95.0 (rustup) and 0.3.2929 (Zed download); the controls `RA_LOG` and `RA_PROFILE` print 1. `rust-analyzer --print-config-schema` documents `rust-analyzer.cargo.targetDir` (a client setting; `true` uses "a subdirectory of the existing target directory") and `rust-analyzer.cachePriming.enable` ("Warm up caches on project load", default `true`). That the analysis index lives only in memory is `[asserted]`.
- **F11 [verified]:** The pinned toolchain has no rust-analyzer. In the repo, `rust-analyzer --version` fails with `Unknown binary 'rust-analyzer' in official toolchain '1.94.1-x86_64-unknown-linux-gnu'`; `rust-toolchain.toml` lists `components = ["rustfmt", "clippy"]`.
- **F12 [verified]:** `make ebpf-build` builds the nested workspace `crates/uas-ebpf/ebpf` on `nightly-2026-10-06` and copies from the hard-coded path `crates/uas-ebpf/ebpf/target/bpfel-unknown-none/release/uas-ebpf-ebpf` (`sed -n 80,83p Makefile`). `cargo metadata --no-deps --manifest-path crates/uas-ebpf/ebpf/Cargo.toml` reports `target_directory` as that nested `target` without the variable, and as `/tmp/x-cache/target` with `CARGO_TARGET_DIR=/tmp/x-cache/target`. An exported variable breaks that copy.
- **F13 [verified]:** A size walk of a 7.5 GiB `target` holding 23091 entries took 0.04 s with a warm dentry cache. Commands: `/usr/bin/time -f '%e s' du -sb target`, `find target -xdev | wc -l`. A cold walk was not measured.
- **F14 [verified]:** `AuditReceipt` is defined at `crates/uas-ebpf/src/audit.rs` line 87, and its `cpu_cap` field carries a named serde default (`default_cpu_cap`). `PoolConfig` (`crates/uas-pool/src/slot.rs` line 18) has only `max_slots`, `idle_suspend_duration` and `cgroup_base`. `sha2` 0.11.0 is pinned for `uas-ebpf` and `uas-router` only (spec 0004 line 181).
- **F15 [verified]:** `uas-pool` mints no `SlotId`. `grep -n SlotId crates/uas-pool/src/slot.rs` shows `create_slot(&self, id: SlotId)` at line 129, which takes any id the caller passes, and only readers besides. `grep -rn 'SlotId::new' crates/*/src` shows the CLI building ids from `--slot` (`crates/uas-cli/src/main.rs` lines 267, 326 and 550). A cache keyed by `SlotId` has no bound on how many exist.
- **F16 [verified]:** On ceres, `~/.cache`, `/tmp` and the shared checkout share one filesystem. Command: `stat -f -c %i ~/.cache /tmp /home/ctodie/projects/unsigned/uas` prints one id three times. Other hosts can differ.
- **F17 [verified]:** Cargo executes build scripts and test binaries from under `target`, and Landlock refuses `execve` beneath a path granted without execute. The shared checkout's `target/debug/build` holds 52 `build-script-build` executables (`ls target/debug/build/*/build-script-build | wc -l`); its `target/debug/deps` holds the test executables, built before the rename (`find target/debug/deps -maxdepth 1 -type f -perm -u+x`). A Python ctypes probe that handles only execute, under a ruleset granting it beneath `/usr`, `/lib`, `/lib64`, `/bin` and one scratch directory, runs a copy of `/bin/true` there (exit 0) and gets `EACCES` for a copy in a sibling directory. The probe script was not kept; the REQ-0047-03 Test row proves the same in the pool.

## Design

Three paths, with `C` = `${XDG_CACHE_HOME:-~/.cache}/uas`:

| Name | Path | Holds |
|---|---|---|
| Slot cache | `C/slots/<i>/` | One key directory per base repository and toolchain, plus `.stamps/`. |
| Key directory | `C/slots/<i>/<repo16>-<channel>/` | `target/`, the slot's `CARGO_TARGET_DIR`. |
| sccache store | `C/sccache/` | sccache objects, shared by every slot (D5, off by default). |

`<i>` is the spec 0036 host slot index (`slot-<i>.lock`, `0 <= i < host_max_slots`), not the
caller's `SlotId` (F15). At most `host_max_slots` slot caches exist, and a slot stays warm
whatever id the caller passes. The pool reads or writes `C/slots/<i>/` only while it holds
`slot-<i>.lock` (spec 0036 D3), so no other process evicts or builds into it.

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Each host slot has a slot cache at `C/slots/<i>/`, outside the worktree, created mode `0700`, used only under `slot-<i>.lock`. `Reclaiming` deletes nothing in it; only D4 eviction removes data. The worktree stays disposable. | F1 deletes everything inside the worktree. One cache per slot, not per host, because two slots building into one `target` would wait on cargo's build-directory lock `[asserted]`. The host index, not `SlotId`, bounds the count (F15) and gives one process exclusive use. | integrating lane w25:p1, 2026-10-10; keyed by host index instead of `slot_id` by the spec lane, from F15 |
| D2 | When the pool spawns a worker, it exports `CARGO_TARGET_DIR=<key>/target` and adds the key directory to the slot's Landlock ruleset with `allow_read_write`, execute included. This amends spec 0010 D7 (line 59): the key directory joins the workspace and the system paths as a path that can execute. It adds no other path under `~/.cache` except the D5 store, which stays without execute. The pool creates the key directory, and under D5 the store, before it builds the ruleset, because the sandbox refuses a missing path (`SandboxError::PathMissing`, `crates/uas-sandbox/src/canon.rs` line 17). | Cargo executes build scripts and test binaries from `target`, and Landlock refuses that without execute (F17). So a grant without execute was rejected: every build with a build script would fail, and the only other writable path that can execute is the workspace, where F1 deletes `target`. The cost is a path a worker can write and execute that outlives the lease (D10). Granting the key directory, not the slot cache, keeps one repository's worker out of another's cache. Also amends the spec 0010 path list, which the pool owns. | proposed: integrating lane w25:p1, 2026-10-10; awaits Christian; narrowed to the key directory by the spec lane |
| D3 | The key directory name is `<repo16>-<channel>`. `repo16` is the first 16 hex digits of SHA-256 over the base repository's canonical path. `channel` is `[toolchain] channel` from the base repository's `rust-toolchain.toml` or `rust-toolchain`, else `default`, with every byte outside `[A-Za-z0-9._-]` replaced by `_`. | A slot reused for another repository or toolchain gets a sibling directory, which is also the eviction unit. Cargo's own metadata hashes already keep artifacts of different toolchains apart `[asserted]`; the key bounds thrash and gives D4 a unit. SHA-256 because the key is on disk and must be stable across Rust releases. | integrating lane w25:p1, 2026-10-10 |
| D4 | `PoolConfig.cache_max_bytes`, default 20 GiB, bounds each slot cache. The pool enforces it when it provisions a worktree and in `Reclaiming` before the receipt is sealed. It walks the slot cache (a `du -sb` equivalent) and removes whole key directories, oldest `.stamps/<key>` mtime first, until the total is at most the quota. A key directory alone over the quota is removed. The pool touches `.stamps/<key>` at each lease. The spec 0041 `min_free_bytes` floor applies after eviction, to the worktree root's filesystem and, by the D11 amendment, to the slot cache's filesystem. | F6: one CLI build is 246 MiB and a full workspace build grows to GiB. With `target` out of the worktree, reclaim no longer recovers disk, so a quota must. The stamp sits outside the key directory, so a worker cannot reorder eviction. The walk is cheap (F13). Two slots at 20 GiB is 40 GiB of the 298 GiB free (F6). The two filesystems are one on ceres (F16), not on every host. | proposed: integrating lane w25:p1, 2026-10-10; the 20 GiB default awaits Christian |
| D5 | sccache is off by default. WHERE `PoolConfig.sccache` is `true` and `sccache` resolves on the worker's `PATH`, the pool exports `RUSTC_WRAPPER=sccache`, `SCCACHE_DIR=C/sccache` and `SCCACHE_CACHE_SIZE` from `PoolConfig.sccache_max_bytes` (default 10 GiB), and adds `C/sccache` to the ruleset with `allow_write`. Each slot has its own loopback-only network namespace (spec 0011), so each slot starts its own sccache server; all of them share one directory. | F3: the binary exists; F4: CI uses it only remotely. sccache stores objects and runs nothing from its directory, so write without execute suffices (spec 0010 D7). D4 does not cover this store, so its size is set explicitly. Concurrent servers on one directory being safe is `[asserted]`, not tested. | proposed: integrating lane w25:p1, 2026-10-10; shared-store safety awaits Christian |
| D6 | The pool sets no rust-analyzer variable. rust-analyzer writes its `cargo check`, build-script and proc-macro output through cargo, so the exported `CARGO_TARGET_DIR` puts it in the key directory. The harness adapter, not the pool, passes `rust-analyzer.cargo.targetDir = true` in the LSP initialization options, so that output lands in a subdirectory of `<key>/target` and does not wait on the agent's own builds. The in-memory index stays warm only while a slot is `Suspended` (spec 0015 freezer). | F10: the brief proposed `RA_CACHE_DIR`; that variable does not exist, so this row changed. The setting is a client option the pool cannot set. | spec lane, 2026-10-10, from F10 |
| D7 | In `Reclaiming`, before the receipt is sealed, the pool records `cache_bytes_before`, `cache_bytes_after` (both `u64`, the slot cache) and `cache_evicted` (`u32`, key directories removed) on the `AuditReceipt`. Each field carries `#[serde(default)]`, as `cpu_cap` does (F14). | A verifier sees whether a slow build followed an eviction. Old receipts still parse. | integrating lane w25:p1, 2026-10-10 |
| D8 | Spec 0028 Phase 8 in-memory worktrees keep the slot cache. Only the worktree changes. A build that materializes files gets the same `CARGO_TARGET_DIR`. | The cache never lived in the worktree. | integrating lane w25:p1, 2026-10-10 |
| D9 | `make ebpf-build` passes `--target-dir crates/uas-ebpf/ebpf/target` to cargo. | F12: an exported `CARGO_TARGET_DIR` breaks its copy step, and would put nightly eBPF output in a stable-keyed directory. The flag takes precedence over the variable `[asserted]`; the Test row proves it. | spec lane, 2026-10-10, from F12 |
| D10 | The cache now outlives a lease. A worker can write an artifact that a later lease of the same repository reuses, and with D5 on, every slot reuses it. No gate evidence (spec 0002) is taken from a build that read a slot cache or the sccache store. | Before this spec, `reset_clean` gave every lease a clean `target`. Persistence moves a trust boundary. | proposed: spec lane, 2026-10-10; Christian to rule |
| D11 | Amendment this spec makes to spec 0041. D3 there says "Disk has no per-slot quota"; with D4, build output in the slot cache has one. D4 is an eviction bound the pool enforces from user space at provision and at `Reclaiming`, not a filesystem quota, so D3's reason against project quotas (a mount option and root) still holds. REQ-0041-04 checks the worktree root's filesystem only; it now also checks the slot cache's filesystem, after eviction. Neither bound limits what one worker writes during a lease. | D3 rests on reclaim freeing build output (spec 0041 F7). With `target` in the slot cache, reclaim frees none of it (D1), and the slot cache can sit on another filesystem than the worktree root (F16). | proposed with D4: integrating lane w25:p1, 2026-10-10; stated as an amendment by the spec lane, critic round one; awaits Christian with D4 |

### Requirements (EARS)

- **[REQ-0047-01]** (Ubiquitous): The `uas-pool` crate shall keep each slot's build cache under `C/slots/<i>/`, where `<i>` is the spec 0036 host slot index, outside the slot worktree; it shall read or write that directory only while it holds `slot-<i>.lock`; and `Reclaiming` shall remove nothing there except by D4 eviction.
- **[REQ-0047-02]** (Event-driven): WHEN `uas-pool` spawns a worker in a slot, it shall export `CARGO_TARGET_DIR` set to `<key>/target` for the active key directory.
- **[REQ-0047-03]** (Event-driven): WHEN `uas-pool` builds a slot's Landlock ruleset, it shall first create the active key directory, then add it with `allow_read_write`, and shall add no other path under `${XDG_CACHE_HOME:-~/.cache}` except the D5 store.
- **[REQ-0047-04]** (Ubiquitous): The key directory name shall be the first 16 hex digits of SHA-256 over the canonical base repository path, a hyphen, and the sanitized toolchain channel, or `default` when the repository names none.
- **[REQ-0047-05]** (Event-driven): WHEN `uas-pool` provisions a slot worktree or a slot enters `Reclaiming`, it shall remove whole key directories, oldest stamp first, until the slot cache is at most `cache_max_bytes`.
- **[REQ-0047-06]** (Event-driven): WHEN a slot is leased, `uas-pool` shall touch `.stamps/<key>` in the slot cache, outside the key directory.
- **[REQ-0047-07]** (Unwanted): IF, after eviction, the filesystem of the worktree root or of the slot cache has less than `min_free_bytes` free, THEN `uas-pool` shall refuse `allocate` with `PoolError::DiskLow` and provision nothing. Spec 0041 defines the floor and the error; the check on the slot cache's filesystem is this spec's amendment (D11).
- **[REQ-0047-08]** (Optional): WHERE `PoolConfig.sccache` is `true` and `sccache` resolves on the worker's `PATH`, `uas-pool` shall export `RUSTC_WRAPPER`, `SCCACHE_DIR` and `SCCACHE_CACHE_SIZE` and shall add `C/sccache` to the ruleset with `allow_write`.
- **[REQ-0047-09]** (Unwanted): IF `PoolConfig.sccache` is `true` and `sccache` does not resolve, THEN `uas-pool` shall log at `warn`, export no `RUSTC_WRAPPER`, and spawn the worker.
- **[REQ-0047-10]** (Ubiquitous): The `uas-pool` crate shall export no rust-analyzer variable.
- **[REQ-0047-11]** (Event-driven): WHEN a slot enters `Reclaiming`, `uas-pool` shall record `cache_bytes_before`, `cache_bytes_after` and `cache_evicted` on the `AuditReceipt` before it is sealed.
- **[REQ-0047-12]** (State-driven): WHILE a slot serves a spec 0028 in-memory worktree, `uas-pool` shall keep the same slot cache and export the same `CARGO_TARGET_DIR` for any build that materializes files. Held on spec 0028: its test is deferred until an in-memory worktree exists (Test).

## Plan

- [ ] Spec 0004: extend the `sha2` row (line 181) to `uas-pool` before first use (rule 12).
- [ ] Order: the `uas-pool` items below land after the spec 0036 host slot arbiter, which supplies `<i>`.
- [ ] `uas-pool`: `PoolConfig` gains `cache_max_bytes`, `sccache` and `sccache_max_bytes`; key derivation (D3); slot cache creation at mode `0700` (D1).
- [ ] `uas-pool`: export `CARGO_TARGET_DIR` and, under D5, the sccache variables at spawn; add the key directory and the store to the ruleset path list.
- [ ] `uas-pool`: stamp at lease; size walk and eviction at provision and in `Reclaiming` (D4).
- [ ] `uas-ebpf`: three `AuditReceipt` fields with `#[serde(default)]` (D7).
- [ ] `Makefile`: `ebpf-build` passes `--target-dir` (D9).
- [ ] Spec 0010: amend D7 (line 59) so the key directory can execute (D2), and cite this spec at the path list (line 114). Spec 0015: `Reclaiming` keeps the slot cache. Spec 0041: amend D3 and REQ-0041-04 as D11 states.
- [ ] Spec 0028: its in-memory worktree tests adopt the REQ-0047-12 control.

## Test

| Requirement | Negative Control |
|---|---|
| REQ-0047-01, -02 | An integration test provisions a slot on this repository, runs `cargo build -p uas-domain --offline -j 2`, reclaims, runs `cargo build -p uas-domain --offline -j 2 -v`, and asserts `Fresh serde` with no `Compiling serde`. Goes red when the key directory sits inside the worktree or `CARGO_TARGET_DIR` is unset, as F1 deletes it. It does not assert `uas-domain` fresh (F9). |
| REQ-0047-01 (index) | Two pool processes take the same host index in turn with different `--slot` ids; the second finds the first's key directory. Goes red if the cache is keyed by `SlotId`. |
| REQ-0047-03 | A jailed worker creates and runs an executable under `<key>/target` (both succeed), then creates a file under a sibling key directory and under `~/.cache` (both `EACCES`). Goes red if the rule grants the slot cache or `~/.cache`, or drops execute on the key directory (D2). |
| REQ-0047-04 | A unit test pins the key for `/srv/repo` and channel `1.94.1` to a golden string, and checks that a symlink to the same repository gives the same key. Goes red if the hash, the truncation or canonicalization changes. |
| REQ-0047-05, -06 | A fixture slot cache holds three 8 MiB key directories stamped in order; with a 20 MiB quota the oldest goes and two stay. A single 30 MiB key directory under the same quota goes. Goes red if eviction is removed or orders by name or atime. |
| REQ-0047-07 | A fake free-space probe reports below the floor before eviction and above it after; `allocate` succeeds. Reporting below after eviction returns `DiskLow`. Goes red if the floor is checked before eviction, or on one filesystem only. |
| REQ-0047-08, -09 | With `sccache = true` and a `PATH` without sccache, the worker environment has no `RUSTC_WRAPPER` and the log has one `warn`. Goes red if the wrapper is exported unconditionally. |
| REQ-0047-10 | The worker environment holds no variable starting `RA_`. Goes red if one is added. |
| REQ-0047-11 | Reclaiming the over-quota fixture yields a receipt with `cache_bytes_before` above `cache_bytes_after` and `cache_evicted` 1; a receipt JSON without the fields parses. Goes red if the fields stay 0 or lose `serde(default)`. |
| REQ-0047-12 | Held on spec 0028, control deferred. Spec 0028 is `planned` and `uas-pool` has no in-memory worktree: `grep -rniE -e 'in.?memory' -e delta crates/uas-pool/src` prints nothing (2026-10-10; the control `grep -rn reset_clean crates/uas-pool/src` prints line 183). When it lands, a test leases one host index with a physical worktree, then with an in-memory one that materializes files, and asserts the same `CARGO_TARGET_DIR` both times. Goes red if the in-memory path derives `CARGO_TARGET_DIR` from the worktree or leaves it unset. |
| D9 | In a scratch worktree with no `crates/uas-ebpf/ebpf/target`, `CARGO_TARGET_DIR=$(mktemp -d) make ebpf-build` exits 0, the nested `target/bpfel-unknown-none/release/uas-ebpf-ebpf` exists, and the temporary directory holds no `bpfel-unknown-none`. It runs in a scratch worktree because the recipe copies the build onto the tracked `uas-ebpf.o` and rewrites the tracked `uas-ebpf.o.sha256` on every successful build (`sed -n 80,84p Makefile`, `git ls-files crates/uas-ebpf/ebpf`). It needs the pinned `nightly-2026-10-06` toolchain with `rust-src`, and `bpf-linker`. Goes red without `--target-dir`: the `cp` finds no nested artifact. |

```bash
cargo nextest run -p uas-pool cache -j 2 && cargo nextest run -p uas-ebpf receipt -j 2
```

## Notes

- Workspace members recompile after every reclaim (F9), because checkout rewrites every mtime. The retained incremental state keeps that cheap (0.12 s for `uas-domain`). Making them `Fresh` needs a reset that leaves unchanged files alone; that change belongs to spec 0015.
- Rejected: a `target` symlink inside the worktree. A test of the `reset_clean` loop showed it removes the link every cycle and keeps the cache, so the pool would recreate the link at each lease, and it covers only the root workspace.
- Any repository script that hard-codes a `target/` path breaks under the exported variable, as `ebpf-build` does (F12). The variable also overrides a repository's `build.target-dir` `[asserted]`.
- sccache does not cache incrementally compiled crates `[asserted]`. In debug builds it helps only dependencies, which the key directory already keeps. Its gain is sharing them across slots and keys, which is why D5 is off by default.
- rust-analyzer needs a binary outside the pinned toolchain (F11). Which binary a slot gets is the harness adapter's choice, not this spec's.
- If `host_max_slots` shrinks, the slot caches at higher indices are left behind. The operator removes them; this spec adds no sweep.
- Two rows amend other specs: D2 loosens spec 0010 D7 so the key directory can execute, and D11 amends spec 0041 D3 and REQ-0041-04. Both await Christian: D2 directly, D11 with D4.
- REQ-0047-12 is held on spec 0028. Its Test row names the control to adopt when the in-memory worktree lands.
- Parked from the research lane's list: SVID pre-staged pools (identity is Allen's, spec 0029); IFC taint tracking (research only).
