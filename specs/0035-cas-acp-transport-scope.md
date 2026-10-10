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

Addresses the contradiction between `crates/cas-acp/README.md` (which claims TCP support [verified, `grep -in "TCP" crates/cas-acp/README.md`]) and Spec 0016 (which strictly defines stdio [verified, `grep -cin tcp specs/0016*.md` returning 0]). Determines the transport scope for `cas-acp`.

## Design

### Decisions

| ID | Decision | Source |
|---|---|---|
| D1 | `cas-acp` does not get a TCP transport; it remains strictly stdio over JSON-RPC 2.0. | proposed: lane w10:pN, 2026-10-10; awaits integrating lane or the operator |

Reasoning: Spec 0016 D2 pins ACP 2.2.0 with no custom framing. TCP without custom framing (like TLS-PSK or token headers) provides no authentication mechanism, exposing the JSON-RPC interface. A Unix domain socket could be used (per Spec 0013 convention) but stdio is sufficient for current Zed IDE integration. Spec 0011 isolates network namespaces restricting outbound networking. Thus, TCP is rejected. This removes an advertised capability, so it awaits integrating lane or the operator's confirmation before the README and manifest change.

### Requirements (EARS)

- **[REQ-0035-01]** (Ubiquitous): The `cas-acp` crate shall exclusively use stdio for its JSON-RPC 2.0 transport.
- **[REQ-0035-02]** (Unwanted): IF `cas-acp` is started, THEN `cas-acp` shall not bind a TCP socket.

## Plan

- [ ] (Conditional on D1 confirmation) Remove TCP claims from `crates/cas-acp/README.md`.
- [ ] (Conditional on D1 confirmation) Remove TCP claims from `crates/cas-acp/Cargo.toml`.

## Test

- **Verified claim:** `grep -cin tcp specs/0016*.md` returns 0, proving Spec 0016 does not define TCP.
- **Test case:**
```bash
cargo test -p cas-acp
```
A test case starts the ACP server and asserts that `ss -ltn` shows no new listening TCP ports. If the binding restriction in `cas-acp` is deleted and it attempts to bind TCP, this test goes red.

## Notes

- Defect: `crates/cas-acp/README.md` and `Cargo.toml` incorrectly claim TCP support.
- Fix: Strip "and TCP" from the README and Cargo manifest description.
