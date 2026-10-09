---
status: complete
created: 2026-10-09
tags: [sandbox, landlock, kernel, phase-2]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0004-dependency-and-licence-policy]
---

# 0010-landlock-ruleset-and-abi-probe

## Overview

`cas-sandbox` is a placeholder. Phase 2 starts here. This spec gives the crate
its first real code. CER-2715 adds the Landlock ABI probe and the ruleset
builder. CER-2716 adds the launcher. Parent: CER-2680.

Why now: spec 0001 invariant 4.4 says a worker writes only inside its
ephemeral worktree. Nothing enforces that today. The launcher (CER-2716), the
network namespace (CER-2717) and the escape tests (CER-2684, CER-2721) all
build on the ruleset this spec defines.

Facts, measured 2026-10-09 on ceres:

- Kernel `6.6.87.2-microsoft-standard-WSL2` (`uname -r`). Landlock is built
  in and first in the LSM order: `zcat /proc/config.gz | grep -E
  'CONFIG_SECURITY_LANDLOCK|CONFIG_LSM='` prints `CONFIG_SECURITY_LANDLOCK=y`
  and `CONFIG_LSM="landlock,lockdown,..."`.
- The probe test reports `effective_abi=3` on that kernel. The ABI to kernel
  map comes from `landlock-0.4.7:src/compat.rs`: v3 is Linux 6.2, v4 is 6.7,
  v5 is 6.10.
- The `landlock` crate 0.4.7 (MIT OR Apache-2.0, MSRV 1.71) knows ABI v1 to
  v9. It has no public ABI probe: `LandlockStatus::current()` is private and
  the status is returned only by `restrict_self()`.
- Christian approved the spec 0004 epic dependency list on 2026-10-09 by
  interview in the CAS lane. `landlock` 0.4.7 is the first row to enter a
  `Cargo.toml`.
- The CI runner kernel is not measured. The probe test prints what it finds.

## Design

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | Use the `landlock` crate, not raw syscalls. The crate carries the `unsafe`; `cas-sandbox` stays free of it in this spec. | CAS lane, 2026-10-09 |
| D2 | The ABI probe applies a minimal ruleset on a scratch thread and reads `RestrictionStatus::landlock`. Landlock restricts the calling thread only, so the process is not confined. | CAS lane, 2026-10-09 |
| D3 | The workspace root is a parameter. `/workspace` is the pooled-runtime default, not a constant in the crate. | CAS lane, 2026-10-09 |
| D4 | Every path is canonicalised with `std::fs::canonicalize` before it enters a rule. A path that does not exist is an error, not a silent skip. | CAS lane, 2026-10-09 |
| D5 | ABI v1 accesses are a hard requirement. `build()` fails on a kernel that cannot enforce them. Newer accesses (refer, truncate, ioctl) are best effort. | CAS lane, 2026-10-09 |
| D6 | Default deny. The builder lists what a worker may touch. Everything else is denied for every handled access. | CAS lane, 2026-10-09 |
| D7 | Scratch and device paths get write without execute. Only the workspace and the system paths may execute. | CAS lane, 2026-10-09, after a critic pass |
| D8 | The child enters the jail in `pre_exec`, between fork and exec. The parent never restricts itself. A refused jail is reported as `ENOTRECOVERABLE`, because std forwards only a raw errno from `pre_exec`. | CAS lane, 2026-10-09 |

Christian corrects any of these in place.

### Probe

`abi::probe()` returns `AbiReport`:

| Field | Meaning |
|---|---|
| `status` | `Unsupported`, `NotEnabled` or `Available`. |
| `effective_abi` | The highest ABI both kernel and crate understand, as a number. 0 when not available. |
| `kernel_abi` | The raw kernel ABI when it is newer than the crate knows. |

The probe runs on its own thread and joins it. The calling thread is never
restricted. A test writes a file after the probe to prove it.

### Ruleset builder

`LandlockRulesetBuilder` collects path rules and produces a
`landlock::RulesetCreated` for the launcher to apply.

| Method | Access granted beneath the path |
|---|---|
| `allow_read(path)` | read file, read dir |
| `allow_read_exec(path)` | read plus execute |
| `allow_write(path)` | every access except execute |
| `allow_read_write(path)` | every filesystem access the effective ABI offers |
| `allow_optional(path, access)` | the rule only if the path exists now; the plan records a skip |
| `plan()` | canonicalises and returns the `RulesetPlan` |
| `build()` | `plan()`, then opens each path and adds the rules; returns the ruleset and the plan |

`RulesetPlan` lists each rule as (canonical path, access class) plus the
optional paths that were skipped. Tests and logs read it. It has no I/O of
its own.

### Worker preset

`LandlockRulesetBuilder::worker(workspace, toolchain)` returns a builder with:

- every access beneath the workspace root;
- read and execute beneath `/usr`, `/lib`, `/bin` and each toolchain path;
- read beneath `/etc`, which holds `ld.so.cache`, `resolv.conf`, the CA
  certificates and `passwd`;
- write without execute beneath `/dev/null`, `/dev/zero`, `/dev/urandom` and
  `/tmp`.

`/lib64` and `/sbin` are added only when they exist, because they are
distribution-specific. `/home`, `/root` and the rest of `/etc` get no write
rule. There is no rule for `/proc`; CER-2716 decides what the launcher needs
there.

Toolchain paths are the caller's choice. Residual risk: `~/.cargo` holds
`credentials.toml` on a workstation. Pass `~/.cargo/bin`, `~/.cargo/registry`
and `~/.rustup`, never `~/.cargo` whole. The pool (phase 3) owns that list.

A symlink cannot widen a rule. Landlock binds a rule to the inode it opened,
so a link under `/tmp` that points at `/home` keeps the `/home` verdict. A
test proves it.

### Errors

`SandboxError` is a `thiserror` enum: `PathMissing(PathBuf)`,
`PathOpen(PathBuf, io::Error)`, `Ruleset(landlock::RulesetError)` and
`ProbeThread(String)`. On a kernel without Landlock, `build()` returns
`Ruleset` because of D5. `JailRefused` means the child could not enter the
jail and did not run. `Spawn` wraps any other `spawn` failure.

Correction, CAS lane, 2026-10-09: an earlier revision said the launcher must
treat `PartiallyEnforced` as a failed jail. That was wrong. D5 makes newer
accesses best effort, so a kernel at ABI v3 reports `PartiallyEnforced` for
a ruleset that handles v5 accesses. Only `NotEnforced` fails the jail.

### Launcher

`launcher::jail(command, ruleset)` installs a `pre_exec` hook on a
`std::process::Command`. `launcher::launch(ruleset, command)` installs it and
spawns. In the child, before `exec`:

1. `restrict_self` sets `PR_SET_NO_NEW_PRIVS` and calls
   `landlock_restrict_self`. The crate does both; the launcher calls neither
   syscall itself.
2. The child refuses to continue when `no_new_privs` is false or the ruleset
   status is `NotEnforced`. It returns `ENOTRECOVERABLE`, which `exec` never
   produces, and the parent's `spawn` fails with `SandboxError::JailRefused`.
   The worker never runs. A kernel errno from the syscalls passes through.
3. Nothing in the child allocates: every error is built from a raw errno.
   The parent process may have other threads, and an allocation after fork
   can deadlock on the allocator lock.

The ruleset stays in the parent's closure. Each child takes its own copy
after fork, so one jailed command spawns any number of jailed workers.
`pre_exec` is the one `unsafe` block in the crate; it carries a `SAFETY`
comment (AGENTS.md rule 14).

A kernel errno reaches the parent unchanged in `Spawn`. The test switches
`no_new_privs` off. Measured 2026-10-09: on ceres (unprivileged) the kernel
refuses `landlock_restrict_self` with `EPERM` and the parent sees that
errno; on the CI runner the process already has `no_new_privs` set, so the
kernel accepts the call and the child reaches the launcher's own
`JailRefused` verdict. Both paths leave the worker unrun.

Correction, CAS lane, 2026-10-09: an earlier revision said the CI runner
holds `CAP_SYS_ADMIN`. It holds no capability at all (`CapBnd` 0, measured
in PR #27). The verdict path was reached through `NoNewPrivs: 1`, not a
capability.

### Out of scope

Network namespaces and the escape suite belong to CER-2717, CER-2684 and
CER-2721.

## Plan

- [x] Record the dependency ruling in spec 0004 and pin `landlock = "0.4.7"` in the workspace.
- [x] `abi::probe()` and `AbiReport`.
- [x] `canon::canonical()` with the missing-path error.
- [x] `LandlockRulesetBuilder`, `RulesetPlan` and the `worker` preset.
- [x] Integration test `landlock_probe`: the probe reports, the caller stays unrestricted, the builder canonicalises a symlink, a missing path fails, and a jailed thread writes inside the workspace but not under `$HOME`, not through a `/tmp` symlink.
- [x] CHANGELOG line and `specs/README.md` row.
- [x] `launcher::jail` and `launcher::launch` with the `JailRefused` and `Spawn` errors (CER-2716).
- [x] Integration test `launcher`: a jailed `sh` writes inside the workspace only and reports `NoNewPrivs: 1`. A jailed command jails every child. A refused jail never runs the worker.

## Test

```bash
cargo test -p cas-sandbox -j 2 --test landlock_probe
cargo test -p cas-sandbox -j 2 --test launcher
```

Passing means exit 0 for both. Add `-- --nocapture` to see the probe line
`landlock probe: status=Available effective_abi=N kernel_abi=None`, N at
least 1. On a kernel with no Landlock the jail and launcher cases print
`skipped` and pass; the PR body then states which kernel ran the gate. Also run
`make check`, `make hygiene` and `make smoke`.

## Notes

Alternative for the probe: call `landlock_create_ruleset(NULL, 0,
LANDLOCK_CREATE_RULESET_VERSION)` directly. That needs `libc` as a direct
dependency and an `unsafe` block. `libc` is not in the spec 0004 list, so D2
avoids both. Revisit if the scratch thread proves a problem.

The ticket says "ABI v1 to v5". The crate knows v1 to v9, so the probe reports
whatever the kernel offers. Nothing in this spec needs more than v1.

Not tested: a write to `/etc` or `/usr` from the jail. A non-root user is
denied there by file permissions as well, so the test could not tell Landlock
from DAC. The `$HOME` case is the one that only Landlock denies.

References:

- `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md` section 3.3
- `landlock-0.4.7:src/compat.rs` (`ABI`, `LandlockStatus`, private `current()`)
- Linux `Documentation/userspace-api/landlock.rst`
