---
status: in-progress
created: 2026-10-11
tags: [providers, harnesses, claude, agy, omp, codex, mock, engine, domain]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0006-cas-engine-and-harness-dogfood, 0025-wave-execution-on-the-custody-chain, 0057-work-items-and-approval-routes]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0058-agent-harness-providers-and-mock-runtime

> **Status**: in-progress · **Priority**: high · **Created**: 2026-10-11

## Overview

The Unsigned Agent Substrate runs agent sessions across heterogeneous agent
harnesses. In the custody chain and task admission pipelines, the runtime
previously assumed Anthropic Claude Code as the only executor. This spec defines
first-class provider abstraction in `uas-domain` and dynamic harness routing in
`uas-engine`.

Supported providers:
- `claude`: Anthropic Claude Code CLI harness.
- `agy`: Google DeepMind Antigravity CLI harness.
- `omp`: OpenMultiPlatform (Oh My Prompt) agent harness.
- `codex`: OpenAI Codex CLI agent harness.
- `mock`: In-process, hermetic test harness for offline repeatable substrate tests on ceres localhost.

## Design

### Requirements (EARS)

- **Ubiquitous:** The `Provider` enum shall reside in `uas-domain` with zero direct I/O dependencies.
- **Event-driven:** WHEN a `task.toml` specifies a `provider` key, the wave executor shall dispatch the task to that designated harness provider.
- **State-driven:** WHILE running repeatable substrate tests on localhost ceres, the executor shall permit the `mock` provider to run without external network access or credentials.
- **Unwanted behavior:** IF an unknown provider is specified in `task.toml`, THEN task admission shall fail and report the unrecognized name.
- **Optional feature:** WHERE a provider is not specified in `task.toml`, the executor shall default to `claude`.

### Decisions

| ID | Decision | Rationale | Owner |
|---|---|---|---|
| D1 | Place `Provider` in `uas-domain` as a pure enum. | Keeps domain types zero-I/O while providing canonical provider identity across all layers. | uas-architect |
| D2 | Support 5 core providers: `claude`, `agy`, `omp`, `codex`, `mock`. | Reflects actual estate harnesses and provides a hermetic test path. | uas-architect |
| D3 | Provide default model mapping per provider. | Ensures safe fallback when `task.toml` omits model. | uas-engineer |
| D4 | The `mock` provider operates without network sockets or API keys. | Required for deterministic localhost testing on physical host `ceres`. | uas-engineer |

## Plan

- [x] Define `Provider` enum in `uas-domain::provider` with serialization and parsing.
- [x] Wire `Provider` into `uas-domain::lib` exports.
- [ ] Extend `uas-engine::wave_exec` to read `provider` from `task.toml`.
- [ ] Add `mock` provider execution branch in `uas-engine` for hermetic testing.
- [ ] Add `make ceres-test` target in `Makefile`.

## Test

```bash
cargo test -p uas-domain --test lib provider::tests -- --nocapture
```

- [x] Provider parsing and string round-trips pass.
- [x] Unrecognized provider names return `ParseProviderError`.
- [x] Zero-I/O verified by `make smoke`.

## Notes

Estate harnesses running on ceres and ganymede include Claude Code, Antigravity CLI (agy),
OMP, and Codex. The mock provider enables zero-token CI testing of the entire custody chain.
