---
status: complete
created: 2026-10-09
tags: [sandbox, security, test, phase-2]
priority: high
depends_on: [0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation]
---

# 0012-sandbox-escape-suite

## Overview

Specs 0010 and 0011 each prove one property with one test. This spec is the
adversarial suite: one jailed worker runs every escape the tickets name and
the parent asserts each outcome (CER-2684, child CER-2721). It is the
evidence that the three mechanisms compose: the Landlock ruleset, the
launcher's `no_new_privs`, and the network namespace.

Why now: CER-2717 left a criterion open ("blocks raw sockets"). This suite
measures it instead of arguing it.

Facts, measured 2026-10-09 on ceres (kernel `6.6.87.2-microsoft-standard-WSL2`,
unprivileged): `/usr/bin/sudo` is setuid root; `/usr/bin/ping` is present
and not setuid; `net.ipv4.ping_group_range` is `1 0` (ICMP datagram sockets
are closed to every group, so `ping` needs a raw socket).

## Design

### Shape

The worker is the test binary, re-executed with `helper_attacks` as the
only test, under `launch` with the spec 0010 worker preset plus read and
execute on `target/` and read on `/proc`. The matrix runs twice: once with
Landlock and the launcher alone, and once with the network namespace added
when the host can create one (`netns::isolate` on `/bin/true` succeeds).
Without a namespace the network rows are reported as `skipped`, as spec
0011 rules. Every attack prints `check=<name> result=<value>`; the parent
asserts the matrix.

### Matrix

| Check | Attack | Expected | Mechanism |
|---|---|---|---|
| `write_ws` | write inside the workspace | `ok` | positive control |
| `write_home` | write under `$HOME` | `EACCES` | Landlock |
| `write_etc` | create `/etc/cas-escape-test` | `EACCES` | Landlock, also DAC |
| `write_root` | create `/cas-escape-test` | `EACCES` | Landlock, also DAC |
| `read_outside` | read a file under `$HOME` | `EACCES` | Landlock |
| `write_via_symlink` | symlink in the workspace to `$HOME`, write through it | `EACCES` | Landlock binds the inode |
| `rename_out` | rename a workspace file into `$HOME` | `EACCES` or `EXDEV` | Landlock refer |
| `hardlink_out` | hard-link a workspace file into `$HOME` | `EACCES` or `EXDEV` | Landlock refer |
| `exec_from_tmp` | copy `/usr/bin/true` to `/tmp`, execute it | `EACCES` | spec 0010 D7, write without execute |
| `nnp` | `NoNewPrivs` in `/proc/self/status` | `1` | launcher |
| `grandchild_nnp` | the same from a child `sh` | `1` | inherited across exec |
| `grandchild_write_home` | a child `sh` writes under `$HOME` | non-zero exit, no file | Landlock inherited across exec |
| `setuid_sudo` | `sudo -n true` | `The "no new privileges" flag is set` (sudo 1.9) or `effective uid is not 0`; with a user namespace, `owned by uid 65534` | `no_new_privs` ignores the setuid bit; in the user namespace root is unmapped |
| `tcp_wan` | TCP connect to `1.1.1.1:80` | `ENETUNREACH` | namespace, no route |
| `udp_wan` | UDP send to `1.1.1.1:53` | `ENETUNREACH` | namespace, no route |
| `ping_raw` | `ping -c1 -W1 1.1.1.1` | non-zero exit | `no_new_privs` ignores file capabilities, so the raw socket cannot open; with no route it would fail anyway |
| `interfaces` | `/proc/net/dev` | `lo` | namespace |

A host without `sudo` or `ping` reports `absent` for that row and the
parent accepts it. The parent also checks that no file appeared outside the
workspace and that the moved file is still inside it.

### Known gaps

- `write_etc` and `write_root` are denied by file permissions as well as by
  Landlock for a non-root worker. Those two rows cannot tell the mechanisms
  apart. The `$HOME` rows can, and they are the ones that matter.
- A Unix socket outside the workspace can still be connected to. Landlock
  scopes Unix sockets from ABI v6 (Linux 6.12); ceres is at v3. Not in the
  matrix. CER-2718 decides the socket policy.
- Measured 2026-10-09 on ceres: `ping_raw` reports `exit2 socktype:
  SOCK_RAW`. The raw socket could not be opened: `no_new_privs` makes the
  kernel ignore `ping`'s file capabilities, and ICMP datagram sockets are
  closed by `ping_group_range`. That closes CER-2717's open criterion for a
  worker that gains no capability at `exec`. A worker that already holds
  `CAP_NET_RAW` could open one and would still have no route.
- The network rows do not run on `ci-rust` until OPS-1660 lands.

## Plan

- [x] Integration test `escapes` with the matrix above and the parent-side checks.
- [x] CHANGELOG line and `specs/README.md` row.

## Test

```bash
cargo test -p cas-sandbox -j 2 --test escapes
```

Passing means exit 0. Add `-- --nocapture` to see one line per row. On a
host without Landlock the suite prints `skipped` and passes; on a host
without namespaces only the network rows are skipped. The PR body says
which host ran it. Also run `make check`, `make hygiene` and `make smoke`.

## Notes

The suite deliberately runs every attack in one worker. A per-attack worker
would be cleaner to read but would take seconds per row and hide
interactions between rows, such as the staged `/tmp` binary.

References:

- `specs/0010-landlock-ruleset-and-abi-probe.md`, `specs/0011-network-namespace-isolation.md`
- Linux `landlock(7)`, `prctl(2)` (`PR_SET_NO_NEW_PRIVS`), `user_namespaces(7)`
