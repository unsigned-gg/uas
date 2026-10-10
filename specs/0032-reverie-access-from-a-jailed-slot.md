---
status: planned
created: 2026-10-10
tags: [sandbox, router, reverie, memory, unix-socket, phase-2]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0008-soma-cortex-saga-reverie-integration, 0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation, 0013-llm-gateway-socket-proxy]
---

# 0032-reverie-access-from-a-jailed-slot

> **Status**: planned - **Priority**: high - **Created**: 2026-10-10

## Overview

Spec 0008 specifies that the CAS substrate queries context and records attestations by connecting to Reverie at `http://127.0.0.1:7437` [reported: spec 0008 line 66]. However, Spec 0011 isolates worker processes inside private network namespaces (`CLONE_NEWNET`) where the kernel assigns `127.0.0.1/8` to the slot's private loopback interface and denies outbound routing [verified: `cargo test -p cas-sandbox -j 2 --test netns`]. Inside a jailed slot, `http://127.0.0.1:7437` queries the slot's empty loopback and fails immediately [verified: `unshare -Urn curl -s --connect-timeout 1 http://127.0.0.1:7437` exits 7]. A jailed worker cannot read prior memory or persist execution attestations as specified by Spec 0008 [reported: integrating lane w25:p1 gap analysis].

This specification resolves the contradiction between Spec 0008 and Spec 0011. Following the architectural pattern established in Spec 0013 for the LLM gateway [reported: spec 0013 D1-D5], CAS exposes Reverie access via a Unix domain socket located inside the worker workspace at `.ipc/reverie.sock`. A socket proxy running in `cas-router` forwards authorized requests across the namespace boundary to the host daemon at `127.0.0.1:7437` under a strict method and route allowlist.

Facts, measured 2026-10-10 on ceres:

- **F1 [reported]:** Spec 0008 line 66 requires connecting to `http://127.0.0.1:7437` for context search (`GET /search`, `GET /context/smart`) and attestation persistence (`POST /observations`), designating Reverie at line 20 as "the persistent memory daemon (`127.0.0.1:7437`)".
- **F2 [verified]:** Spec 0011 isolates slots in a network namespace with only a loopback interface; outbound connections fail with `ENETUNREACH`. Command: `cargo test -p cas-sandbox -j 2 --test netns`.
- **F3 [verified]:** In a private network namespace, connecting to `127.0.0.1:7437` fails immediately with connection refused. Command: `unshare -Urn curl -s --connect-timeout 1 http://127.0.0.1:7437` exits 7.
- **F4 [verified]:** A process inside a Landlock jail and network namespace can connect to a pathname Unix domain socket inside its workspace without Landlock mediation or namespace restriction. Command: `cargo test -p cas-sandbox -j 2 --test uds_proxy` (Spec 0012 row `uds_connect_out`).
- **F5 [verified]:** `cas-domain` contains zero I/O imports (`std::net`, `std::fs`, `tokio`, `std::process`). Command: `make smoke JOBS=2`. Layering defined in Spec 0001 dictates `deck > acp > engine > pool > sandbox, ebpf > router > domain`.
- **F6 [reported]:** Spec 0013 established the egress proxy pattern in `cas-router` using `tokio`, exposing `<workspace>/.ipc/llm.sock` with strict path allowlisting, header stripping, and credential injection.
- **F7 [reported]:** Spec 0029 (`specs/0029-agent-identity-and-on-behalf-of-delegation.md`) is unratified and sits in the Director of Engineering's identity veto area; delegation headers must be conditional.
- **F8 [reported]:** Spec 0023 (`specs/0023-test-time-compute-and-verifiable-reward.md`) relies on Reverie memory to persist rollout trajectories and verifiable reward receipts; jailed test-time compute slots require socket transport to emit these records.
- **F9 [verified]:** Probing the live Reverie daemon on `127.0.0.1:7437` confirms that `/health` returns HTTP 200, `/healthz` returns HTTP 404, and `GET /observations/13347` returns HTTP 200, proving `/observations/<id>` is a valid route and distinguishing a missing route from a nonexistent ID. Commands: `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7437/health` and `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7437/observations/13347`.

## Design

The Reverie proxy lives in `cas-router` on `tokio` (Layer 2), maintaining the layering rule that `cas-domain` remains zero I/O and `cas-sandbox` stays strictly below `cas-router` [reported: spec 0001; verified: `make smoke JOBS=2`]. The sandbox manages only the filesystem convention: `cas_sandbox::ipc::ensure_ipc_dir` creates `<workspace>/.ipc` with mode `0700` and defines the socket filename `reverie.sock` [reported: spec 0013 D3].

Because Landlock does not mediate connect on pathname sockets and `CLONE_NEWNET` isolates only network interfaces [verified: spec 0012 row `uds_connect_out`], the proxy's route and method allowlist is the security boundary [asserted: kernel pathname socket semantics]. The proxy permits semantic context reading and observation recording while rejecting memory deletion, administrative endpoints, and path traversal attempts.

### Decisions

| # | Decision | Rationale | Source |
|---|---|---|---|
| D1 | The Reverie proxy lives in `cas-router`, on `tokio`. `cas-sandbox` owns only the socket path convention and directory mode `0700`. Layering holds: `cas-domain` stays zero I/O, `cas-router` handles socket proxying and upstream TCP. | Architecture layering rule, Spec 0001; the operator, 2026-10-09, by interview in the CAS lane; adopted for Reverie by CAS spec lane w1X:p1, 2026-10-10 |
| D2 | The socket path is `<workspace>/.ipc/reverie.sock`. `cas_sandbox::ipc::ensure_ipc_dir` creates `.ipc` with mode `0700` and refuses a symlink in its place. `cas-router` binds and unlinks the socket file. | Spec 0013 precedent; CAS spec lane w1X:p1, 2026-10-10 |
| D3 | Route allowlist: `GET` to `/search`, `/context/smart`, `/observations/*`, `/tree`, `/health`; `POST` to `/observations` and `/sessions/*`. A query string is allowed on `GET`. All other methods (including `DELETE`), administrative endpoints, `..` path segments, and absolute URIs return HTTP 403 Forbidden. | Reverie daemon API surface verified on 127.0.0.1:7437; CAS spec lane w1X:p1, 2026-10-10 |
| D4 | Strict non-hanging timeouts: 500 ms upstream connect timeout, 2 s head timeout, 10 s total exchange timeout. Upstream absence or failure returns HTTP 502 Bad Gateway or 504 Gateway Timeout without killing the slot. Slot context reads fail open with empty context. | Slot resilience invariant; CAS spec lane w1X:p1, 2026-10-10 |
| D5 | Delegation assertion propagation is conditional upon Spec 0029 ratification. When Spec 0029 is active, the proxy forwards validated `on-behalf-of` delegation headers; otherwise requests pass through without delegation metadata. | the Director of Engineering identity veto rule, reported by integrating lane w25:p1, 2026-10-10 |
| D6 | Reuses `cas_router::uds_proxy` core HTTP parser, connection limiter, and forwarding mechanics, adding a dedicated Reverie configuration preset rather than introducing a separate proxy crate or duplicate HTTP engine. | DRY and binary footprint minimization; CAS spec lane w1X:p1, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0032-01]** (Ubiquitous): The `cas-router` shall expose a Unix domain socket proxy at `<workspace>/.ipc/reverie.sock` for jailed slot memory access.
- **[REQ-0032-02]** (Event-driven): WHEN a jailed worker initiates an HTTP request over the socket, the `cas-router` proxy shall validate the HTTP method and path against the Reverie route allowlist.
- **[REQ-0032-03]** (Unwanted): IF a request specifies an unapproved HTTP method, an unapproved path, or path traversal sequences, THEN the `cas-router` proxy shall reject the request with HTTP 403 Forbidden without contacting upstream.
- **[REQ-0032-04]** (Ubiquitous): The `cas-router` proxy shall permit `GET` requests strictly to `/search`, `/context/smart`, `/observations/*`, `/tree`, and `/health`.
- **[REQ-0032-05]** (Ubiquitous): The `cas-router` proxy shall permit `POST` requests strictly to `/observations` and `/sessions/*`.
- **[REQ-0032-06]** (Unwanted): IF an incoming request targets administrative endpoints or specifies the `DELETE` method, THEN the `cas-router` proxy shall reject the request with HTTP 403 Forbidden.
- **[REQ-0032-07]** (State-driven): WHILE forwarding allowed requests to the upstream Reverie daemon on `127.0.0.1:7437`, the `cas-router` proxy shall enforce a 500-millisecond upstream connection timeout and a 10-second exchange deadline.
- **[REQ-0032-08]** (Unwanted): IF the upstream Reverie daemon is unreachable or times out, THEN the `cas-router` proxy shall return HTTP 502 Bad Gateway or HTTP 504 Gateway Timeout without terminating the worker process.
- **[REQ-0032-09]** (Optional): WHERE Spec 0029 agent delegation is active, the `cas-router` proxy shall pass verified `on-behalf-of` delegation headers through to upstream observation writes.
- **[REQ-0032-10]** (Ubiquitous): The `cas-sandbox::ipc` shall provision the `<workspace>/.ipc` directory with mode `0700` and provide the path constructor for `reverie.sock`.

## Plan

- [ ] Add `cas_sandbox::ipc::reverie_socket_path` and `REVERIE_SOCKET` constant in `cas-sandbox`.
- [ ] Implement Reverie proxy configuration and route allowlist validation in `cas-router::uds_proxy`.
- [ ] Implement upstream connect timeouts (500 ms) and error response synthesis (502/504) in `cas-router`.
- [ ] Add integration test in `cas-sandbox` verifying that a jailed worker in a private network namespace can query context and write observations over `<workspace>/.ipc/reverie.sock`.
- [ ] Verify that disallowed routes (`DELETE`, admin paths, path traversal) return HTTP 403 Forbidden.

## Test

```bash
cargo test -p cas-router -j 2 --test reverie_proxy && \
  cargo test -p cas-sandbox -j 2 --test reverie_socket_integration
```

A passing suite verifies that a sandboxed process inside a private network namespace connects to `<workspace>/.ipc/reverie.sock`, reads context, and records observations through the proxy (REQ-0032-01, REQ-0032-02, REQ-0032-04, REQ-0032-05, REQ-0032-10). It also verifies that unreachable upstreams yield HTTP 502 without hanging (REQ-0032-07, REQ-0032-08).

A pass requires each of these negative control cases to exist and to go red when its own check is deleted:

| Case | Where | Requirement |
|---|---|---|
| request with `DELETE /observations/1` returns HTTP 403 | `cas-router` unit test | REQ-0032-03, REQ-0032-06 |
| request with path traversal `/search/../../etc/passwd` returns HTTP 403 | `cas-router` unit test | REQ-0032-03 |
| request to administrative endpoint `/admin/reset` returns HTTP 403 | `cas-router` unit test | REQ-0032-06 |
| absent upstream daemon returns HTTP 502 within 500 ms | `cas-router` unit test | REQ-0032-07, REQ-0032-08 |
| jailed worker direct TCP connect to `127.0.0.1:7437` fails | `cas-sandbox` integration test | REQ-0032-01 |

The primary negative control: if the route allowlist check in `cas-router` is removed, the test asserting HTTP 403 on `DELETE /observations/1` must go red.

## Notes

- Contradiction resolution: This spec reconciles Spec 0008 (which assumed direct loopback access to `127.0.0.1:7437`) with Spec 0011 (which removed host loopback access via `CLONE_NEWNET`) [reported].
- Route verification: The route `GET /observations/*` was probed against existing record ID 13347 on the live daemon (returning HTTP 200), resolving the ambiguity between missing route (HTTP 404 on `/healthz`) and missing record (HTTP 404 on `/observations/1`) [verified: `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7437/observations/13347`].
- Layering preservation: `cas-domain` remains zero I/O [verified: `make smoke JOBS=2`]. Socket proxying and HTTP handling reside in `cas-router`, while `cas-sandbox` restricts itself to IPC directory creation and path resolution [reported: spec 0001].
- Cross-spec linkage with Spec 0023: Spec 0023 specifies test-time compute scaling and verifiable rewards, persisting rollout trajectories into Reverie memory [reported: spec 0023 F1, F2]. Spec 0032 provides the underlying socket transport enabling jailed workers to emit these trajectory records.
- Spec 0029 conditional status: Spec 0029 is unratified and subject to the Director of Engineering's identity veto [reported]. In accordance with D5, delegation header forwarding is specified as conditional rather than a hard dependency.
