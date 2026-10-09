---
status: complete
created: 2026-10-09
tags: [sandbox, netns, kernel, phase-2]
priority: high
depends_on: [0010-landlock-ruleset-and-abi-probe, 0004-dependency-and-licence-policy]
---

# 0011-network-namespace-isolation

## Overview

A worker in the Landlock jail (spec 0010) can still open any network
connection. This spec gives each worker a private network namespace with
only a loopback interface, so outbound networking is denied by default
(CER-2717, parent CER-2681). The gateway proxy over a Unix socket is
CER-2718 and builds on this.

Facts, measured 2026-10-09:

- ceres (kernel `6.6.87.2-microsoft-standard-WSL2`, unprivileged user,
  `CapEff` 0): `unshare -n true` fails with `Operation not permitted`;
  `unshare -Urn` (user namespace plus network namespace) succeeds, and
  `ip link set lo up` works inside it. `/proc/sys/user/max_user_namespaces`
  is 224913.
- The CI runner holds `CAP_SYS_ADMIN` without being root (spec 0010, the
  launcher refusal test on PR #26). There `unshare(CLONE_NEWNET)` alone is
  expected to work. The first CI run of this spec's PR measures it.
- `rustix` 1.1.5 is on the approved spec 0004 list with features `std`,
  `process`, `thread`, `fs`, `mount`. Its `ioctl` module is not feature
  gated. `unshare` is deprecated in favour of `unshare_unsafe`.
- A fresh network namespace has `lo` down and no routes. After `lo` is up
  the kernel assigns `127.0.0.1/8`. A connect to any non-loopback address
  fails with `ENETUNREACH` at once.

## Design

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | The namespace is entered in the child, in a `pre_exec` hook installed by `netns::isolate(command)`. It is installed before `launcher::jail`, so std runs it first and Landlock is applied last. | CAS lane, 2026-10-09 |
| D2 | Two paths. First `unshare(CLONE_NEWNET)`. On `EPERM`, `unshare(CLONE_NEWUSER \| CLONE_NEWNET)` with an identity uid and gid map, so the worker keeps its uid and the host sees no new identity. | CAS lane, 2026-10-09 |
| D3 | `lo` is brought up with `SIOCGIFFLAGS` and `SIOCSIFFLAGS` on a UDP socket. No netlink crate and no `ip` subprocess. | CAS lane, 2026-10-09 |
| D4 | Nothing in the child allocates. Paths are `CStr` literals, the map lines are formatted into stack buffers, and every error is a raw errno. A failed namespace is reported as `ENONET`, which `exec` never produces; the parent sees `SandboxError::NetnsRefused`. | CAS lane, 2026-10-09 |
| D5 | Default deny comes from the namespace itself: no interface but `lo`, no route. No packet filter is installed. | CAS lane, 2026-10-09 |
| D6 | `rustix` enters the workspace with `default-features = false` and `std`, `thread`, `process`, `fs`. The approved row allows `mount` too; it is not enabled until a crate needs it. | CAS lane, 2026-10-09 |

Christian corrects any of these in place.

### Child sequence

1. `unshare(CLONE_NEWNET)`. On success go to 4.
2. On `EPERM`: `unshare(CLONE_NEWUSER | CLONE_NEWNET)`, then write
   `deny` to `/proc/self/setgroups`, `<gid> <gid> 1` to `/proc/self/gid_map`
   and `<uid> <uid> 1` to `/proc/self/uid_map`.
3. Any other error: return `ENONET`.
4. Open a UDP socket, read the `lo` flags, set `IFF_UP`, write them back.
5. Return. The launcher hook runs next and applies Landlock.

The user-namespace path leaves the worker with capabilities inside its own
namespace until `exec`; after `exec` with a non-zero uid they are dropped.
Raw sockets opened in that namespace reach nothing, because the namespace
has no interface but `lo`. This spec does not claim raw sockets are blocked.

### Errors

`SandboxError::NetnsRefused`: the child could not get a private namespace
or could not bring `lo` up. It did not run. A kernel errno other than the
expected `EPERM` is folded into this, because the parent can only receive
one errno and the cause is visible in the spec's sequence.

### Out of scope

The Unix-socket proxy to the LLM gateway (CER-2718). Any egress allow list.
The escape suite (CER-2684, CER-2721).

## Plan

- [x] Pin `rustix = "1.1.5"` in the workspace with the D6 features; note it in spec 0004.
- [x] `netns::isolate(command)` with the two-path sequence, `NETNS_REFUSED_ERRNO` and `SandboxError::NetnsRefused`.
- [x] Integration test `netns`: a jailed child binds and connects on `127.0.0.1`, a connect to `1.1.1.1:80` fails with `ENETUNREACH`, the unjailed control binds on `127.0.0.1` and reports its own WAN result.
- [x] CHANGELOG line and `specs/README.md` row.

## Test

```bash
cargo test -p cas-sandbox -j 2 --test netns
```

Passing means exit 0. The test re-executes its own binary as the worker
under `netns::isolate` plus `launcher::jail`, with a read-and-execute rule
for the `target` directory, and reads the probe lines from its stdout. On a
kernel that cannot create the namespace the child reports `NetnsRefused`
and the test fails; there is no skip, because both measured hosts can.
Add `-- --nocapture` to see the probe lines. Also run `make check`,
`make hygiene` and `make smoke`.

## Notes

Alternative for D3: a netlink message over `AF_NETLINK`. It needs the
`net` feature of `rustix` or a netlink crate; neither is on the approved
list. The two `ioctl` calls need neither.

Alternative for D2: always use a user namespace. Rejected: on a host with
`CAP_SYS_ADMIN` the direct path leaves the worker without any capability
after `exec`, which is the smaller surface.

References:

- `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md` section 3.3
- Linux `user_namespaces(7)`, `network_namespaces(7)`, `netdevice(7)`
- `rustix-1.1.5:src/thread/setns.rs`, `src/ioctl/patterns.rs`
