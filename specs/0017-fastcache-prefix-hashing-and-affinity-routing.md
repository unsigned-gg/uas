---
status: in-progress
created: 2026-10-09
tags: [router, fastcache, prefix-hash, affinity, rendezvous, sse, phase-4]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0004-dependency-and-licence-policy, 0013-llm-gateway-socket-proxy]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0017-fastcache-prefix-hashing-and-affinity-routing

## Overview

UAS agent runners make frequent LLM completions. Every agent turn sends a large
prompt prefix. This prefix includes system rules, tool definitions, and LeanSpecs.
Re-computing KV-cache tokens across vLLM replicas wastes GPU memory and compute.

This spec defines intelligent model call routing in `uas-router`:
- **CER-2729**: Canonical prompt prefix serializer and SHA-256 hasher.
- **CER-2730**: Consistent hash ring with replica affinity and cache hit scoring.
- **CER-2731**: Dual-stream token multiplexer for cognitive `<think>` telemetry.
- **CER-2732**: Active replica health probing and zero-interruption circuit breaking.

Parent tickets: CER-2690, CER-2691, CER-2692, CER-2693.

Facts, measured and recorded 2026-10-09:
- **Upstream Gateway [verified]:** Estate LLM gateway operates at
  `http://llm` and in-cluster at `http://llm.llm:4000`.
- **Approved Crates [verified]:** Spec 0004 approves `sha2` 0.11.0 and
  `futures-util` 0.3.34 for `uas-router`. `tokio` 1.53.1 is in use.
- **Domain Purity [verified 2026-10-11: `crates/uas-router/Cargo.toml` lists only `thiserror`, `sha2` and `tokio`]:** `uas-router` does not depend on `uas-domain`; the earlier text said it did, for
  session identifiers and slot leases. It performs asynchronous network I/O.
- **Cognitive Tag Conventions [reported]:** Reasoning models emit thinking
  tokens wrapped in `<think>...</think>` XML-like blocks.

## Design

### Requirements (EARS)

- **[REQ-0017-01]** (Ubiquitous): The `uas-router` shall serialize and hash prompt prefixes deterministically using SHA-256 for replica affinity.
- **[REQ-0017-02]** (Event-driven): WHEN a completion request arrives, `uas-router` shall select the target replica using rendezvous hashing.
- **[REQ-0017-03]** (Event-driven): WHEN streaming tokens contain `<think>` tags, the router shall route thought traces to telemetry and clean tokens to clients.
- **[REQ-0017-04]** (State-driven): WHILE polling replica health endpoints, the router shall trip circuit breakers after two consecutive probe failures.
- **[REQ-0017-05]** (Unwanted behavior): IF a chosen replica becomes unavailable, THEN the router shall fail over to the next healthy replica.

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | Canonical prefix serialization: Sort tool schemas alphabetically by name, then by json bytes. Normalize system instructions to deterministic UTF-8 bytes before hashing. Every field is length-prefixed with its byte length in decimal: `SYS:<len>:<part>,...\|TOOLS:<len>:<name><len>:<json>,...`. Length prefixes stop a separator inside a part from imitating another separator. | UAS lane, 2026-10-09. Amended 2026-10-10, lane w25:p1, Christian may override (the unescaped `\|`-joined form collided). |
| D2 | SHA-256 prefix hash: Hash the invariant prefix with `sha2` 0.11.0. Use the first 16 bytes as the affinity routing key. | UAS lane, 2026-10-09 |
| D3 | Rendezvous routing: Use Highest Random Weight (HRW) rendezvous hashing across active replica nodes. Achieve deterministic sticky routing without centralized state. | UAS lane, 2026-10-09 |
| D4 | Dual-stream token multiplexer: Intercept streaming SSE chunks. Parse and strip `<think>` blocks. Route thought traces to telemetry and clean completions to client. A `</think>` tag with no open `<think>` block is dropped. The text around it stays on the clean stream. | UAS lane, 2026-10-09. Stray closing tag rule: UAS lane, 2026-10-11, Christian may override. |
| D5 | Active health probe: Probe replica `/healthz` or `/health` every 5 seconds. Probe path: `GET /healthz`; a `404` answer is retried once on `/health` in the same probe. Mark replica unavailable after 2 consecutive failures. Restore: 3 consecutive successes mark an unavailable replica available again. Fail over in-flight requests. Fallback: when every replica is unhealthy, `choose` picks over all registered replicas in ascending id order at index `u64::from_be_bytes(prefix_hash[0..8]) % n` and returns `ReplicaChoice::Fallback`, so callers can log it. An empty table is `NoHealthyReplicas`. | UAS lane, 2026-10-09. Fallback added 2026-10-10, lane w25:p1, Christian may override. Probe path and restore rule: UAS lane, 2026-10-11, Christian may override. |
| D6 | Zero-allocation buffer: Use circular byte window for streaming token parsing. Detect boundary tags without heap allocations. | UAS lane, 2026-10-09 |

### Architecture and Flow

```mermaid
flowchart TD
    subgraph Client["Agent Runner / ACP"]
        Req["POST /v1/chat/completions"]
        RespStream["Clean Stream (Markdown / Tools)"]
    end

    subgraph Router["uas-router (Phase 4)"]
        Parser["Prefix Canonicalizer"]
        Hasher["SHA-256 Hasher"]
        HRW["Rendezvous Hash Selector"]
        Circuit["Circuit Breaker & Health Table"]
        Mux["Dual-Stream Token Multiplexer"]
    end

    subgraph Upstream["vLLM / LiteLLM Fleet"]
        NodeA["vLLM Replica A (FastCache Warm)"]
        NodeB["vLLM Replica B"]
        NodeC["vLLM Replica C (Standby)"]
    end

    subgraph Telemetry["Estate Telemetry"]
        Logs["Cortex Audit & Reverie Trace"]
    end

    Req --> Parser
    Parser --> Hasher
    Hasher -->|16-byte Prefix Hash| HRW
    Circuit -->|Health Weights| HRW
    HRW -->|Affinity Route| NodeA
    NodeA -->|Raw SSE Stream| Mux
    Mux -->|Thought Trace <think>| Logs
    Mux -->|Clean Completion| RespStream
```

### Components

#### 1. Prefix Canonicalizer & Hasher (`prefix.rs`)
- Parses incoming JSON request payloads for `/v1/chat/completions`.
- Extracts the system parts: each `messages[]` entry whose `role` is `system` or
  `developer`, in request order. Its `content` is a string or an array of
  `{"type":"text","text":...}` parts. UAS lane, 2026-10-11, Christian may override.
- Extracts `tools` definitions. A tool's name is `tools[].function.name`. Sorts tools
  by that name, then by json bytes.
- Serializes the canonical UTF-8 form of D1, every field length-prefixed with its
  byte length in decimal: `SYS:<len>:<part>,...|TOOLS:<len>:<name><len>:<json>,...`
- Computes SHA-256 digest via `sha2::Sha256`.
- Emits `PrefixHash([u8; 16])`.

#### 2. Consistent Rendezvous Router (`affinity.rs`)
- Implements Highest Random Weight (HRW) hashing.
- For a given `PrefixHash` and replica list:
  `weight(node, hash) = hash64(node_id || prefix_hash)`
- Selects replica with highest weight.
- When an active replica fails, requests route smoothly to next highest replica.
- Minimal cache disruption during node additions or removals.

#### 3. Dual-Stream Token Multiplexer (`multiplexer.rs`)
- Receives raw Server-Sent Events (`data: {"choices":[{"delta":{"content":"..."}}]}`).
- Maintains state machine: `Initial`, `Thinking`, `Completion`.
- Detects opening `<think>` tag. Diverts subsequent tokens to telemetry channel.
- Detects closing `</think>` tag. Diverts subsequent tokens to output client channel.
- Emits `ThoughtChunk` to internal logging channel and `CleanChunk` to client stream.

#### 4. Active Health Monitor (`health.rs`)
- Background task probes replica endpoints with HTTP `GET /healthz`.
- Interval: 5000 milliseconds. Timeout: 1000 milliseconds.
- Thresholds: 2 consecutive failures mark node unhealthy; 3 consecutive successes restore node.
- Unhealthy nodes are excluded from HRW candidate set.

### Invariants

1. **Deterministic Hashing:** Identical system prompts and tool sets always yield identical `PrefixHash` values.
2. **Transparent Fallback:** If all affinity replicas are unhealthy, the router picks over every registered replica in ascending id order at the
   deterministic index `u64::from_be_bytes(prefix_hash[0..8]) % n`, and returns
   `ReplicaChoice::Fallback` (D5).
3. **Stream Purity:** Client streams never receive unclosed or raw `<think>` tags when dual-streaming is active.
4. **Host Throttling:** Network polling and multiplexing run on asynchronous Tokio tasks without blocking OS threads.

### Errors

- `RouterError::InvalidPayload`: Malformed JSON or missing completion fields.
- `RouterError::NoHealthyReplicas`: The table holds no replica. An all-unhealthy table falls back (D5).
- `RouterError::UpstreamTimeout`: Target replica did not respond within configured timeout.
- `RouterError::StreamTerminated`: Upstream disconnected mid-completion.

## Plan

- [x] Add `sha2 = "0.11.0"` to workspace dependencies [verified 2026-10-11: root
  `Cargo.toml` line 54; `crates/uas-router/Cargo.toml` line 14].
- `futures-util`: not needed, no `Stream` in the design (plan 0017 finding 2).
- [x] Implement `uas-router::prefix::PrefixInput::hash` over the D1 canonical form.
- [ ] Build a `PrefixInput` from a chat-completions request body (JSON canonicalization,
  plan 0017 P4).
- [x] Implement `uas-router::affinity::AffinityTable` with per-replica health (`set_healthy`).
- [ ] Implement `uas-router::multiplexer::DualStreamMultiplexer` state machine.
- [ ] Implement `uas-router::health::HealthMonitor` background prober.
- [ ] Unit and property tests:
  - Tool ordering permutations produce identical `PrefixHash`.
  - Node removal moves only 1/N keys in HRW ring.
  - Streaming token splitter properly extracts `<think>` blocks without dropping surrounding text.
- [ ] Update `specs/README.md` and roadmap artifacts.

## Test

One row per requirement. A filter that matches no test makes `cargo nextest run` exit 4,
so a row stays red until its tests exist. The last row runs every test in the crate.

| Requirement | Command | Today (2026-10-11) |
|---|---|---|
| REQ-0017-01 | `cargo nextest run -p uas-router prefix:: --build-jobs 2` | exit 0, 5 tests pass |
| REQ-0017-02 | `cargo nextest run -p uas-router affinity:: --build-jobs 2` | exit 0, 12 tests pass |
| REQ-0017-03 | `cargo nextest run -p uas-router multiplexer:: --build-jobs 2` | exit 4, no tests to run |
| REQ-0017-04 | `cargo nextest run -p uas-router health:: --build-jobs 2` | exit 4, no tests to run |
| REQ-0017-05 | `cargo nextest run -p uas-router --test failover --build-jobs 2` | exit 101, no test target `failover` |
| all | `cargo test -p uas-router -j 2` | exit 0, 27 tests pass |

The Today column is [verified 2026-10-11: each command run in a worktree at
`origin/development` `eb149a0`].

```bash
cargo nextest run -p uas-router prefix:: --build-jobs 2
cargo nextest run -p uas-router affinity:: --build-jobs 2
cargo nextest run -p uas-router multiplexer:: --build-jobs 2
cargo nextest run -p uas-router health:: --build-jobs 2
cargo nextest run -p uas-router --test failover --build-jobs 2
cargo test -p uas-router -j 2
```

## Notes

- **PagedAttention Synergy:** vLLM caches KV blocks by hash of prompt token prefixes.
  Consistent routing ensures agent sessions reuse KV cache blocks across turns.
- **LiteLLM Compatibility:** Request and response framing complies with OpenAI
  v1 Chat Completions specification.
- **Not wired yet [verified 2026-10-11: `grep -rln AffinityTable crates/` lists only
  `affinity.rs` and `lib.rs`; `uds_proxy.rs` line 572 connects to one fixed upstream]:**
  The router does not yet route requests by affinity, because the proxy sends every request
  to one fixed upstream. Wiring `AffinityTable` into the proxy and the telemetry sink for
  thought chunks need a follow-up spec (plan 0017 finding 9).
