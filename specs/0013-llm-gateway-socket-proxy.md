---
status: complete
created: 2026-10-09
tags: [sandbox, router, proxy, unix-socket, gateway, phase-2]
priority: high
depends_on: [0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation, 0012-sandbox-escape-suite, 0004-dependency-and-licence-policy]
---

# 0013-llm-gateway-socket-proxy

## Overview

A worker in the jail (spec 0010) and the network namespace (spec 0011) can
reach no network. It still has to call models through the LiteLLM gateway
and send telemetry. This spec gives it one door: a Unix socket inside its
workspace, served by a proxy that lets only gateway calls through and adds
the gateway key itself (CER-2718, parent CER-2681).

The first draft came from the agy lane (`session-agy-ceres-estate-w25-p3`)
on 2026-10-09; its facts and filter rules are kept. Its runtime decision is
replaced by the operator's ruling below.

Facts, measured 2026-10-09 on ceres (kernel `6.6.87.2-microsoft-standard-WSL2`,
Landlock ABI 3, unprivileged):

- Spec 0012 row `uds_connect_out`: a jailed worker can `connect` to a
  pathname Unix socket under `$HOME`, with and without the network
  namespace. No Landlock ABI known to the crate mediates `connect` on a
  pathname socket (v6 scopes abstract sockets only), and `CLONE_NEWNET`
  isolates only abstract sockets. The proxy's allowlist is therefore the
  boundary, not the jail.
- The worker preset (spec 0010) grants every access beneath the workspace,
  so a socket at `<workspace>/.ipc/llm.sock` needs no new rule.
- Spec 0004 approves `tokio` for `uas-router`, `uas-deck` and
  `uas-storage`, not for `uas-sandbox`. `reqwest` with `rustls` needs cmake
  at build time.
- The public gateway host has been unstable since 2026-07 (OPS-603). The
  tailnet (`http://a-host.example`) and in-cluster
  (`http://llm.llm:4000`) hosts speak plain HTTP.

## Design

### Requirements (EARS)

- **Ubiquitous:** The `uas-router` socket proxy shall expose model gateway access via a Unix domain socket inside the worker workspace.
- **Ubiquitous:** The proxy shall inject the gateway bearer token while stripping incoming authorization headers from the worker.
- **Event-driven:** WHEN a worker sends an HTTP request, the proxy shall validate the target against the route allowlist.
- **State-driven:** WHILE handling proxy connections, the router shall stream bodies under strict size and duration limits.
- **Unwanted behavior:** IF a request contains unapproved paths or malformed headers, THEN the proxy shall reject the request immediately.

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | The proxy lives in `uas-router`, on `tokio`. `uas-sandbox` keeps only the socket mount: the `.ipc` directory and the path convention. Layering holds: sandbox sits above router. | the operator, 2026-10-09, by interview in the UAS lane |
| D2 | Upstream is plain HTTP to the tailnet or in-cluster gateway. No `reqwest`, no TLS stack now. | the operator, 2026-10-09, by interview in the UAS lane |
| D3 | The socket is `<workspace>/.ipc/llm.sock`: a path inside the workspace, not a bind mount. `uas_sandbox::ipc::ensure_ipc_dir` creates `.ipc` with mode `0700` and refuses a symlink in its place. The proxy binds and cleans up only when `.ipc` is a real directory and the path holds nothing or a socket, so a worker cannot redirect the host's `remove_file`. | agy lane draft, kept; UAS lane, 2026-10-09, after a critic pass |
| D4 | Allowlist: `POST` to exactly `/v1/chat/completions` or `/v1/embeddings`; `GET` to `/v1/models` or `/v1/models/<one segment>`. The ticket's telemetry egress has no receiver yet (no gateway path for it is known; not measured), so it is not on the default list; the pool adds a path when one exists. A query string is allowed. Everything else is `403`: other methods, other paths, `..` segments, `//`, percent-encoded `.`, `/` or `\`, malformed percent-encoding, characters outside the URI set, and non-origin-form targets (so `CONNECT` and absolute URLs). | agy lane draft, narrowed after a critic pass; UAS lane, 2026-10-09 |
| D5 | The worker never holds the key. The proxy takes it from a file outside the workspace at start (`with_key_file`), or from the caller that already holds it (`with_bearer_token`). It drops any `Authorization` and every `x-litellm-*` header the worker sent, and adds its own `Authorization`. `Debug` on the config redacts it. | agy lane draft, kept; UAS lane, 2026-10-09 |
| D6 | One request per connection, `Connection: close` on both sides, no half-close toward the upstream. The head is read through a reader capped at 16 KiB (`431`), so a line without a newline cannot grow memory. Bodies need one `Content-Length` of digits only; conflicting values, signs or non-digits are `400`; `Transfer-Encoding` is `411`. Header names must be tokens; control bytes, bare `CR`, non-ASCII and obsolete line folding are `400`. Hop-by-hop headers and `Host` are dropped; `Host` is rewritten; `Content-Length` is re-emitted once, normalized. | UAS lane, 2026-10-09 |
| D7 | `tokio` enters the workspace with the approved features plus `io-util`, a feature of the same crate that `AsyncReadExt` and `AsyncWriteExt` need. Recorded in spec 0004 as a lane amendment. | UAS lane, 2026-10-09 |
| D8 | Limits, all configurable: 64 live connections (`503` beyond), 16 MiB declared body (`413` beyond), 10 s to send the head (`408`), 5 s to connect upstream (`502`), 120 s for the whole exchange (both sides closed). A transient accept error backs off 50 ms and never ends the listener. A short body gets `400` and the upstream connection is closed. | UAS lane, 2026-10-09, after a critic pass |

the operator corrects any of these in place.

### Shape

```text
jailed worker --HTTP/1.1--> <ws>/.ipc/llm.sock --uas-router::uds_proxy--> TCP http://<gateway>
                              (uas-sandbox::ipc)      allowlist, key, Host rewrite
```

`uas_router::ProxyHandle::start(config)` binds the socket, runs the
listener on its own runtime thread, and on drop or `shutdown()` stops the
listener, joins the thread and removes the socket file. `ProxyConfig`
carries the socket path, the upstream `SocketAddr`, the `Host` value, the
allowlist and the token (`with_key_file`).

The pool (phase 3) wires it: create the workspace, `ensure_ipc_dir`, start
the proxy with the key file, spawn the worker through `netns::isolate` and
`launch`. This spec ships the two halves and the gate that joins them.

### Errors

`uas_router::ProxyError`: `Bind`, `KeyFile`, `Runtime`. A rejected request
gets an HTTP status and a one-line reason that never carries upstream data
or the key: `400`, `403`, `408`, `411`, `413`, `431`, `503`, `505`, and
`502` when the upstream cannot be reached or answers nothing. Once any
byte of an upstream response has reached the worker, a later failure only
closes the connection; no second response is written. Upstream responses
are passed through unfiltered; a gateway error body is the gateway's own.

### Out of scope

Prefix-hash routing (spec 0001, phase 4). TLS to the public host. Response
filtering. Rate limits. The pool's wiring.

## Plan

- [x] `uas_sandbox::ipc` with `socket_path` and `ensure_ipc_dir`.
- [x] `uas_router::uds_proxy` with `ProxyConfig`, `ProxyHandle`, the parser, the allowlist and the forwarder; unit tests for the filter and the redaction.
- [x] Spec 0012 row `uds_connect_out` and the gap note updated with the measurement.
- [x] Integration test `uds_proxy` in `uas-sandbox`: a jailed worker (in the namespace where the host allows it) gets `200` through the socket with the key injected and the `Host` rewritten; `CONNECT`, `/admin/keys`, `/etc/passwd`, a `..` path, an encoded `..` and `PUT` get `403`; a 20 KiB head gets `431`; conflicting `Content-Length` gets `400`; a stalled body is cut without a response and the next request still works; the worker cannot read the key file; the socket file is gone after shutdown.
- [x] `tokio` in the workspace; spec 0004 note; CHANGELOG line; `specs/README.md` row.

## Test

```bash
cargo test -p uas-sandbox -j 2 --test uds_proxy
cargo test -p uas-router -j 2
```

Passing means exit 0 for both. The gate uses a fake gateway on
`127.0.0.1` that echoes the `Host` and `Authorization` it received, so no
real gateway is called. On a host without Landlock the gate prints
`skipped` and passes. Add `-- --nocapture` to see one line per check. Also
run `make check`, `make hygiene` and `make smoke`.

## Notes

Alternative for D1, from the agy draft: `std::thread` and blocking I/O in
`uas-sandbox`. Rejected by the operator's ruling; the proxy is model-call
egress, which is the router's job.

Alternative for D6: chunked request bodies. Not needed by the OpenAI-style
clients the estate uses; they send `Content-Length`. Add it when a client
needs it.

References:

- `specs/0004-dependency-and-licence-policy.md` (tokio row, reqwest row)
- `<home>/.agents/sops/blast-radius-and-estate-ops.md` (gateway hosts and keys)
- RFC 9112 (HTTP/1.1 message syntax), hop-by-hop headers
