---
status: planned
created: 2026-10-10
tags: [sandbox, router, reverie, memory, unix-socket, phase-2]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0008-soma-cortex-saga-reverie-integration, 0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation, 0013-llm-gateway-socket-proxy]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0032-reverie-access-from-a-jailed-slot

> **Status**: planned - **Priority**: high - **Created**: 2026-10-10

## Overview

Spec 0008 specifies that the UAS substrate queries context and records attestations by connecting to Reverie at `http://127.0.0.1:7437` [reported: spec 0008 line 66]. However, Spec 0011 isolates worker processes inside private network namespaces (`CLONE_NEWNET`) where the kernel assigns `127.0.0.1/8` to the slot's private loopback interface and denies outbound routing [reported: spec 0011 lines 35-37]. Inside a jailed slot with loopback active, `http://127.0.0.1:7437` queries the slot's isolated loopback and fails immediately with connection refused [verified: `unshare -Urn sh -c "ip link set lo up && curl -s --connect-timeout 1 http://127.0.0.1:7437"` exits 7]. A jailed worker cannot read prior memory or persist execution attestations as specified by Spec 0008 [reported: integrating lane w25:p1 gap analysis].

This specification resolves the contradiction between Spec 0008 and Spec 0011. Following the architectural pattern established in Spec 0013 for the LLM gateway [reported: spec 0013 D1-D5], UAS exposes Reverie access via a Unix domain socket located inside the worker workspace at `.ipc/reverie.sock`. A socket proxy running in `uas-router` forwards authorized requests across the namespace boundary to the host daemon at `127.0.0.1:7437` under a strict method, route, and project scoping allowlist.

Facts, measured 2026-10-10 on ceres:

- **F1 [reported]:** Spec 0008 line 66 requires connecting to `http://127.0.0.1:7437` for context search (`GET /search`, `GET /context/smart`) and attestation persistence (`POST /observations`), designating Reverie at line 20 as "the persistent memory daemon (`127.0.0.1:7437`)".
- **F2 [reported]:** Spec 0011 isolates slots in a network namespace with only a loopback interface; outbound non-loopback connections fail with `ENETUNREACH` (lines 35-37).
- **F3 [verified]:** In a private network namespace with loopback brought up, connecting to `127.0.0.1:7437` fails with connection refused. Command: `unshare -Urn sh -c "ip link set lo up && curl -s --connect-timeout 1 http://127.0.0.1:7437"` exits 7.
- **F4 [reported]:** Spec 0012 row `uds_connect_out` establishes that Landlock and `CLONE_NEWNET` do not mediate connect on pathname Unix domain sockets. A jailed worker connects to a pathname socket inside its workspace [verified: `cargo test -p uas-sandbox -j 2 --test uds_proxy`].
- **F5 [verified]:** `uas-domain` contains zero I/O imports (`std::net`, `std::fs`, `tokio`, `std::process`). Command: `make smoke JOBS=2`. Layering defined in Spec 0001 dictates `deck > acp > engine > pool > sandbox, ebpf > router > domain`.
- **F6 [reported]:** Spec 0013 established the egress proxy pattern in `uas-router` using `tokio`, exposing `<workspace>/.ipc/llm.sock` with strict path allowlisting, header stripping, and credential injection.
- **F7 [reported]:** Spec 0029 (`specs/0029-agent-identity-and-on-behalf-of-delegation.md`) is unratified and sits in the Director of Engineering's identity veto area; delegation headers must be conditional.
- **F8 [reported]:** Spec 0023 (`specs/0023-test-time-compute-and-verifiable-reward.md`) relies on Reverie memory to persist rollout trajectories and verifiable reward receipts; jailed test-time compute slots require socket transport to emit these records.
- **F9 [verified]:** Probing the live Reverie daemon on `127.0.0.1:7437` confirms that `/health` returns HTTP 200, `/healthz` returns HTTP 404, and `GET /observations/13347` returns HTTP 200, proving `/observations/<id>` is a valid route and distinguishing a missing route from a nonexistent ID. Commands: `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7437/health` and `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7437/observations/13347`.

## Design

The Reverie proxy lives in `uas-router` on `tokio` (Layer 2). Layering holds: `uas-sandbox` sits above `uas-router` per Spec 0013 D1, and `uas-domain` remains zero I/O [reported: spec 0001, spec 0013; verified: `make smoke JOBS=2`]. The sandbox manages only the filesystem convention: `uas_sandbox::ipc::ensure_ipc_dir` creates `<workspace>/.ipc` with mode `0700` and defines the socket filename `reverie.sock` [reported: spec 0013 D3].

Because Landlock does not mediate connect on pathname sockets and `CLONE_NEWNET` isolates only network interfaces [reported: spec 0012 row `uds_connect_out`], the proxy's route, method, and tenant scoping allowlist is the security boundary [asserted: kernel pathname socket semantics]. The proxy permits semantic context reading and observation recording scoped strictly to the slot's assigned project, while rejecting memory deletion, administrative endpoints, cross-project data access, and path traversal attempts.

### Decisions

| # | Decision | Rationale | Source |
|---|---|---|---|
| D1 | The Reverie proxy lives in `uas-router`, on `tokio`. `uas-sandbox` owns only the socket path convention and directory mode `0700`. Layering holds: sandbox sits above router per Spec 0013 D1, and domain remains zero I/O. | Architecture layering rule, Spec 0001; Spec 0013 D1; UAS spec lane w1X:p1, 2026-10-10 |
| D2 | The socket path is `<workspace>/.ipc/reverie.sock`. `uas_sandbox::ipc::ensure_ipc_dir` creates `.ipc` with mode `0700` and refuses a symlink in its place. `uas-router` binds and unlinks the socket file. | Spec 0013 precedent; UAS spec lane w1X:p1, 2026-10-10 |
| D3 | Route allowlist: `GET` to `/search`, `/context/smart`, `/observations/*`, `/tree`, `/health`; `POST` to `/observations` and `/sessions/*`. All other methods (including `DELETE`), administrative endpoints, `..` path segments, and absolute URIs return HTTP 403 Forbidden. Inherits request hardening and framing from Spec 0013 D4 and D6. | Reverie daemon API surface verified on 127.0.0.1:7437; Spec 0013 D4, D6; UAS spec lane w1X:p1, 2026-10-10 |
| D4 | Strict non-hanging timeouts: 500 ms upstream connect timeout (overriding Spec 0013 D8's 5 s for local daemon), 2 s head timeout, 10 s exchange timeout. Refused upstream connections return HTTP 502 Bad Gateway; upstream connect timeouts return HTTP 504 Gateway Timeout. Slot context reads fail open with empty context. | Slot resilience invariant; UAS spec lane w1X:p1, 2026-10-10 |
| D5 | Delegation assertion propagation is conditional upon Spec 0029 ratification. When Spec 0029 is active, the proxy forwards validated `on-behalf-of` delegation headers; otherwise requests pass through without delegation metadata. | The Director of Engineering identity veto rule, reported by integrating lane w25:p1, 2026-10-10 |
| D6 | Reuses `uas_router::uds_proxy` core HTTP parser, connection limiter, and socket mechanics, but extends it with prefix path matching (for `GET /observations/*` and `POST /sessions/*`) rather than exact matching alone. Bearer token injection (Spec 0013 D5) is disabled (OFF) for Reverie. | Architecture reuse; UAS spec lane w1X:p1, 2026-10-10 |
| D7 | Project scoping: The proxy is configured with the slot's assigned `project`. The proxy enforces that `GET /search` and `GET /context/smart` queries specify `project=<assigned>` (rejecting missing or mismatched values with HTTP 403 Forbidden), and validates that `POST /observations` bodies declare matching `project`. This prevents cross-tenant memory reads and poisoning. | Multi-tenant isolation invariant; UAS spec lane w1X:p1, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0032-01]** (Ubiquitous): The `uas-router` shall expose a Unix domain socket proxy at `<workspace>/.ipc/reverie.sock` for jailed slot memory access.
- **[REQ-0032-02]** (Event-driven): WHEN a jailed worker initiates an HTTP request over the socket, the `uas-router` proxy shall validate the HTTP method and path against the Reverie route allowlist.
- **[REQ-0032-03]** (Unwanted): IF a request specifies an unapproved HTTP method, an unapproved path, or path traversal sequences, THEN the `uas-router` proxy shall reject the request with HTTP 403 Forbidden without contacting upstream, per Spec 0013 D4.
- **[REQ-0032-04]** (Ubiquitous): The `uas-router` proxy shall permit `GET` requests strictly to `/search`, `/context/smart`, `/observations/*`, `/tree`, and `/health`.
- **[REQ-0032-05]** (Ubiquitous): The `uas-router` proxy shall permit `POST` requests strictly to `/observations` and `/sessions/*` using path prefix matching.
- **[REQ-0032-06]** (Unwanted): IF an incoming request targets administrative endpoints or specifies the `DELETE` method, THEN the `uas-router` proxy shall reject the request with HTTP 403 Forbidden.
- **[REQ-0032-07]** (State-driven): WHILE forwarding allowed requests to the upstream Reverie daemon on `127.0.0.1:7437`, the `uas-router` proxy shall enforce a 500-millisecond upstream connect timeout and a 10-second exchange deadline without injecting bearer tokens.
- **[REQ-0032-08]** (Unwanted): IF the upstream Reverie daemon connection is refused or closed prematurely, THEN the `uas-router` proxy shall return HTTP 502 Bad Gateway without terminating the worker process.
- **[REQ-0032-09]** (Unwanted): IF establishing the upstream Reverie connection exceeds 500 milliseconds or the exchange exceeds 10 seconds, THEN the `uas-router` proxy shall return HTTP 504 Gateway Timeout without terminating the worker process.
- **[REQ-0032-10]** (Optional): WHERE Spec 0029 agent delegation is active, the `uas-router` proxy shall pass verified `on-behalf-of` delegation headers through to upstream observation writes.
- **[REQ-0032-11]** (Ubiquitous): The `uas-sandbox::ipc` shall provision the `<workspace>/.ipc` directory with mode `0700` and provide the path constructor for `reverie.sock`.
- **[REQ-0032-12]** (Unwanted): IF a `GET` request to `/search` or `/context/smart` omits the `project` parameter or specifies a project differing from the slot's assigned project, THEN the `uas-router` proxy shall reject the request with HTTP 403 Forbidden.
- **[REQ-0032-13]** (Unwanted): IF a `POST /observations` payload specifies a `project` differing from the slot's assigned project, THEN the `uas-router` proxy shall reject the write with HTTP 403 Forbidden.

## Plan

- [ ] Add `uas_sandbox::ipc::reverie_socket_path` and `REVERIE_SOCKET` constant in `uas-sandbox`.
- [ ] Extend `uas_router::uds_proxy` to support prefix matching for `POST` paths (`/sessions/*`) and segmented `GET` paths (`/observations/*`).
- [ ] Configure the Reverie proxy instance in `uas-router` to disable bearer token injection (Spec 0013 D5 OFF) and apply a 500 ms upstream connect timeout (overriding Spec 0013 D8).
- [ ] Implement project scoping validation in `uas-router::uds_proxy` for `/search`, `/context/smart`, and `/observations`.
- [ ] Implement upstream connect timeout (500 ms) error synthesis returning HTTP 504 Gateway Timeout, and connection refusal returning HTTP 502 Bad Gateway.
- [ ] Add unit tests in `uas-router` for route allowlisting, scoping violations, 502 connection refusal, and 504 timeouts.
- [ ] Add integration test in `uas-sandbox` verifying that a jailed worker in a private network namespace accesses Reverie only through `<workspace>/.ipc/reverie.sock`, requiring the test to fail explicitly if netns cannot be instantiated.

## Test

```bash
cargo test -p uas-router -j 2 --test reverie_proxy && \
  cargo test -p uas-sandbox -j 2 --test reverie_socket_integration
```

A passing suite verifies that a sandboxed process inside a private network namespace connects to `<workspace>/.ipc/reverie.sock`, reads context, and records observations through the proxy (REQ-0032-01, REQ-0032-02, REQ-0032-04, REQ-0032-05, REQ-0032-11). It also verifies error synthesis and project scoping (REQ-0032-08, REQ-0032-09, REQ-0032-12, REQ-0032-13).

A pass requires each of these negative control cases to exist and to go red when its own check is deleted:

| Case | Where | Requirement |
|---|---|---|
| request with `DELETE /observations/1` returns HTTP 403 | `uas-router` unit test | REQ-0032-03, REQ-0032-06 |
| request with path traversal `/search/../../etc/passwd` returns HTTP 403 | `uas-router` unit test | REQ-0032-03 |
| request to administrative endpoint `/admin/reset` returns HTTP 403 | `uas-router` unit test | REQ-0032-06 |
| request to `/search` with mismatched project parameter returns HTTP 403 | `uas-router` unit test | REQ-0032-12 |
| `POST /observations` with mismatched project field returns HTTP 403 | `uas-router` unit test | REQ-0032-13 |
| refused connection to absent upstream returns HTTP 502 immediately | `uas-router` unit test | REQ-0032-08 |
| stalled upstream connection exceeding 500 ms returns HTTP 504 | `uas-router` unit test | REQ-0032-07, REQ-0032-09 |
| request with delegation header fails without `--features delegation` | `uas-router` unit test | REQ-0032-10 |
| `ensure_ipc_dir` refuses existing symlink with `SandboxError::PathOpen` | `uas-sandbox` unit test | REQ-0032-11 |
| jailed worker direct TCP connect to `127.0.0.1:7437` fails | `uas-sandbox` integration test | REQ-0032-01 |

Test environment rule: In the `uas-sandbox` integration test, if the environment lacks unprivileged namespace capabilities (`CLONE_NEWUSER`/`CLONE_NEWNET`), the test shall fail with an explicit diagnostic rather than skipping silently, because a skipped test is indistinguishable from a pass in CI nextest output.

The primary negative control: if the route allowlist check in `uas-router` is removed, the test asserting HTTP 403 on `DELETE /observations/1` must go red.

## Notes

- Contradiction resolution: This spec reconciles Spec 0008 (which assumed direct loopback access to `127.0.0.1:7437`) with Spec 0011 (which removed host loopback access via `CLONE_NEWNET`) [reported].
- Route verification: The route `GET /observations/*` was probed against existing record ID 13347 on the live daemon (returning HTTP 200), resolving the ambiguity between missing route (HTTP 404 on `/healthz`) and missing record (HTTP 404 on `/observations/1`) [verified: `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7437/observations/13347`].
- Layering preservation: `uas-sandbox` sits above `uas-router` per Spec 0013 D1, while `uas-domain` remains zero I/O [reported: spec 0001, spec 0013; verified: `make smoke JOBS=2`]. Socket proxying and HTTP handling reside in `uas-router`, while `uas-sandbox` restricts itself to IPC directory creation and path resolution.
- Cross-spec linkage with Spec 0023: Spec 0023 specifies test-time compute scaling and verifiable rewards, persisting rollout trajectories into Reverie memory [reported: spec 0023 F1, F2]. Spec 0032 provides the underlying socket transport enabling jailed workers to emit these trajectory records.
- Spec 0029 conditional status: Spec 0029 is unratified and subject to the Director of Engineering's identity veto [reported]. In accordance with D5, delegation header forwarding is specified as conditional rather than a hard dependency.
