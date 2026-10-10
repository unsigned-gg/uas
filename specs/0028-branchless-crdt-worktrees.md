---
status: planned
created: 2026-10-10
tags: [cas, deltadb, crdt, worktree, phase-8, blackwall]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0002-verification-gates-and-evidence, 0003-branch-model-and-promotion, 0004-dependency-and-licence-policy, 0009-saga-compensation-runner-and-durability, 0010-landlock-ruleset-and-abi-probe, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0022-custody-landing]
---

# 0028-branchless-crdt-worktrees

## Overview

Physical Git worktree allocation causes disk exhaustion, memory starvation, and Git index lock collisions on workstation ceres.
Mandatory host throttling with `-j 2` makes disk-backed worktrees a primary bottleneck for multi-agent task execution.
CAS Phase 8 introduces branchless in-memory Conflict-Free Replicated Worktrees to replace physical worktrees for transient agent tasks.

Scope covers the in-memory delta stream engine, slot virtual worktree adapters, atomic compensation deallocation, and dependency selection for CRDT collaboration.
Slot leasing and cgroups remain governed by spec 0015. Landlock sandbox isolation remains governed by spec 0010. Saga compensation remains governed by spec 0009. Branch promotion remains governed by spec 0003 and AGENTS.md rule 4. Custody landing remains governed by spec 0022.

Facts, 2026-10-10:

- **F1 [verified, read `specs/0004-dependency-and-licence-policy.md` l.198]:** Spec 0004 explicitly defers Phase 8 CRDT selection: "- Phase 8 CRDT (`yrs` 0.28.0 pulls `smallstr`, unmaintained, RUSTSEC-2026-0215; `automerge` 0.12.0 needs Rust 1.90): chosen in the DeltaDB spec."
- **F2 [verified, throwaway crate, `cargo tree -i smallstr` and `cargo deny check advisories`]:** `yrs` 0.28.0 pulls `smallstr` 0.3.1 which triggers advisory `RUSTSEC-2026-0215` (unmaintained, no safe upgrade).
- **F3 [verified, throwaway crate, `cargo metadata` and `cargo deny check --config deny.toml`]:** `automerge` 0.12.0 declares `rust_version = "1.90.0"` and passes `deny.toml` with zero advisory, ban, or license errors.
- **F4 [verified, read `rust-toolchain.toml` l.3]:** The pinned toolchain is `rustc 1.94.1`.
- **F5 [verified, read `specs/0001-cas-architecture-and-crate-layout.md`]:** Physical worktree allocation causes disk exhaustion and git lock contention on ceres under mandatory `-j 2` throttling.

## Design

### Architecture

CAS Phase 8 implements a two-tier hybrid architecture:

1. **Tier 1: Native In-Memory Delta Stream (Default):**
   - Implemented in `cas-engine::delta_stream`.
   - Records sequential `(run_hash, seq)` operation deltas in memory.
   - Zero external CRDT dependencies (uses existing `serde` and `thiserror`).
   - Target memory consumption is below 5 MB per active slot with instant rollback.
   - Serves standard agent tool execution turns (writes, edits, patches).

2. **Tier 2: Optional Document CRDT Adapter:**
   - Powered by `automerge` 0.12.0 behind cargo feature `crdt-collab`.
   - Enabled only when concurrent character-level collaborative editing is required.
   - `yrs` 0.28.0 is disqualified due to unmaintained dependency `smallstr` (`RUSTSEC-2026-0215`).
   - `loro` is deferred to Phase 9.

### Requirements (EARS)

- **[REQ-0028-01]** (Ubiquitous): The CAS execution engine shall execute transient agent file operations through branchless in-memory delta streams without allocating physical Git worktrees on disk.
- **[REQ-0028-02]** (Ubiquitous): The CAS dependency manager shall reject any candidate CRDT dependency with active security advisories or unmaintained status reported by `cargo deny`.
- **[REQ-0028-03]** (Ubiquitous): The CAS virtual workspace manager shall assign a deterministic, monotonically increasing identifier `(run_hash, seq)` to every operation delta.
- **[REQ-0028-04]** (Event-driven): WHEN an agent invokes a file modification tool, the virtual workspace adapter shall buffer the mutation as an in-memory `DeltaRecord` without disk write I/O.
- **[REQ-0028-05]** (Event-driven): WHEN a leased slot is released or cancelled by the pool manager (spec 0015) or compensated by Saga (spec 0009), the virtual stream manager shall deallocate all associated in-memory delta buffers without leaving disk residue.
- **[REQ-0028-06]** (State-driven): WHILE a virtual workspace is active, the delta stream reader shall enforce a target memory ceiling of 5 megabytes per active slot and a heap limit of 50 megabytes.
- **[REQ-0028-07]** (Unwanted): IF an ingested delta sequence number is non-monotonic, THEN the virtual stream reader shall reject the delta with `SequenceOutOfOrder`.
- **[REQ-0028-08]** (Unwanted): IF a candidate CRDT crate introduces an unmaintained transitive dependency, THEN the build gate shall fail immediately.
- **[REQ-0028-09]** (Optional): WHERE concurrent character-level collaborative editing is enabled by cargo feature `crdt-collab`, the CAS engine shall activate the secondary `automerge` document adapter.
- **[REQ-0028-10]** (Optional): WHERE the `DELTADB_WORKSPACE` environment variable is present, the Cortex client shall bind `WorkspaceKind::DeltaDbVirtualThread`.
- **[REQ-0028-11]** (Complex): WHILE an active delta stream is being materialized for custody landing under spec 0022, WHEN all step verification gates pass, the virtual stream reader shall export the committed delta sequence as an in-memory unified patch.

### State Machines and Invariants

```
+--------------+   slot leased (spec 0015) +-------------------+
|     Cold     | -----------------------> |    SlotLeased     |
+--------------+                          +-------------------+
                                                    |
                                                    | ingest DeltaRecord
                                                    v
+--------------+   land custody (spec 0022)+-------------------+
|  Committed   | <----------------------- |  StreamingDeltas  |
+--------------+                          +-------------------+
                                                    |
                                                    | deallocate (spec 0009)
                                                    v
                                          +-------------------+
                                          |     Deallocate    |
                                          +-------------------+
                                                    |
                                                    | free buffers
                                                    v
                                          +-------------------+
                                          |   Zero Disk Res   |
                                          +-------------------+
```

Invariants:
1. **Delta Sequence Invariant:** Sequence numbers within a run must satisfy `seq(n) == seq(n-1) + 1`.
2. **Clean Memory Reclamation:** Buffer deallocation must free all delta buffers without leaving files in `/tmp` or `.git/`.
3. **Immutability Invariant:** Once an in-memory delta sequence is exported for custody landing, its recorded deltas become append-only.

### Error Containment

- Ingestion failures: Malformed delta payloads emit `DeltaStreamError::InvalidPayload` and fail the active slot without contaminating sibling slots.
- Memory ceiling breach: If a virtual workspace exceeds the 50 MB target heap ceiling, the watcher suspends the slot and requests prompt compaction.
- Sequence disorder: Out-of-order delta sequence arrivals trigger immediate rejection with `SequenceOutOfOrder`.

## Plan

- [ ] 1. Implement in-memory `DeltaRecord` and `DeltaStreamReader` in `cas-engine`.
- [ ] 2. Update `specs/0004-dependency-and-licence-policy.md` recording disqualification of `yrs` and conditional adoption of `automerge` under optional feature.
- [ ] 3. Implement virtual worktree adapter in `cas-pool` connecting leased slots to `DeltaStreamReader`.
- [ ] 4. Implement atomic buffer deallocation on slot release or Saga compensation with zero disk residue.
- [ ] 5. Implement optional `automerge` document CRDT adapter behind cargo feature `crdt-collab`.
- [ ] 6. Add unit and integration test suite verifying monotonic delta sequencing and memory ceiling compliance.

## Test

```bash
cargo test -p cas-engine --test deltadb_stream -j 2 && cargo test -p cas-pool -j 2
```

A pass means in-memory delta streams record sequential edits without physical worktree creation, invalid sequence numbers reject with `SequenceOutOfOrder`, buffer deallocation leaves zero disk residue, and `cargo deny check` passes with zero advisory warnings.

## Notes

### Design Trade-Offs

- **Native Delta Stream vs Document CRDT:** Agent tools operate in discrete turns (`Write`, `Edit`, `Patch`). Sequential monotonic `DeltaRecord` events mirror agent turns accurately with microsecond latency and zero external dependency risk. Heavy document CRDTs add unnecessary overhead for single-actor agent turns.
- **Dependency Disqualification:** `yrs 0.28.0` is disqualified due to unmaintained `smallstr` (`RUSTSEC-2026-0215`). `automerge 0.12.0` meets all licence, audit, and MSRV criteria for multi-party collaborative editing.
- **Upstream Zed DeltaDB Tracking:** Zed DeltaDB remains closed-source. In-memory monotonic delta logging provides functional architectural parity without proprietary vendor code.
