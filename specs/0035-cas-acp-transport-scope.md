---
status: planned
created: 2026-10-10
tags: [acp, transport, zed]
priority: high
depends_on: [0011-network-namespace-isolation, 0013-llm-gateway-socket-proxy, 0016-zed-agent-communication-protocol]
---

# 0035-cas-acp-transport-scope

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

Addresses the contradiction between `crates/uas-acp/README.md` (which claims TCP support) and Spec 0016 (which strictly defines stdio). Determines the transport scope for `uas-acp`.

### Facts
- **F1** [verified 2026-10-10]: `crates/uas-acp/README.md` claims TCP support. Command: `grep -in "TCP" crates/uas-acp/README.md` returns lines containing TCP.
- **F2** [verified 2026-10-10]: Spec 0016 does not define TCP. Command: `grep -cin tcp specs/0016*.md` returns 0.
- **F3** [verified 2026-10-10]: `uas-acp` binds no TCP today. Command: `grep -rn 'TcpListener\|tokio::net\|std::net' crates/uas-acp/src/` returns nothing.

## Design

### Decisions

| ID | Decision | Source |
|---|---|---|
| D1 | `uas-acp` does not get a TCP transport; it remains strictly stdio over JSON-RPC 2.0. | proposed: lane w10:pN, 2026-10-10; awaits integrating lane or Christian |

Reasoning: Spec 0016 D2 pins ACP 2.2.0 with no custom framing. TCP without custom framing (like TLS-PSK or token headers) provides no authentication mechanism, exposing the JSON-RPC interface. Stdio is sufficient for the Zed integration path. This removes an advertised capability, so it awaits integrating lane or Christian's confirmation before the README and manifest change.

### Requirements (EARS)

- **[REQ-0035-01]** (Ubiquitous): The `uas-acp` crate shall exclusively use stdio for its JSON-RPC 2.0 transport.
- **[REQ-0035-02]** (Unwanted): IF `uas-acp` is built or tested, THEN its source code shall not contain references to TCP networking (`std::net` or `tokio::net`).
- **[REQ-0035-03]** (Unwanted): IF `uas-acp` is started, THEN `uas-acp` shall not bind a TCP socket.

## Plan

- [ ] (Conditional on D1 confirmation) Remove TCP claims from `crates/uas-acp/README.md`.
- [ ] (Conditional on D1 confirmation) Remove TCP claims from `crates/uas-acp/Cargo.toml`.

## Test

| Requirement | Negative Control |
|---|---|
| REQ-0035-02 | A source scan failing if `std::net` or `tokio::net` is found in `uas-acp`. Goes red if a TCP import is added. |
| REQ-0035-03 | A runtime check running `ss -ltn` during `uas-acp` test. Goes red if a TCP port is bound. |

```bash
! grep -rnE 'TcpListener|tokio::net|std::net' crates/uas-acp/src/
cargo test -p uas-acp
```
A test case starts the ACP server and asserts that `ss -ltn` shows no new listening TCP ports. The source scan asserts no networking imports exist. If a developer imports `std::net` to bind a TCP port, the source scan goes red. If the binding happens via FFI without imports, the runtime `ss` test goes red.

## Notes

- Defect: `crates/uas-acp/README.md` and `Cargo.toml` incorrectly claim TCP support.
- Fix: Strip "and TCP" from the README and Cargo manifest description.
