---
status: complete
created: 2026-10-11
tags: [engine, harness, mcp, security, isolation, phase-3]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0010-landlock-ruleset-and-abi-probe, 0025-wave-execution-on-the-custody-chain, 0031-mcp-security-baseline, 0032-reverie-access-from-a-jailed-slot, 0062-agent-harness-cli-invocation-and-environment-sanitization]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0063-agent-harness-mcp-configuration-bridge

> **Status**: complete, **Priority**: high, **Created**: 2026-10-11

## Overview

Agent harness processes (`claude`, `agy`, `omp`, `codex`) require configuration files that declare Model Context Protocol (MCP) servers. Ambient MCP configurations in repository roots (`.mcp.json`) present critical security risks [reported: spec 0025 D17]. A malicious changelist can commit a `.mcp.json` that executes arbitrary shell commands with ambient authority.

Furthermore, worker slots run with network namespace isolation (`CLONE_NEWNET`) and Landlock filesystem rules [reported: spec 0010, spec 0011]. Platform tools like Reverie memory communicate through Unix domain sockets outside the slot [reported: spec 0032]. Harnesses require a secure bridge from stdio-based MCP protocols to isolated socket endpoints.

This specification defines the agent harness MCP configuration bridge in `uas-engine`. It synthesizes isolated, role-specific configuration files in slot scratch directories. It masks untrusted repository configurations and enforces role-based tool capabilities.

Facts, measured 2026-10-11 on ceres:

- **F1 [verified]:** `claude --help` documents `--strict-mcp-config` and `--mcp-config <path>` (Claude CLI 2.1.296).
- **F2 [reported]:** Spec 0025 D17 requires the critic to ignore `.mcp.json` in the target repository.
- **F3 [verified]:** Spec 0032 provides Reverie memory access through Unix domain socket proxies under `/tmp/uas-reverie-proxy/`.
- **F4 [verified]:** Spec 0062 implements `HarnessEnv` secret scrubbing and headless argument vector generation.

## Design

The MCP configuration bridge is owned by `uas-engine::harness::mcp`. It takes agent role requirements, slot identity, and available socket proxies. It produces a valid JSON configuration file written into the slot scratch directory (`<scratch>/mcp-config.json`).

### Decisions

| # | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Synthesize MCP configuration strictly in slot scratch directory (`<scratch>/mcp-config.json`). | Prevents cross-slot state pollution and ensures automatic cleanup on slot teardown. | Spec 0062; Spec 0025 D17 |
| D2 | Always pass `--strict-mcp-config` to CLI providers that support it (Claude Code). Mask any repository-level `.mcp.json`. | Eliminates unauthorized command execution from untrusted repository files. | Spec 0025 D17 |
| D3 | Standard MCP server schema representation (`mcpServers` object with `command`, `args`, `env`). | Ensures compatibility with standard Model Context Protocol client implementations. | MCP specification 2024-11-05 |
| D4 | Translate Unix domain socket proxy endpoints (Spec 0032) into local stdio bridge commands (`uas-bridge --socket <path>`). | Bridges CLI stdio expectations with isolated Unix socket proxies without opening network ports. | Spec 0032 D2; Spec 0013 D3 |
| D5 | Filter allowed MCP servers by agent role (`Builder` vs `Critic`). | Enforces least-privilege tool access on the custody chain. | Spec 0025 F14, D17 |

### Requirements (EARS)

- **[REQ-0063-01]** (Ubiquitous): The `uas-engine` shall synthesize an isolated MCP configuration file `mcp-config.json` inside the slot scratch directory.
- **[REQ-0063-02]** (Ubiquitous): The synthesized MCP configuration shall adhere to the standard MCP schema containing a root `mcpServers` dictionary.
- **[REQ-0063-03]** (State-driven): WHILE launching Claude Code, `uas-engine` shall pass `--strict-mcp-config` and `--mcp-config <scratch>/mcp-config.json`.
- **[REQ-0063-04]** (Unwanted): IF a target repository contains `.mcp.json`, THEN `uas-engine` shall ensure the file is ignored by the harness runner.
- **[REQ-0063-05]** (Event-driven): WHEN configuring Reverie memory access, `uas-engine` shall emit an MCP server definition bridging stdio to the slot Unix domain socket.
- **[REQ-0063-06]** (State-driven): WHILE an agent role is `Critic`, `uas-engine` shall emit only read-only MCP capabilities.
- **[REQ-0063-07]** (Unwanted): IF an MCP server command is not in the approved tool allowlist, THEN `uas-engine` shall reject configuration generation with `EngineError::McpServerUnauthorized`.
- **[REQ-0063-08]** (State-driven): WHILE injecting environment variables into MCP server definitions, `uas-engine` shall scrub secret-named variables using `HarnessEnv`.

## Plan

- [x] Define `McpConfigBuilder`, `McpServerConfig`, and `McpTransport` in `uas-engine::harness::mcp`.
- [x] Add `EngineError::McpServerUnauthorized` to `uas-engine::error`.
- [x] Implement role-based server filtering (`Builder` vs `Critic`).
- [x] Implement socket bridge command serialization for Reverie UDS endpoints (Spec 0032).
- [x] Write integration test verifying synthesized JSON structure, role isolation, and secret scrubbing.

## Test

```bash
cargo test -p uas-engine -j 2 --test mcp_config_bridge
```

A passing test suite verifies that:
1. Synthesized MCP config files match the standard schema (REQ-0063-01, REQ-0063-02).
2. Claude arguments include `--strict-mcp-config` (REQ-0063-03, REQ-0063-04).
3. Reverie UDS endpoints are serialized as socket bridges (REQ-0063-05).
4. Critic roles receive strictly read-only MCP capabilities (REQ-0063-06).
5. Unauthorized server commands are rejected fail-closed (REQ-0063-07).
6. Injected server environment variables are scrubbed of credentials (REQ-0063-08).

## Notes

- Spec 0031 defines cryptographic provenance and ledger audit requirements for tool invocations. Spec 0063 focuses on the local file and transport configuration bridge required by CLI harnesses.
- Spec 0032 defines the Unix domain socket path and proxy routing for Reverie memory access from a jailed slot.
