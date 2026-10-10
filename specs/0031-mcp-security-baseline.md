---
status: planned
created: '2026-10-10'
tags: [architecture, security, mcp]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0013-llm-gateway-socket-proxy, 0016-zed-agent-communication-protocol]
---

# 0031-mcp-security-baseline

> **Status**: planned - **Priority**: high - **Created**: 2026-10-10

## Overview

Agentic systems executing tool calls against enterprise infrastructure demand strict cryptographic provenance, derived from invariant 4.13 in the estate synthesis (reported). The NSA Model Context Protocol (MCP) Security Baseline mandates in-line control planes to prevent unauthorized access. This specification defines the cryptographic enforcement, ephemeral tool tokens, and tamper-evident auditing required for secure MCP execution. (Note: Permission intersection is governed by spec 0029, REQ-0029-03).

## Design

The AI Control Plane (`cas-acp`) enforces digital signatures at the wire protocol layer. Tool executions are bound to short-lived tokens to eliminate ambient authority. The `cas-engine` records operations to a ledger, while `cas-sandbox` quarantines unauthorized local servers.

### Decisions

| Decision | Rationale | Source |
|---|---|---|
| Ed25519 Signatures | Best performance and security trade-off for JSON-RPC wire protocol signatures. | CAS spec lane w10:pN, 2026-10-10 |
| 30-second Token Lifetime | Balances prompt execution windows with narrow attack surfaces for ambient authority. | CAS spec lane w10:pN, 2026-10-10 |
| Merkle Ledger | Hash-chained append-only ledgers ensure tamper-evident auditing for all tool I/O. | CAS spec lane w10:pN, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0031-01]** (Ubiquitous): The `cas-acp` shall enforce Ed25519 digital signatures on all messages at the JSON-RPC wire protocol layer.
- **[REQ-0031-02]** (Event-driven): WHEN a tool is invoked, the `cas-acp` shall issue an execution token scoped strictly to that single tool call with a 30-second time-to-live expiration.
- **[REQ-0031-03]** (State-driven): WHILE tool calls execute, the `cas-engine` shall record signed inputs, outputs, and authorization policies in a hash-chained append-only Merkle ledger.
- **[REQ-0031-04]** (Unwanted): IF a workstation monitor detects an unapproved local MCP server, THEN the `cas-sandbox` shall quarantine the rogue endpoint immediately.
- **[REQ-0031-05]** (Ubiquitous): The `cas-acp` shall enforce tool allowlisting against an explicit list of approved capabilities per agent role.
- **[REQ-0031-06]** (Event-driven): WHEN a new MCP server is connected, the `cas-acp` shall verify server provenance and pin its cryptographic identity.
- **[REQ-0031-07]** (Unwanted): IF an MCP server is marked compromised, THEN the `cas-acp` shall immediately revoke its credentials and terminate active connections.
- **[REQ-0031-08]** (State-driven): WHILE parsing tool outputs, the `cas-acp` shall employ content sanitization to contain prompt injections embedded in the output.
- **[REQ-0031-09]** (Unwanted): IF the append-only Merkle ledger cannot be written, THEN the `cas-engine` shall halt further tool executions until durability is restored.

## Plan

- [ ] Implement Ed25519 digital signature verification and tool allowlisting for JSON-RPC payloads in `cas-acp`.
- [ ] Develop the 30-second TTL single-use tool execution token issuer and server provenance verification in `cas-acp`.
- [ ] Integrate the hash-chained Merkle ledger and failure halts for tool inputs and outputs in `cas-engine`.
- [ ] Implement the rogue MCP server detection and containment ruleset in `cas-sandbox`.

## Test

```bash
cargo test -p cas-acp -j 2 --test mcp_security_enforcement
```

A passing test suite verifies that unsigned JSON-RPC messages are rejected and valid messages produce deterministic append-only ledger entries. As a negative control, if the tool execution TTL check is deleted, the test executing a tool token at 31 seconds must go red.

## Notes

The requirement for an append-only Merkle ledger aligns with the Soma ledger architecture (reported). Cryptographic primitives for Ed25519 signatures will require updates to the pinned dependencies in spec 0004. Containment of rogue servers leverages Landlock capabilities in `cas-sandbox` and `cas-ebpf` (asserted).
