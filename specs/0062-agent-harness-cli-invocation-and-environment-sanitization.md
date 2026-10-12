---
status: complete
created: 2026-10-11
tags: [providers, harnesses, environment, sanitization, cli, security, engine]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0006-cas-engine-and-harness-dogfood, 0013-llm-gateway-socket-proxy, 0032-reverie-access-from-a-jailed-slot, 0058-agent-harness-providers-and-mock-runtime, 0060-agent-harness-process-supervision-and-signals-coordination, 0061-agent-harness-adapter-streaming-and-event-translation]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0062-agent-harness-cli-invocation-and-environment-sanitization

> **Status**: complete · **Priority**: high · **Created**: 2026-10-11

## Overview

The Unsigned Agent Substrate launches agent harness processes across heterogeneous CLI providers
(`claude`, `agy`, `omp`, `codex`, and `mock`) inside isolated worker slots. Each harness process requires
precise command-line arguments and a strictly sanitized environment.

Ambient host processes frequently contain sensitive credentials in environment variables (such as
`GITHUB_TOKEN`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `AWS_SECRET_ACCESS_KEY`, and SSH variables).
Passing these ambient variables into a jailed slot risks leaking credentials into logs, tool invocations,
and child processes. Furthermore, default user home paths (`~/.claude`, `~/.config`, `~/.gemini`) can cause
unwanted state crosstalk between independent agent slots or host sessions.

This specification defines the environment sanitization and CLI invocation protocol in `uas-engine`:
1. Strict allowlist-based environment scrubbing that strips host credentials.
2. Injection of slot identity metadata (`UAS_SLOT_ID`, `UAS_LEASE_ID`, `UAS_SESSION_ID`, `UAS_PROVIDER`, `UAS_MODEL`).
3. Configuration of proxy endpoints (`HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`) for socket proxies.
4. Isolation of harness configuration directories (`HOME`, `XDG_CONFIG_HOME`, `CLAUDE_CONFIG_DIR`, `AGY_CONFIG_DIR`) to slot scratch trees.
5. Canonical, headless non-interactive CLI argument generation across all supported harness providers.

## Design

### Architecture

The `uas-engine` crate implements `HarnessCommandBuilder` and `HarnessEnv`:
- `HarnessEnv`: Manages the sanitized environment map. It clears the inherited process environment and populates only explicitly permitted system variables (`PATH`, `USER`, `LANG`, `LC_ALL`, `TERM`), followed by UAS slot-specific variables.
- `HarnessCommandBuilder`: Constructs the complete subprocess execution specification (`program`, `args`, `env`, `current_dir`) tailored to the target provider.
- `ProviderArgs`: Formats provider-specific flags for headless execution:
  - `Claude`: `["claude", "--print", "--output-format", "json", "-p", "<prompt>", "--dangerously-skip-permissions"]`
  - `Agy`: `["agy", "run", "--stream", "--format", "jsonl", "--prompt", "<prompt>"]`
  - `Omp`: `["omp", "exec", "--json", "--prompt", "<prompt>"]`
  - `Codex`: `["codex", "exec", "--json", "<prompt>"]`
  - `Mock`: `["mock-harness", "--task", "<prompt>"]`

### Decisions

| ID | Decision | Rationale | Owner |
|---|---|---|---|
| D1 | Allowlist over denylist for environment variables. | Denylists inevitably miss newly added credential names; allowlisting ensures complete confidentiality. | uas-architect |
| D2 | Isolate harness configuration roots to `<slot_dir>/.config`. | Prevents reading host configuration files and eliminates inter-slot configuration leakage. | uas-engineer |
| D3 | Reject empty prompts and prompts containing null bytes (`\0`). | Prevents argument injection and invalid execution states before launching child processes. | uas-engineer |
| D4 | Proxy environment variables point strictly to jailed UDS or localhost bridges. | Restricts egress to verified socket proxies per Spec 0013 and Spec 0032. | uas-security |

### Requirements (EARS)

- **[REQ-0062-01]** (Ubiquitous): The `uas-engine` shall construct child process environments for agent harnesses using strict allowlisting, scrubbing all ambient host secrets and API credentials.
- **[REQ-0062-02]** (Event-driven): WHEN spawning a harness provider process, the `uas-engine` shall inject slot identity variables `UAS_SLOT_ID`, `UAS_LEASE_ID`, `UAS_SESSION_ID`, `UAS_PROVIDER`, and `UAS_MODEL`.
- **[REQ-0062-03]** (State-driven): WHILE a harness process executes in a jailed slot, the `uas-engine` shall point config and state home directories (`HOME`, `XDG_CONFIG_HOME`, `CLAUDE_CONFIG_DIR`, `AGY_CONFIG_DIR`) to isolated paths within the slot worktree.
- **[REQ-0062-04]** (Ubiquitous): The `uas-engine` shall generate deterministic CLI argument vectors (`argv`) tailored for headless non-interactive execution for each supported `Provider` (`Claude`, `Agy`, `Omp`, `Codex`, `Mock`).
- **[REQ-0062-05]** (Unwanted): IF an environment variable name matches the pattern of a credential or secret (`*KEY*`, `*TOKEN*`, `*SECRET*`, `*PASSWORD*`, `*CRED*`, `*AUTH*`), THEN `uas-engine` shall exclude it from the child environment unless explicitly scoped as a slot proxy socket.
- **[REQ-0062-06]** (Event-driven): WHEN socket proxies are configured for the slot, the `uas-engine` shall inject `HTTP_PROXY`, `HTTPS_PROXY`, and `ALL_PROXY` referencing the local proxy endpoint.
- **[REQ-0062-07]** (Unwanted): IF a task prompt is empty or contains null bytes, THEN `uas-engine` shall reject command generation with `EngineError::InvalidCommandInput`.
- **[REQ-0062-08]** (Ubiquitous): The `uas-domain` shall maintain zero I/O and zero dependency on command execution primitives.

## Plan

- [x] Define `HarnessCommandBuilder`, `HarnessEnv`, and `HarnessInvocation` in `uas-engine::harness`.
- [x] Implement allowlist-based environment sanitization filtering out ambient credentials.
- [x] Implement CLI argument generators for `Claude`, `Agy`, `Omp`, `Codex`, and `Mock`.
- [x] Implement prompt validation rejecting empty strings and null bytes.
- [x] Add unit and integration tests in `crates/uas-engine/tests/harness_invocation.rs`.
- [x] Register spec 0062 in `specs/README.md` and validate with `cargo xtask spec-check`.

## Test

```bash
cargo test -p uas-engine -j 2 --test harness_invocation
```

This test suite verifies:
1. Environment scrubbing strips ambient secrets while preserving allowlisted system variables (REQ-0062-01, REQ-0062-05).
2. Slot metadata and proxy settings are injected correctly (REQ-0062-02, REQ-0062-06).
3. Configuration roots point inside the slot path (REQ-0062-03).
4. CLI arguments match canonical headless vectors for each provider (REQ-0062-04).
5. Prompts with null bytes or empty contents are rejected (REQ-0062-07).
6. Negative control: deleting the secret scrubbing filter causes tests with injected `*_KEY` variables to fail.

## Notes

- Preserves crate layering: `uas-domain` remains purely zero-I/O.
- Coordinates with Spec 0013 (LLM gateway proxy) and Spec 0032 (Reverie proxy).
- Enforces physical host `ceres` throttling (`-j 2` mandatory).
