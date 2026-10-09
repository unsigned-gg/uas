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
- The CI runner (`ci-rust` pod, kernel `6.18.51-talos`) holds no
  capability (`CapBnd` 0), has `NoNewPrivs: 1` already set and a seccomp
  filter (`Seccomp: 2`). `unshare -n` and `unshare -Urn` both fail with
  `Operation not permitted`; `max_user_namespaces` is 15000. Measured in
  this spec's PR #27 by the gate's diagnostics. The runner cannot create a
  network namespace by either path.
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
| D2 | Two paths. First `unshare(CLONE_NEWNET)` plus the `lo` ioctl. If either is refused, `unshare(CLONE_NEWUSER \| CLONE_NEWNET)` with an identity map of the effective uid and gid, so the worker keeps its ids. | CAS lane, 2026-10-09 |
| D3 | `lo` is brought up with `SIOCGIFFLAGS` and `SIOCSIFFLAGS` on a UDP socket. No netlink crate and no `ip` subprocess. | CAS lane, 2026-10-09 |
| D4 | Nothing in the child allocates. Paths are `CStr` literals, the map lines are formatted into stack buffers, and every error is a raw errno. A failed namespace is reported as `ENONET`, which `exec` never produces; the parent sees `SandboxError::NetnsRefused`. | CAS lane, 2026-10-09 |
| D5 | Default deny comes from the namespace itself: no interface but `lo`, no route. No packet filter is installed. | CAS lane, 2026-10-09 |
| D6 | `rustix` enters the workspace with `default-features = false` and `std`, `thread`, `process`, `fs`. The approved row allows `mount` too; it is not enabled until a crate needs it. | CAS lane, 2026-10-09 |

Christian corrects any of these in place.

### Child sequence

1. `unshare(CLONE_NEWNET)`. On success, bring `lo` up (step 4). If that
   works, return.
2. On `EPERM` from step 1, or a failed step 4 after it: `unshare(CLONE_NEWUSER
   | CLONE_NEWNET)`, then write `deny` to `/proc/self/setgroups`, `<egid>
   <egid> 1` to `/proc/self/gid_map` and `<euid> <euid> 1` to
   `/proc/self/uid_map`. The kernel accepts a single-line map only for the
   writer's effective ids.
3. Any other error from step 1: return `ENONET`.
4. Open a UDP socket, read the `lo` flags, set `IFF_UP`, write them back.
5. Return. The launcher hook runs next and applies Landlock.

For a non-root worker: the user-namespace path leaves capabilities inside
its own namespace until `exec`, and `exec` with a non-zero uid drops them.
A root worker keeps its capabilities on either path; `no_new_privs` does
not clear them. Raw sockets opened in the namespace reach nothing, because
it has no interface but `lo`.

Ticket criterion not met as written: CER-2717 says raw sockets are blocked.
This spec does not block them. It makes them useless by leaving them no
interface. The escape suite (CER-2684, CER-2721) is the place to prove that.

### Errors

`SandboxError::NetnsRefused`: the child could not get a private namespace
or could not bring `lo` up. It did not run. The parent receives one errno,
`ENONET`, for every cause. The cause is lost. A caller that needs it runs
the sequence by hand outside `pre_exec`.

### Out of scope

The Unix-socket proxy to the LLM gateway (CER-2718). Any egress allow list.
The escape suite (CER-2684, CER-2721).

## Plan

- [x] Pin `rustix = "1.1.5"` in the workspace with the D6 features; note it in spec 0004.
- [x] `netns::isolate(command)` with the two-path sequence, `NETNS_REFUSED_ERRNO` and `SandboxError::NetnsRefused`.
- [x] Integration test `netns`: the jailed worker sees only `lo`, connects on `127.0.0.1`, and gets `ENETUNREACH` for `1.1.1.1:80`; the unjailed control sees another interface and connects on `127.0.0.1`.
- [x] CHANGELOG line and `specs/README.md` row.

## Test

```bash
cargo test -p cas-sandbox -j 2 --test netns
```

Passing means exit 0. The worker is the test binary, re-executed through
`netns::isolate` and `launch`. The ruleset adds read and execute on the
`target` directory and read on `/proc`. The parent reads three probe lines
from the worker's stdout. The jailed worker must see only `lo` in
`/proc/net/dev`; the unjailed control must see another interface. The
jailed case first probes the host with `netns::isolate` alone. A host that
cannot create a namespace prints `skipped` plus its diagnostics and the
case passes; the PR body then says which host ran it. Decision, Christian,
2026-10-09, by interview in the CAS lane: probe and skip, and file a ticket
for the `ci-rust` runner so CI proves the namespace later. Add
`-- --nocapture` to see the probe lines. Also run `make check`,
`make hygiene` and `make smoke`.

## Notes

Alternative for D3: a netlink message over `AF_NETLINK`. It needs the
`net` feature of `rustix` or a netlink crate; neither is on the approved
list. The two `ioctl` calls need neither.

Alternative for D2: always use a user namespace. Rejected. On a host with
`CAP_SYS_ADMIN`, the direct path gives a non-root worker no capability
after `exec`. That is the smaller surface.

References:

- `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md` section 3.3
- Linux `user_namespaces(7)`, `network_namespaces(7)`, `netdevice(7)`
- `rustix-1.1.5:src/thread/setns.rs`, `src/ioctl/patterns.rs`
