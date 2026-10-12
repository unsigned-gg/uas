---
status: planned
created: 2026-10-11
tags: [providers, streaming, adapter, events, jsonrpc, acp, engine, harnesses]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0006-cas-engine-and-harness-dogfood, 0016-zed-agent-communication-protocol, 0058-agent-harness-providers-and-mock-runtime, 0060-agent-harness-process-supervision-and-signals-coordination]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0061-agent-harness-adapter-streaming-and-event-translation

> **Status**: planned · **Priority**: high · **Created**: 2026-10-11

## Overview

The Unsigned Agent Substrate coordinates diverse agent harnesses (`claude`, `agy`, `omp`, `codex`,
and `mock`) across isolated worker slots. Each harness exposes different process execution models,
streaming formats (ndjson, SSE, JSON-RPC 2.0, or raw terminal sequences), and token accounting.

Spec 0058 established the canonical `Provider` enum and basic mock runtime routing. Spec 0060
established process group supervision and signal coordination. This specification defines the unified
streaming event adapter and translation pipeline in `uas-engine`:
1. Unifying stream event parsing across heterogeneous harness stdout/stderr streams.
2. Mapping harness-specific event schemas into substrate `WorkflowEvent` records.
3. Tracking cumulative and per-turn token usage and model latency.
4. Resynchronizing on malformed lines and partial chunks without crashing the session.
5. Providing a hermetic streaming mock harness for repeatable localhost testing on `ceres`.

## Design

### Architecture

The `uas-engine` crate implements a streaming adapter pipeline:
- `HarnessAdapter`: Trait for spawning harness processes and wrapping their I/O streams into a unified `HarnessEventStream`.
- `HarnessEvent`: Internal event representation capturing thought deltas, tool calls, tool results, turn boundaries, and token metrics.
- `StreamTranslator`: State machine that consumes raw bytes or lines from a harness stdout, detects frame boundaries, parses provider-specific envelopes, and emits verified `WorkflowEvent` objects.
- `MockHarnessStream`: Deterministic in-memory stream generator for offline, repeatable localhost verification under `-j 2` CPU constraints on physical host `ceres`.

### Decisions

| ID | Decision | Rationale | Owner |
|---|---|---|---|
| D1 | Stream parsing resides in `uas-engine`. | Keeps `uas-domain` strictly zero-I/O while centralizing workflow event translation. | uas-architect |
| D2 | Maximum frame size capped at 1 MiB (`1024 * 1024` bytes). | Matches ACP server framing threshold from Spec 0016; protects against memory exhaustion. | uas-engineer |
| D3 | Malformed frames trigger resynchronization at newline boundaries. | Prevents a single corrupt line from failing a long-running multi-turn session. | uas-engineer |
| D4 | The `mock` adapter produces deterministic token metrics and tool call streams. | Enables hermetic testing of the entire stream processing pipeline without network access. | uas-engineer |

### Requirements (EARS)

- **[REQ-0061-01]** (Ubiquitous): The `uas-engine` shall define a unified `HarnessAdapter` trait and `HarnessEvent` enum for translating heterogeneous agent process streams into substrate workflow events.
- **[REQ-0061-02]** (Event-driven): WHEN an agent harness yields structured NDJSON output or JSON-RPC 2.0 frames on stdout, the adapter shall translate turns, thoughts, tool invocations, and completions into typed `WorkflowEvent` variants.
- **[REQ-0061-03]** (State-driven): WHILE a prompt turn is active, the adapter shall aggregate token usage metrics (`prompt_tokens`, `completion_tokens`, `cached_tokens`) and execution latency without dropping partial frames.
- **[REQ-0061-04]** (Unwanted): IF a child harness emits malformed or truncated stream output, THEN the adapter shall record a framing error event, advance past the corrupted newline delimiter, and continue processing subsequent frames.
- **[REQ-0061-05]** (Event-driven): WHEN executing under the `mock` provider, the adapter shall replay deterministic simulated events and token usage without spawning external binaries or opening network sockets.
- **[REQ-0061-06]** (Ubiquitous): The adapter stream parser shall enforce a maximum frame size limit of 1 MiB (`1024 * 1024` bytes), matching the ACP server threshold in Spec 0016.

## Plan

- [ ] Define `HarnessEvent` and `TokenMetrics` types in `uas-engine`.
- [ ] Implement `StreamTranslator` with newline delimiter parsing and 1 MiB frame limit protection.
- [ ] Implement `ClaudeStreamParser`, `AgyStreamParser`, and `OmpStreamParser` modules.
- [ ] Implement `MockStreamGenerator` for deterministic localhost test replay.
- [ ] Wire stream events into `WorkflowSession::handle_event` and event store persistence.
- [ ] Add unit and integration tests covering normal streams, malformed line recovery, and mock replay.

## Test

```bash
cargo test -p uas-engine --test harness_stream_translation -j 2
```

This test suite exercises stream parsing, token accounting, and malformed line resynchronization across mock and translated agent harness event sequences. As a negative control, if the 1 MiB frame size limit is deleted, feeding an oversized 2 MiB line must go red and fail to reject the frame.

## Notes

The 1 MiB frame threshold directly mirrors `crates/uas-acp/src/server.rs:28` (`MAX_FRAME_BYTES`). Token usage metrics align with the billing and lease budget structures defined in Spec 0007 and Spec 0026.
