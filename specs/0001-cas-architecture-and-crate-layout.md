---
status: in-progress
created: 2026-10-08
tags: [architecture, workspace, layering, substrate]
priority: high
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0001-cas-architecture-and-crate-layout

## Overview

UAS is the Unsigned Agent Substrate. It is a Rust-native, pooled runtime for
agent harnesses. It runs agent sessions as durable workflows. It confines each
worker with kernel features. It routes model calls by prompt prefix. It
reports to the operator through a dashboard.

Why now: the research lane landed the UAS design on 2026-10-07 in
`cerebral-work/research` (PRs #12 and #13, head `4068134`). The design is a
draft. Its owner is lane w19:p1 and `approved_by` is empty. It never names a
repository. This repo is that repository.

This repo was made from the estate Rust monorepo template on 2026-10-08. The
scaffold carries the gates in specs 0002 to 0005. This spec fixes what is UAS
specific: the crates, the layering rule, the invariants and the open gaps.

Relation to `unsigned-gg/substrate`: GitHub reports its language as Go and its
description as "Agent Substrate: the core system" (read 2026-10-08). It is a
different system with a similar name. UAS does not import it, call it, or
replace it.

Status on 2026-10-08: scaffold and domain only. No runtime exists yet. The
eight non-domain crates are placeholders with a README and no code.

## Design

### Requirements (EARS)

- **Ubiquitous:** The workspace crates shall strictly follow the downward layering order (`deck > acp > engine > pool > sandbox, ebpf > router > domain`).
- **Ubiquitous:** The `uas-domain` crate shall contain pure types and zero direct I/O dependencies.
- **Event-driven:** WHEN a crate requires estate contracts, the workspace shall vendor schema definitions under `vendor/` without compiling them as members.
- **State-driven:** WHILE a worker runs builds on shared hosts, the substrate shall clamp build process trees to `nice 19`.
- **Unwanted behavior:** IF an unauthorized file access occurs in a worker sandbox, THEN the kernel shall deny the call and trigger saga compensation.
- **Optional feature:** WHERE DeltaDB virtual worktrees are enabled, the substrate shall reconcile changesets through pull requests targeting `development`.

### Crates

The workspace has ten crates. Phases come from the research roadmap
(`cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md`
section 6).

| Crate | Role | Phase |
|---|---|---|
| `uas-domain` | Slots, leases, envelopes, attestations. Pure types. Zero I/O. | 1 |
| `uas-engine` | Autumn-Harvest workflows, activities and saga compensators. | 1 |
| `uas-sandbox` | Landlock rulesets, namespace isolation, path canonicalisation. | 2 |
| `uas-ebpf` | Aya eBPF tracepoint programs: priority clamping, audit telemetry. | 2 |
| `uas-pool` | Worker slot manager, warm worktrees, cgroup v2 suspend and resume. | 3 |
| `uas-acp` | Agent Communication Protocol server (JSON-RPC 2.0) for IDEs. | 3 |
| `uas-router` | Prefix-hash routing and load balancing for model calls. | 4 |
| `uas-deck` | Operator dashboard: Axum, Maud, htmx, Tailwind. | 5 |
| `uas-storage` | Clients for rustfs, minikv and aistore. | 6 |
| `uas-cli` | Binary `uas`. Kept from the template, see Decisions. | n/a |

`xtask/` also exists. It holds `spec-check` and `toolchain-check`. It is not a
UAS crate and sits outside the layering.

### Layering

Source: `cerebral-work/research@4068134:agentic-systems/designs/substrate-diagrams-and-components.md`
section 3. From the top layer to the bottom:

```text
deck > acp > engine > pool > sandbox, ebpf > router > domain
```

The rule: a crate depends only on crates in lower layers. `uas-domain` depends
on no other crate in the workspace and has zero I/O. It uses no sockets, no
files, no processes, no database driver and no async runtime. `make smoke`
greps for `std::net`, `std::fs`, `std::process` and `tokio` in its source.

Two placements are not in the diagram:

- `uas-storage` is not drawn in the research diagram. This spec places it
  beside `uas-router`, above `uas-domain`. Spec 0021, itself `planned`, records
  that placement on 2026-10-10: its F7 restates the layering and its first requirement states
  that `uas-storage` sits in layer 2 beside `uas-router`, depending only on
  `uas-domain` and dependencies approved in spec 0004.
- `uas-cli` is the application edge. It may depend on any crate.

The `Will depend on` line in each crate README is the intended edge set. It is
not a Cargo dependency. Add a dependency when the first real code needs it.

### Invariants

Source: `cerebral-work/research@4068134:SYNTHESIS-ESTATE-AND-SUBSTRATE.md`
sections 4.1 to 4.7. The research states them as design goals. This spec
adopts them as the targets to verify. None is verified yet.

| # | Invariant | Crate |
|---|---|---|
| 4.1 | The domain is pure and compiles synchronously. Adapters use traits and typed envelopes. | domain |
| 4.2 | Shared hosts run builds with two parallel jobs. The research wants the kernel to enforce it. | ebpf |
| 4.3 | Recovery replays the event log. A completed turn never calls the model again. | engine |
| 4.4 | A worker writes only inside its ephemeral worktree. `PR_SET_NO_NEW_PRIVS` is set. | sandbox |
| 4.5 | Turns with the same prefix hash go to the same model replica. | router |
| 4.6 | Irreversible actions wait for an operator signal. An agent never approves its own promotion. | engine, deck |
| 4.7 | Git operations run in-process with gitoxide. No `git` subprocess in a worker. | pool |

### Failure matrix

Source: `cerebral-work/research@4068134:agentic-systems/designs/substrate-diagrams-and-components.md`
section 11. Four failures, each with a planned response:

| Failure | Planned response | Crate |
|---|---|---|
| Host reboot or worker crash | Replay the event log. No token re-spend. | engine |
| Compilation failure in a worker | Restore the last checkpoint tree in memory. | pool |
| Unauthorised file access | The kernel denies the call. A compensation runs. | sandbox, engine |
| Host CPU saturation | Clamp the build process tree to `nice 19`. | ebpf |

The research claims a tree restore under 5 ms and a cache hit rate over 90
percent (SYN sections 4.5 and 4.7, ATL section 11). These are design claims.
Nobody has measured them here.

### Integration edges

Source: `cerebral-work/research@4068134:SYNTHESIS-ESTATE-AND-SUBSTRATE.md`
section 3.

1. A task arrives. The soma reflex gateway classifies its policy tier.
2. Swarm-control allocates a slot lease under its capacity gate.
3. The lease reaches `uas-engine`, which runs the session as a durable workflow.
4. The pool runs the harness in a jail. The router serves its model calls.
5. Execution receipts go to saga. An exit code of 0 earns the `verified` tier.
6. Saga compensates on failure. Results go to reverie as memory and to the
   cortex mesh as signed messages.
7. `uas-deck` connects to the cortex settle queue at `/v1/settle/pending`
   (DES section 3.6). The operator signs off there.

UAS integrates with soma, cortex, saga and reverie through service edges. It
vendors their contract files and never their service code:

- A contract file is a schema, a protocol definition, a flow template or a
  contract type source. It lives under `vendor/<name>/`.
- Each `vendor/<name>/README.md` names the source repository and the upstream
  commit. Refresh a copy only by replacing it from a newer commit.
- Nothing under `vendor/` is a workspace member or compiled into a crate. A
  crate that needs a contract type defines it, or reaches it through a
  service edge.
- Spec 0004 lists the copies and their licences.

### Decisions of record

| Decision | Who | When |
|---|---|---|
| Repo is `unsigned-gg/uas`. Private. Proprietary licence. | the operator | 2026-10-08 |
| First landing is the scaffold plus `uas-domain`. The other eight research crates start as placeholders. | the operator | 2026-10-08 |
| `uas-cli` stays beside the nine research crates. The smoke gate and the release tag use it. | the operator | 2026-10-08 |
| Crate prefix is `uas`. | the operator | 2026-10-08 |
| GPL dependencies are denied by `deny.toml`. | the operator | 2026-10-08 |
| UAS vendors estate contract files, never service code ("contracts yes, code no"). | the operator | 2026-10-09 |
| Phase 8 (DeltaDB) enters the epic saga run. Recorded as adoption into the plan, see Notes. | the operator | 2026-10-09 |
| `unsafe` stays forbidden in the workspace. `uas-sandbox`, `uas-pool` and `uas-ebpf` deny it instead, with a per-block allow and a SAFETY comment (AGENTS.md rule 14). | the operator | 2026-10-09 |
| The kernel-side BPF program object declares `Dual MIT/GPL`. The loader and every other crate stay proprietary. | the operator | 2026-10-09 |
| eBPF tests on ceres run through a sudo `make ebpf-test` target. The sysctl `kernel.unprivileged_bpf_disabled` is not changed. | the operator | 2026-10-09 |

The research names nine crates. This repo has ten because of `uas-cli`.

### Open gaps

These are not decided. Each needs a spec or an operator ruling before code.

1. **Pool runtime.** Closed by spec 0019 D2 (2026-10-10): the operator ruled
   Option A, OS threads with `std::sync` as built, on the benchmark addendum
   numbers [reported, relayed by the integrator, mesh e42efbc2]. The heph design and the
   options are in spec 0019 and `cerebral-work/research@4068134:repos/heph.md`
   line 92. `uas-domain` stays runtime-neutral.
2. **Kernel `-j 2`.** Closed by spec 0020 (2026-10-10). The research claim that
   eBPF enforces two parallel jobs is dropped: `nice 19` is a priority, not a
   cap. The cap is the cgroup v2 CPU quota per slot (`cpu.max`, two CPUs by
   the operator's ruling) that `uas-pool` writes, plus the `-j 2` convention.
3. **`uas-storage` has no phase.** Resolved by spec 0021 (2026-10-10): `uas-storage`
   is assigned to Phase 6 (Distributed Storage & Operator Client) alongside
   the AIStore Kubernetes operator deployment.
4. **aistore is Go.** Resolved by spec 0021 (2026-10-10): `uas-storage` provides
   Rust client adapters over network protocols (S3-compatible REST API for rustfs
   and AIStore, HTTP/REST for minikv) and does not link Go internals.
5. **eBPF licence.** Decided 2026-10-09, see Decisions of record and spec 0004:
   the kernel-side BPF program object declares `Dual MIT/GPL`. Every other
   crate, the userspace loader included, stays proprietary. (An eBPF program
   that calls GPL-only kernel helpers must declare a GPL-compatible licence;
   Linux kernel `Documentation/bpf/bpf_licensing.rst`.)
6. **eBPF toolchain and access.** The eBPF side needs nightly Rust and
   `bpf-linker`. The `ci-rust` image has neither (spec 0005). On ceres,
   tracepoint programs need CAP_BPF and CAP_PERFMON, whatever
   `kernel.unprivileged_bpf_disabled` says. Decided 2026-10-09: a
   `make ebpf-test` target runs only the eBPF test binary under sudo, and the
   operator types the password. CI still needs a privileged runner.
7. **The research disagrees about the crate list.** By count on 2026-10-08:
   the synthesis table names 6 UAS crates, the diagram atlas draws 8, and the
   design names 9. The synthesis omits `uas-acp`, `uas-storage` and
   `uas-domain`. The atlas omits `uas-storage`. This spec follows the design
   (9) plus `uas-cli`.
8. **The research is a draft.** Its status is `draft` and `approved_by` is
   empty. Claims in it are design intent until measured.

## Plan

- [x] Scaffold from the estate template. Ten crates compile.
- [x] Build `uas-domain` (slot, lease, envelope, attestation, error). PR #3.
- [x] Write the spec that chooses the pool runtime: spec 0019, ruled Option A on 2026-10-10 (D2).
- [x] Write the spec that designs a job cap for builds, or drops the claim: spec 0020.
- [x] Give `uas-storage` a phase or merge it into another crate (spec 0021).
- [x] Decide the eBPF licence position and record it in spec 0004 (2026-10-09).
- [ ] Add each crate dependency only when its first real code lands.

## Test

Check that the specs are valid, that every crate builds, and that the domain
crate has no I/O.

```bash
make spec-check && make build && make smoke
```

Passing means exit 0. A new `std::fs` use in `crates/uas-domain/src` makes
`make smoke` fail. A crate that depends upward makes review fail. No tool
checks the layering yet. Moon `dependsOn` is the planned enforcement.

## Notes

References, as `repo@sha:path`:

- `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md`
  (design, sections 3, 4 and 6)
- `cerebral-work/research@4068134:agentic-systems/designs/substrate-diagrams-and-components.md`
  (layers in section 3, failures in section 11)
- `cerebral-work/research@4068134:SYNTHESIS-ESTATE-AND-SUBSTRATE.md`
  (edges in section 3, invariants in section 4)
- `cerebral-work/research@4068134:repos/gitoxide.md`, `autumn-harvest.md`,
  `heph.md`, `madstack.md`, `neutral-engineering.md`, `zerostack.md`

Alternatives rejected: dropping `uas-cli` to match the research list. The smoke
gate and the release tag need a binary.

DeltaDB virtual-worktree integration with Saga, Blackwall and Soma
(`cerebral-work/research@ddf42c8:agentic-systems/designs/deltadb-virtual-worktree-integration.md`,
draft, no approver):

- 2026-10-08: the operator ruled to record it here only, as proposed, not adopted.
- 2026-10-08: the cortex lane sent six review findings to the proposing lane
  (w1R:p1). One finding blocks: the design commits directly to main.
- 2026-10-09: the operator included Phase 8 (DeltaDB) in the epic saga run, by
  interview in the UAS lane. The UAS lane records this as adoption into the
  plan. the operator corrects it here if that reading is wrong.
- The blocking finding still stands. No Phase 8 code runs until a revised
  design lands that never commits directly to main. Phase 8 runs last.
- Estate-wide DeltaDB and Blackwall custody process adopted with PR-floor constraints
  across estate Rust repositories (CER-2671, operator ruling 2026-10-08). Adopts the
  integration design with PR-only landing: changesets reconcile to feature branches,
  open PRs targeting `development`, and merge on green with merge commits. Direct push
  to `main` remains forbidden.
