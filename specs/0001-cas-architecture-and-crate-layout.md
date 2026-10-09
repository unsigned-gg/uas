---
status: in-progress
created: 2026-10-08
tags: [architecture, workspace, layering, substrate]
priority: high
---

# 0001-cas-architecture-and-crate-layout

## Overview

CAS is the Cerebral Agent Substrate. It is a Rust-native, pooled runtime for
agent harnesses. It runs agent sessions as durable workflows. It confines each
worker with kernel features. It routes model calls by prompt prefix. It
reports to the operator through a dashboard.

Why now: the research lane landed the CAS design on 2026-10-07 in
`cerebral-work/research` (PRs #12 and #13, head `4068134`). The design is a
draft. Its owner is lane w19:p1 and `approved_by` is empty. It never names a
repository. This repo is that repository.

This repo was made from the estate Rust monorepo template on 2026-10-08. The
scaffold carries the gates in specs 0002 to 0005. This spec fixes what is CAS
specific: the crates, the layering rule, the invariants and the open gaps.

Relation to `unsigned-gg/substrate`: GitHub reports its language as Go and its
description as "Agent Substrate: the core system" (read 2026-10-08). It is a
different system with a similar name. CAS does not import it, call it, or
replace it.

Status on 2026-10-08: scaffold and domain only. No runtime exists yet. The
eight non-domain crates are placeholders with a README and no code.

## Design

### Crates

The workspace has ten crates. Phases come from the research roadmap
(`cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md`
section 6).

| Crate | Role | Phase |
|---|---|---|
| `cas-domain` | Slots, leases, envelopes, attestations. Pure types. Zero I/O. | 1 |
| `cas-engine` | Autumn-Harvest workflows, activities and saga compensators. | 1 |
| `cas-sandbox` | Landlock rulesets, namespace isolation, path canonicalisation. | 2 |
| `cas-ebpf` | Aya eBPF tracepoint programs: priority clamping, audit telemetry. | 2 |
| `cas-pool` | Worker slot manager, warm worktrees, cgroup v2 suspend and resume. | 3 |
| `cas-acp` | Agent Communication Protocol server (JSON-RPC 2.0) for IDEs. | 3 |
| `cas-router` | Prefix-hash routing and load balancing for model calls. | 4 |
| `cas-deck` | Operator dashboard: Axum, Maud, htmx, Tailwind. | 5 |
| `cas-storage` | Clients for rustfs, minikv and aistore. | none (gap) |
| `cas-cli` | Binary `cas`. Kept from the template, see Decisions. | n/a |

`xtask/` also exists. It holds `spec-check` and `toolchain-check`. It is not a
CAS crate and sits outside the layering.

### Layering

Source: `cerebral-work/research@4068134:agentic-systems/designs/substrate-diagrams-and-components.md`
section 3. From the top layer to the bottom:

```text
deck > acp > engine > pool > sandbox, ebpf > router > domain
```

The rule: a crate depends only on crates in lower layers. `cas-domain` depends
on no other crate in the workspace and has zero I/O. It uses no sockets, no
files, no processes, no database driver and no async runtime. `make smoke`
greps for `std::net`, `std::fs`, `std::process` and `tokio` in its source.

Two placements are not in the diagram:

- `cas-storage` is not drawn in the research diagram. This spec places it
  beside `cas-router`, above `cas-domain`. A later spec confirms or moves it.
- `cas-cli` is the application edge. It may depend on any crate.

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
3. The lease reaches `cas-engine`, which runs the session as a durable workflow.
4. The pool runs the harness in a jail. The router serves its model calls.
5. Execution receipts go to saga. An exit code of 0 earns the `verified` tier.
6. Saga compensates on failure. Results go to reverie as memory and to the
   cortex mesh as signed messages.
7. `cas-deck` connects to the cortex settle queue at `/v1/settle/pending`
   (DES section 3.6). The operator signs off there.

CAS integrates with soma, cortex, saga and reverie through service edges. It
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
| Repo is `cerebral-work/cas-substrate`. Private. Proprietary licence. | Christian | 2026-10-08 |
| First landing is the scaffold plus `cas-domain`. The other eight research crates start as placeholders. | Christian | 2026-10-08 |
| `cas-cli` stays beside the nine research crates. The smoke gate and the release tag use it. | Christian | 2026-10-08 |
| Crate prefix is `cas`. | Christian | 2026-10-08 |
| GPL dependencies are denied by `deny.toml`. | Christian | 2026-10-08 |
| CAS vendors estate contract files, never service code ("contracts yes, code no"). | Christian | 2026-10-09 |
| Phase 8 (DeltaDB) enters the epic saga run. Recorded as adoption into the plan, see Notes. | Christian | 2026-10-09 |

The research names nine crates. This repo has ten because of `cas-cli`.

### Open gaps

These are not decided. Each needs a spec or an operator ruling before code.

1. **Pool runtime.** The pool design shows thread-per-core actors (heph). The
   heph evaluation keeps Tokio for cortex. See
   `cerebral-work/research@4068134:repos/heph.md` line 92. Nobody has chosen the
   runtime for `cas-pool`. `cas-domain` stays runtime-neutral either way.
2. **Kernel `-j 2`.** The research says eBPF enforces two parallel jobs
   (`SYNTHESIS-ESTATE-AND-SUBSTRATE.md` section 4.2). The mechanism it
   describes sets `nice 19` on build processes. Nice is a scheduling priority.
   It does not cap the number of jobs. No mechanism for a job cap is designed.
3. **`cas-storage` has no phase.** The roadmap lists no phase for this crate.
   Phase 6 deploys a Kubernetes operator for aistore. That is not a crate.
4. **aistore is Go.** `cas-storage` can only be an HTTP client of it.
5. **eBPF licence.** An eBPF program that calls GPL-only kernel helpers must
   declare a GPL-compatible licence in its `license` section. (Source: the
   Linux kernel, `Documentation/bpf/bpf_licensing.rst`; not in the research.) This repo is
   proprietary. The helpers needed are unknown. Spec 0004 records the open
   question. Decide before the first program.
6. **eBPF toolchain.** The eBPF side needs nightly Rust and `bpf-linker`. The
   `ci-rust` image has neither (spec 0005).
7. **The research disagrees about the crate list.** By count on 2026-10-08:
   the synthesis table names 6 CAS crates, the diagram atlas draws 8, and the
   design names 9. The synthesis omits `cas-acp`, `cas-storage` and
   `cas-domain`. The atlas omits `cas-storage`. This spec follows the design
   (9) plus `cas-cli`.
8. **The research is a draft.** Its status is `draft` and `approved_by` is
   empty. Claims in it are design intent until measured.

## Plan

- [x] Scaffold from the estate template. Ten crates compile.
- [x] Build `cas-domain` (slot, lease, envelope, attestation, error). PR #3.
- [ ] Write the spec that chooses the pool runtime.
- [ ] Write the spec that designs a job cap for builds, or drops the claim.
- [ ] Give `cas-storage` a phase or merge it into another crate.
- [ ] Decide the eBPF licence position and record it in spec 0004.
- [ ] Add each crate dependency only when its first real code lands.

## Test

Check that the specs are valid, that every crate builds, and that the domain
crate has no I/O.

```bash
make spec-check && make build && make smoke
```

Passing means exit 0. A new `std::fs` use in `crates/cas-domain/src` makes
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

Alternatives rejected: dropping `cas-cli` to match the research list. The smoke
gate and the release tag need a binary.

DeltaDB virtual-worktree integration with Saga, Blackwall and Soma
(`cerebral-work/research@ddf42c8:agentic-systems/designs/deltadb-virtual-worktree-integration.md`,
draft, no approver):

- 2026-10-08: Christian ruled to record it here only, as proposed, not adopted.
- 2026-10-08: the cortex lane sent six review findings to the proposing lane
  (w1R:p1). One finding blocks: the design commits directly to main.
- 2026-10-09: Christian included Phase 8 (DeltaDB) in the epic saga run, by
  interview in the CAS lane. The CAS lane records this as adoption into the
  plan. Christian corrects it here if that reading is wrong.
- The blocking finding still stands. No Phase 8 code runs until a revised
  design lands that never commits directly to main. Phase 8 runs last.
- Estate-wide DeltaDB and Blackwall custody process adopted with PR-floor constraints
  across estate Rust repositories (CER-2671, operator ruling 2026-10-08). Adopts the
  integration design with PR-only landing: changesets reconcile to feature branches,
  open PRs targeting `development`, and merge on green with merge commits. Direct push
  to `main` remains forbidden.
