---
status: in-progress
created: 2026-10-10
tags: [sandbox, seccomp, syscall, landlock, escape-suite, phase-2]
priority: high
depends_on: [0004-dependency-and-licence-policy, 0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation, 0012-sandbox-escape-suite, 0014-ebpf-priority-clamping-and-audit-probes]
---

# 0043-seccomp-syscall-filter-for-slot-workers

> **Status**: in-progress, **Priority**: high, **Created**: 2026-10-10

## Overview

The slot sandbox confines a worker with Landlock (files, spec 0010), a network namespace
(spec 0011) and `no_new_privs`. It does not filter system calls. A worker can still call
`ptrace` on a sibling, `keyctl` on the session keyring, `bpf`, `mount` inside a user namespace,
`perf_event_open`, `io_uring_setup` or `open_by_handle_at`, none of which Landlock mediates.
The research names a seccomp-BPF layer as part of the sandbox and no spec owns it [reported:
research lane w1X:p1, mesh 924be679, citing `repos/agent-substrate.md` sections 2 and 3;
integrating lane w25:p1 gap analysis, 2026-10-10].

This spec adds a seccomp-BPF deny-list filter to `uas-sandbox`, states its place in the
launch order, its failure mode, and the escape-suite rows that prove it.

### Facts

Measured 2026-10-10 on ceres (WSL2 kernel 6.6.87.2) at development `bd31291`.

- **F1 [verified]:** No spec and no crate installs a seccomp filter. Commands, run before this spec was added: `grep -ilE seccomp specs/*.md` printed only 0005 and 0011, both describing a filter the CI pod or the host already has; `grep -rnE 'seccomp' crates/*/src --include=*.rs` prints one comment, `crates/uas-sandbox/src/launcher.rs` line 20.
- **F2 [verified]:** The kernel supports filter mode and this shell already runs under one. Command: `grep -E Seccomp /proc/self/status` prints `Seccomp: 2` and `Seccomp_filters: 1`. A child inherits that filter; a new filter stacks on it.
- **F3 [verified]:** The launch order is: `netns::isolate` installs a `pre_exec` hook that calls `unshare(NEWUSER | NEWNET)` when plain `NEWNET` is refused (`crates/uas-sandbox/src/netns.rs` lines 48-72); `launcher::launch` installs `restrict_child`, which applies the Landlock ruleset and refuses the launch unless `no_new_privs` is set (`launcher.rs` lines 38, 67-90). Spec 0011 requires the netns hook before Landlock.
- **F4 [reported]:** Spec 0012 lines 84-96: a jailed worker can `connect` to a pathname Unix socket outside the workspace, because Landlock below ABI v6 does not mediate it. The escape suite records that row as a measurement, not a gate.
- **F5 [verified]:** `seccompiler` 0.5.0 (rust-vmm) is licensed `Apache-2.0 OR BSD-3-Clause` and builds the BPF program in Rust without linking `libseccomp`. Command: `curl -s https://crates.io/api/v1/crates/seccompiler | jq '.crate.max_version, .versions[0].license'`. It is not in spec 0004's tables: `grep -n seccomp specs/0004-*.md` prints nothing.
- **F6 [verified]:** The kernel offers the `errno` verdict. Command: `cat /proc/sys/kernel/seccomp/actions_avail` prints `kill_process kill_thread trap errno user_notif trace log allow`.
- **F7 [reported]:** The research's "jail in under two milliseconds" figure is for the external `agent-substrate/substrate` project (`repos/agent-substrate.md` line 23), not for UAS. This spec measures its own cost and promises no number.

## Design

The filter is a deny list, not an allow list. A worker runs arbitrary harness tooling
(`cargo`, `git`, `node`, editors' language servers), and an allow list would break one of them
every week. The deny list names the syscalls that give a confined process a way out, or a way
to read another process, and leaves every other call untouched.

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | `uas-sandbox` gains a `seccomp` module that builds the BPF programs of D2 with `seccompiler` and installs them in the child in `pre_exec`, after the netns hook and after `restrict_child`, as the last step before `exec`. The API is `build()`, which runs in the parent and returns `Programs`, and `install(&Programs)`, which runs in the child (PR #139). | `unshare(NEWUSER)` must run before the filter that denies it (F3). Landlock first keeps spec 0011's order intact. | integrating lane w25:p1, 2026-10-10; API note ruled: integrating lane w25:p1, 2026-10-11 |
| D2 | The denied syscalls and the verdict `SECCOMP_RET_ERRNO(EPERM)`: `ptrace`, `process_vm_readv`, `process_vm_writev`, `kcmp`, `keyctl`, `add_key`, `request_key`, `bpf`, `perf_event_open`, `mount`, `umount`, `umount2`, `pivot_root`, `chroot`, `open_by_handle_at`, `unshare`, `setns`, `setgroups`, `io_uring_setup`, `userfaultfd`, `kexec_load`, `kexec_file_load`, `init_module`, `finit_module`, `delete_module`, `reboot`, `swapon`, `swapoff`, `acct`, `personality`; `clone` with `CLONE_NEWUSER` in its flags argument; `socket` with domain `AF_PACKET` or type `SOCK_RAW`. `clone3` gets `SECCOMP_RET_ERRNO(ENOSYS)`, not `EPERM`: its flags sit in a user-space struct the filter cannot read, and glibc falls back to `clone` on `ENOSYS`, where the flag check applies [reported: the Docker default seccomp profile uses the same fallback]. Implementation (PR #139): a `seccompiler` filter has one match verdict, so the list is two programs, one returning `EPERM` for the list and its argument rules and one returning `ENOSYS` for `clone3` alone. On `x86_64`, three raw BPF instructions after the architecture check kill any syscall number with bit 30 set (the x32 ABI), which carries the native audit architecture and would miss every entry. The `x86_64` ABI has no `umount`, so that entry maps to `umount2`'s number. | Each entry is either a namespace or privilege change, a kernel attack surface, or a read of another process. `EPERM` lets a tool that probes a call degrade; `KILL` would turn a harmless probe into a dead worker. | integrating lane w25:p1, 2026-10-10; `kcmp`, `setgroups` and `umount` added on the research lane's critic pass, mesh 9d7c6d4b; implementation notes ruled: integrating lane w25:p1, 2026-10-11 |
| D3 | A syscall from a non-native architecture gets `SECCOMP_RET_KILL_PROCESS`. The filter is built for the build target's architecture only; `x86_64` first, `aarch64` when a runner needs it. | A 32-bit ABI on a 64-bit kernel is the classic filter bypass. `seccompiler` validates the architecture at build time. | integrating lane w25:p1, 2026-10-10 |
| D4 | If `seccomp(2)` or the program build fails, the launch is refused with `JailRefused`, as a Landlock failure is today. The sandbox does not run a worker without the filter. | Spec 0010 D5 makes the Landlock floor a hard requirement; the filter is the same kind of floor. | integrating lane w25:p1, 2026-10-10 |
| D5 | The filter is installed without `SECCOMP_FILTER_FLAG_TSYNC`; the child is single-threaded in `pre_exec`. `no_new_privs` is already set by the Landlock path, so the install needs no capability. | Nothing to synchronise; one fewer flag to reason about. | integrating lane w25:p1, 2026-10-10 |
| D6 | The escape suite (spec 0012) gains one row per denied family: `ptrace_self`, `keyctl_session`, `bpf_syscall`, `perf_open`, `mount_tmpfs`, `unshare_user` (`unshare(CLONE_NEWUSER)`), `clone_newuser` (`clone` with the flag), `raw_socket`, `io_uring`, `open_by_handle`, each expecting `EPERM`, `clone3_enosys` expecting `ENOSYS`, and `x32_syscall` (a syscall number with bit 30 set) expecting the process killed by `SIGSYS`. The `uds_connect_out` row of F4 stays a measurement: this spec does not fix it. Without the filter, `bpf_syscall`, `mount_tmpfs`, `open_by_handle` and `raw_socket` already get `EPERM` from the kernel for an unprivileged worker, so those rows cannot go red by removing the filter [reported: plan 0043 finding 8, asserted there; spec 0012 matrix]. | A deny list without a probe per entry drifts. | integrating lane w25:p1, 2026-10-10; `x32_syscall` row and the kernel note ruled: integrating lane w25:p1, 2026-10-11 |
| D7 | `seccompiler` 0.5.0 is added to spec 0004 for `uas-sandbox`, phase 2, pinned, default features; the row is approved. `libc` becomes a direct dependency of `uas-sandbox` at the version `Cargo.lock` holds, 0.2.190 [verified: `grep -n 'name = "libc"' -A1 Cargo.lock`, line 2222, 2026-10-11]. The spec 0004 `libc` row moves off 0.2.169 and "NOT in the workspace", and plan 0043 R1 waives the 2026-09-25 cutoff for it. | F5. The crate builds BPF in Rust and adds no C dependency, which keeps spec 0004's "no system library" line. `seccompiler` keys its rules by raw syscall number and `rustix` exports none, so the numbers come from `libc::SYS_*` (plan 0043 finding 3). | integrating lane w25:p1, 2026-10-10; `seccompiler` approved and `libc` added per plan 0043 R1, ruled: Christian, lane w25:p1, 2026-10-11 about 00:05Z |
| D8 | The filter's cost is measured, not promised: the `escapes` suite prints the time from `fork` to `exec` with and without the filter. A spec may cite the number once measured. | F6. | integrating lane w25:p1, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0043-01]** (Event-driven): WHEN `uas-sandbox` launches a worker, it shall install the seccomp-BPF filter of D2 in the child after the network namespace hook and after the Landlock ruleset, as the last `pre_exec` step before `exec`.
- **[REQ-0043-02]** (Unwanted): IF a worker calls a syscall in the D2 deny list, THEN the kernel shall return `EPERM` and the worker shall keep running.
- **[REQ-0043-03]** (Unwanted): IF a worker calls `clone` with `CLONE_NEWUSER`, or `socket` with `AF_PACKET` or `SOCK_RAW`, THEN the kernel shall return `EPERM`; IF a worker calls `clone3`, THEN the kernel shall return `ENOSYS`.
- **[REQ-0043-04]** (Unwanted): IF a worker makes a syscall from an architecture other than the build target's, THEN the kernel shall kill the process.
- **[REQ-0043-05]** (Unwanted): IF the filter cannot be built or installed, THEN `uas-sandbox` shall refuse the launch with `JailRefused` and shall not `exec` the worker.
- **[REQ-0043-06]** (Ubiquitous): The `uas-sandbox` crate shall build the filter with `seccompiler` and shall link no `libseccomp`.
- **[REQ-0043-07]** (Ubiquitous): The escape suite shall hold one row per denied family of D6, each expecting the result D6 names for it.
- **[REQ-0043-08]** (Optional feature): WHERE `/proc/sys/kernel/seccomp/actions_avail` is absent or lacks `errno`, the escape suite shall mark the seccomp rows as skipped and print the skip.
- **[REQ-0043-09]** (Ubiquitous): The filter shall deny nothing beyond the D2 list; every other syscall shall pass to the kernel unchanged.

## Plan

- [x] Spec 0004: add the `seccompiler` 0.5.0 row to both tables and move the `libc` row to 0.2.190 for `uas-sandbox` (D7). Run `make pin-check`. Landed in PR #136.
- [x] `uas-sandbox`: `seccomp.rs` with the D2 program, `install()` for `pre_exec`, and the `JailRefused` mapping (D4). Landed in PR #139.
- [x] `uas-sandbox`: wire `install()` into `launcher::launch` after `restrict_child` (D1). Landed in PR #141.
- [x] Escape suite: the twelve D6 rows and the D8 timing print. Landed in PR #142.
- [x] Spec 0012: add the rows to its matrix and this spec to its `depends_on`. Eleven rows landed in PR #136; the `x32_syscall` row followed on 2026-10-11.

## Test

| Requirement | Negative Control |
|---|---|
| REQ-0043-02 | Row `ptrace_self` calls `ptrace(PTRACE_TRACEME)` and expects `EPERM`. Goes red if the filter is not installed, since an unfiltered unprivileged process may trace itself. |
| REQ-0043-03 | Rows `unshare_user` and `clone_newuser` expect `EPERM`; row `clone3_enosys` expects `ENOSYS`. Goes red without the filter: spec 0011 F1 shows `unshare -Urn` succeeds on ceres, and `clone3` succeeds on kernel 6.6. |
| REQ-0043-05 | A unit test hands `launch` a ruleset and a fake installer that fails; expects `JailRefused` and no child. Goes red if the launch proceeds. |
| REQ-0043-09 | Row `write_ws` and `exec_from_ws` of spec 0012 still pass. Goes red if the filter over-denies `openat` or `execve`. |

```bash
cargo test -p uas-sandbox -j 2 --test escapes
cargo nextest run -p uas-sandbox seccomp -j 2
```

Passing means exit 0 with every seccomp row at the result D6 names for it, or `skipped` with the skip printed.

## Notes

- `EPERM` is chosen over `ENOSYS` for every entry but `clone3`: a tool that sees `ENOSYS` falls back to an older call, which is what the list must not allow except where the older call is itself filtered. For `clone3` that fallback is the point (D2).
- The inherited filter of F2 stacks under this one. Stacking only narrows, so the host's filter cannot be loosened by the slot's.
- Out of scope: an allow-list mode, syscall argument inspection beyond D2, and logging of denials. Spec 0014's eBPF audit probes are the place for a denial counter if one is wanted.
- The research lane proposed two further sandbox items, a zero-copy memfd IPC and an L7 prompt firewall. Both are design drafts with no code and no consumer; they are parked, not specified.
- Status in-progress, ruled: integrating lane w25:p1, 2026-10-11. Landed on `development`: PR #136 (wave 1: the `seccompiler` and `libc` pins, the spec 0012 rows), PR #139 (wave 2: `seccomp.rs` with `build` and `install`) and PR #141 (wave 3: `launch` installs the filter last in `pre_exec`). PR #142 (wave 4) then added the D6 rows, `x32_syscall` among them, and the D8 timing print to `crates/uas-sandbox/tests/escapes.rs` [reported: `uas@20832e5`, its subject and the file's line 9].
- Amended 2026-10-11 (source: `git log origin/development` for `crates/uas-sandbox/src/seccomp.rs` and `tests/escapes.rs`, merges of PRs #136, #139, #141, #142): the Plan boxes are ticked for what those PRs delivered. D2 already records the two programs (`EPERM` list, `ENOSYS` for `clone3`) and the x32 kill; spec 0012 now carries the `x32_syscall` row.
