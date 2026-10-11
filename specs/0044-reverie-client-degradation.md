---
status: planned
created: 2026-10-10
tags: [engine, reverie, memory, degradation, outbox, http]
priority: high
depends_on: [0004-dependency-and-licence-policy, 0006-cas-engine-and-harness-dogfood, 0008-soma-cortex-saga-reverie-integration, 0009-saga-compensation-runner-and-durability, 0032-reverie-access-from-a-jailed-slot, 0033-cargo-feature-policy-and-ci-matrix, 0042-cas-binary-conventions]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0044-reverie-client-degradation

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

Spec 0008 has the substrate read context from reverie and write attestations to it, at
`127.0.0.1:7437` from the host and through the spec 0032 proxy from a slot. Spec 0032 says
what the slot side does when the daemon fails: it fails open with empty context. No spec says
what the host-side client does when `reveried` is down, slow or restarting [reported: research
lane w1X:p1, mesh 5871db01, ranked first; integrating lane w25:p1 gap analysis, 2026-10-10].
The host-side client does not exist yet: `uas-engine` has the port and an in-memory fake.

This spec adds the host-side HTTP client. It states the deadlines, the fail-open read path, a
write-behind outbox for attestations, the memory status every activity result carries, and how
the outbox survives a restart.

### Facts

Measured 2026-10-10 on ceres at development `6e7510e`.

- **F1 [verified]:** `ReverieClient` has two methods, `record` and `search`, both synchronous (`crates/uas-engine/src/reverie.rs` lines 40-46). The one implementation is `InMemoryReverieClient` (lines 48-91). Command: `grep -rn 'impl ReverieClient for' crates --include=*.rs` prints only line 66.
- **F2 [verified]:** Nothing calls the client. `grep -rn 'ReverieClient\|InMemoryReverieClient' crates --include=*.rs` prints only `reverie.rs` and the re-export at `lib.rs` line 86; `grep -rn reverie crates/uas-cli/` prints nothing. Spec 0008 lines 89 and 91 are checked `[x]` all the same.
- **F3 [verified]:** `uas-engine` has no HTTP client. Its dependencies are `crates/uas-engine/Cargo.toml` lines 12-24; `tokio` and `tonic` are optional, behind `cortex-grpc` (lines 19-24, 37-45). Commands: `cat -n crates/uas-engine/Cargo.toml`; `grep -nA2 '^name = "reqwest"' Cargo.lock` prints nothing, while the same pattern for `hyper` finds 1.12.0.
- **F4 [verified]:** Spec 0004 approves `reqwest` 0.13.5 for `uas-router` and `uas-deck` only, phases 4 and 5, with `rustls`, `http2`, `json` and `stream` (line 182). It approves `tokio` in `uas-engine` only behind `cortex-grpc` (line 180). No crate enters a `Cargo.toml` before its row is approved (lines 164-165). Command: `grep -nE 'reqwest|ureq|hyper|aws-sdk' specs/0004-*.md`.
- **F5 [verified]:** The blocking client of `reqwest` 0.13.5 has `timeout` and `connect_timeout` and no read or head timeout. Command: `grep -nE 'pub fn (read_timeout|connect_timeout|timeout)'` on the 0.13.5 registry source prints lines 388 and 399 for `src/blocking/client.rs`, and lines 1450, 1462 (`read_timeout`) and 1475 for `src/async_impl/client.rs`.
- **F6 [reported]:** The `reqwest` blocking client panics when called on an async runtime thread (`reqwest` 0.13.5 `src/blocking/mod.rs` lines 6-11). `uas-engine` runs `tokio` when `cortex-grpc` is on (F3).
- **F7 [verified]:** Spec 0032 D4 (line 44) sets a 500 ms connect, 2 s head and 10 s exchange deadline, and slot context reads fail open with empty context. D3 (line 43) allowlists `GET /health`, `GET /search` and `POST /observations`, among others. Command: `grep -nE 'D4|500 ms|timeout' specs/0032-*.md`.
- **F8 [verified]:** Spec 0008 line 49 and lines 65-67 route a slot through the spec 0032 proxy and the host to `127.0.0.1:7437` directly. Line 89 marks the adapter done. Command: `sed -n '49p;65,67p;89p' specs/0008-*.md`.
- **F9 [verified]:** `reveried` answers on the host, and answers an HTTP/1.0 `GET` with `content-length` and no chunked body. Commands: `curl -s -o /dev/null -w '%{http_code}' --connect-timeout 1 http://127.0.0.1:7437/health` printed `200` at 19:06 -04:00; `curl -s --http1.0 -D - -o /dev/null 'http://127.0.0.1:7437/search?q=uas&limit=1'` printed `HTTP/1.0 200 OK` and `content-length: 2135`. A `POST` over HTTP/1.0 was not probed, since it would write a row [asserted].
- **F10 [reported]:** `POST /observations` is idempotent on `event_id`: a repeat returns the same id and adds no row (`reverie@90e9497d:crates/reveried/tests/kreatorflow_idempotency.rs` lines 1-20).
- **F11 [verified]:** Spec 0033 D1-D4 (lines 58-61) put a feature in the crate that owns the code, off by default, kebab-case, with a comment naming its spec. D7 (line 64) fails `make hygiene` for a feature with no matrix leg. Commands: `grep -nE 'optional|feature|dep:' specs/0033-*.md`; `grep -nE '^features|feature' Makefile` prints nothing, so no `make features` target exists yet.
- **F12 [verified]:** Spec 0042 D1 (line 41) orders flag, then `UAS_` variable, then default, declared on the clap argument. The workspace `clap` has `derive` only (`Cargo.toml` line 33). `WaveEnv` reads `XDG_STATE_HOME` and `HOME` in `uas-cli` (`wave_flow.rs` lines 394-398). Commands: `grep -nE 'D1|precedence|env' specs/0042-*.md`; `grep -n '^clap' Cargo.toml`; `sed -n '390,400p' crates/uas-cli/src/wave_flow.rs`.
- **F13 [verified]:** `ActivityResult` (`activity.rs` lines 42-51) is serialized inside `WorkflowEvent::ActivityCompleted` (`event.rs` lines 35-39). Spec 0009 line 36 replays a session from its events without re-executing activities. `Attestation.at` is epoch seconds (`crates/uas-domain/src/attestation.rs` lines 69-70), and `topic_key` is a grouping key (`reverie.rs` line 20). Commands: `cat -n crates/uas-engine/src/activity.rs`; `sed -n '33,40p' crates/uas-engine/src/event.rs`; `grep -n replay specs/0009-*.md`; `grep -rn -B3 'pub at:' crates/uas-domain/src`.
- **F14 [verified]:** `cargo nextest run -p uas-engine reverie -j 2` runs 1 test today, `reverie::tests::record_and_search`, and skips 140.

## Design

The client lives in `uas-engine`, behind a cargo feature. A context read never fails an
activity: it returns empty context and marks the result `degraded`. An attestation write never
fails an activity either: when the daemon cannot take it, the record goes to an outbox on disk,
and a later call sends it. Each activity result says which of the four states held.

Four facts change the brief this spec was written from. `reqwest` is not approved for
`uas-engine` (F4). Its blocking client has no head deadline (F5) and, per its docs, panics on a
runtime thread (F6). Nothing calls the client (F2), so the Plan adds the call sites. The
brief's `event_id` key could collide (F13), so D9 uses a per-call id. D2 records the transport
choice for the operator.

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | `HttpReverieClient` lives in `uas-engine` modules `reverie::http` and `reverie::outbox`, behind feature `reverie-http`, declared with a comment naming this spec. `uas-cli` declares the same name and forwards it. Off by default: the default build keeps only `InMemoryReverieClient`, and the default `uas` makes no reverie call. | Spec 0033 D1-D4 (F11). | integrating lane w25:p1 brief D1, 2026-10-10 |
| D2 | Transport. A: `reqwest` 0.13.5, default features off, `blocking` and `json`, no TLS; needs a spec 0004 row amendment. B: an HTTP/1.0 client on `std::net::TcpStream`, `connect_timeout`, then `set_read_timeout` to the remaining budget before each read, `Connection: close`, body sized by `content-length`; no new dependency. This lane recommends B: it meets all three D3 deadlines, has no runtime hazard, and needs no approval. A misses the head deadline (F5) and adds the F6 hazard. Either way the trait stays synchronous; an async caller wraps calls in `spawn_blocking`. | F4, F5, F6, F9. | proposed: this lane, 2026-10-10; the operator to rule |
| D3 | Deadlines equal spec 0032 D4: 500 ms connect, 2 s to the response head, 10 s for the whole exchange. The health probe is `GET /health` with a 500 ms total deadline. Each deadline is absolute: every read gets the remaining budget, so a server that trickles bytes cannot extend it. The values sit in one `ReverieDeadlines` struct whose default is these numbers. | Host and slot then degrade alike (F7). | integrating lane w25:p1 brief D2, 2026-10-10 |
| D4 | Before an activity's context read, the client probes `/health`. If the probe fails, the client skips the search and returns empty context. If the probe passes and the search fails, times out or returns a non-2xx status, it returns empty context too. No request outlives its D3 deadline: a down daemon costs the 500 ms probe, a stalling one at most 10 s per request. A D6 drain adds one bounded request per record pending when it starts, and stops at its first failure. | Fail open, bounded (F7). | integrating lane w25:p1 brief D3, D5, 2026-10-10; per-request bound: this lane, after lane critic, mesh bffbfa99, 2026-10-10 |
| D5 | `ActivityResult` gains `memory: Option<MemoryStatus>` with `#[serde(default)]`. `MemoryStatus` is `OutboxRefused`, `Degraded`, `Outbox(n)` or `Live`, displayed `outbox(refused)`, `degraded`, `outbox(n)`, `live`. Precedence: `OutboxRefused` if D8 refused the outbox directory or the activity's outbox append failed; else `Degraded` if the probe or any reverie call of the activity failed; else `Outbox(n)` if n records are pending; else `Live`. `None` means no client was wired, and is what spec 0009 replay reads from older logs. | F13. A lost record outranks a degraded read. | integrating lane w25:p1 brief D5, 2026-10-10; `outbox(refused)`: lane critic, mesh bffbfa99, 2026-10-10 |
| D6 | Write-behind outbox. When `record` fails and D8 accepted the directory, the client appends one JSON line to `outbox.jsonl` under `${XDG_STATE_HOME:-$HOME/.local/state}/uas/reverie-outbox/`, fsyncs the file, and returns `Ok`. The directory is fsynced after the file is created and after every rename. While records are pending, every new record goes to the tail, never ahead of them. A record made while the activity is `Degraded` or the backoff runs goes straight to the outbox. No background thread runs. A drain sends the records pending when it starts, oldest first, and stops at its first failed send. Three events start a drain before their call returns: a `search` that `reveried` answers with a 2xx; a `record` call whose send `reveried` acknowledges, or that appends behind pending records; and a health probe that flips to live, which is the first passing probe of the process or one that passes after a failed probe. No drain starts while the activity is `Degraded` or a backoff runs. A sidecar `state.json` holds the byte offset of the first unsent line and the dropped count; it is rewritten by temp file, fsync and rename after each acknowledged record. Failed send attempts back off from 1 s, doubling, to a 30 s ceiling; a successful send resets it. | Ordered, durable, append-only. | integrating lane w25:p1 brief D4, 2026-10-10; sidecar and directory fsync: this lane; drain trigger: lane critic, mesh bffbfa99, 2026-10-10 |
| D7 | The outbox holds at most 10,000 pending lines or 50 MiB. An appender over either cap tries `drain.lock`; if it gets it, it drops the oldest pending records by advancing the offset in `state.json`, adds them to the dropped count, and logs at `warn` with the running total. If a drainer holds the lock, the appender appends anyway and the drainer enforces the cap before it releases the lock, so the cap is soft by at most one drain's appends and no appender waits. The dropped count holds only these cap drops on an accepted directory. Compaction rewrites the pending lines by temp file, fsync and rename. | Bounded disk, no silent loss, no blocking. | integrating lane w25:p1 brief D6, 2026-10-10; soft cap: this lane; count scope: lane critic, mesh bffbfa99, 2026-10-10 |
| D8 | The client creates the outbox directory `0700` if it is absent, then checks it once, when the client is built: owned by the process uid, mode `0700`, not a symlink; files `0600`. A directory that fails the check refuses the outbox for the life of the process, as spec 0036 D2 does: the client reads and writes nothing under it, `state.json` included, logs once at `warn`, and marks every activity result `outbox(refused)`. A record it cannot then deliver is lost and is not counted. On an accepted directory, an append or fsync that fails loses that one record: the client logs it at `error`, marks that result `outbox(refused)`, and counts nothing. `append.lock` is held with `File::lock` for one append and its fsync, and the appender opens `outbox.jsonl` by path under that lock every time, so a compaction rename by another process is never written past. `drain.lock` is taken with `File::try_lock`; a process that finds it held skips the drain. Lock order is fixed on every path: `append.lock`, then `drain.lock`. Compaction and truncation run under both, taken in that order. A drainer that must compact releases `drain.lock`, takes `append.lock`, tries `drain.lock` again, and skips the compaction if it misses. The outbox handle records when it holds `drain.lock`, and an `assert!` fails if it then asks for `append.lock`. | Several `uas` processes share one host (spec 0036). A foreign directory could feed records to `reveried`. | proposed: this lane, 2026-10-10; the operator to rule; refusal and lock order: lane critic, mesh bffbfa99, 2026-10-10 |
| D9 | Delivery is at least once. The client assigns each `record` call one `event_id`, `uas:<session_id>:<unix_nanos>:<pid>:<seq>` with `seq` a per-process counter, before the first send attempt, and carries it in the outbox line; resends reuse it, so `reveried` adds no row (F10). The id is built with `std` alone. A key of session, `topic_key` and `at` is rejected: `at` is seconds and `topic_key` groups activities (F13), so two records could share an id and F10 would drop the second without an error. On restart, spec 0009 replay skips completed activities and issues no `record`; the next successful call sends what the outbox held. | F10, F13. | integrating lane w25:p1 brief D7, 2026-10-10; id scheme: this lane |
| D10 | `uas` resolves `--reverie-url`, then `UAS_REVERIE_URL`, then `http://127.0.0.1:7437`, on a clap argument with `env`. `uas-cli` resolves the outbox directory from `XDG_STATE_HOME` and `HOME` and passes both to the engine, which reads no environment. Only the `http` scheme is accepted. The client calls only `GET /health`, `GET /search` and `POST /observations`, all on the spec 0032 D3 allowlist. | Spec 0042 D1 (F12); host and slot clients stay interchangeable. | integrating lane w25:p1 brief D8, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0044-01]** (Ubiquitous): The `uas-engine` crate shall compile `HttpReverieClient` only with feature `reverie-http`, and the default build shall compile and pass its tests without it.
- **[REQ-0044-02]** (Ubiquitous): The `HttpReverieClient` shall bound every request by the D3 deadlines and the health probe by 500 ms.
- **[REQ-0044-03]** (Unwanted): IF the health probe or a context search fails, times out or returns a non-2xx status, THEN the client shall return an empty result and the engine shall mark the activity `degraded`, unless D5 ranks `outbox(refused)` above it, and continue it.
- **[REQ-0044-04]** (Unwanted): IF a `record` call cannot be delivered and D8 accepted the outbox directory, THEN the client shall append it to the outbox, fsync it, and return `Ok`, and the activity shall not fail.
- **[REQ-0044-05]** (State-driven and Event-driven): WHILE the outbox holds pending records, the client shall append each new record to its tail and send none ahead of them. WHEN a `search` gets a 2xx, a `record` send is acknowledged or a `record` appends behind pending records, or the health probe flips to live, the client shall send the records pending at that moment, oldest first, before the call returns, advancing the offset only after `reveried` acknowledges each.
- **[REQ-0044-06]** (Event-driven): WHEN a send attempt fails, the client shall make no further send before the backoff elapses, starting at 1 s, doubling, capped at 30 s, and reset after a successful send.
- **[REQ-0044-07]** (Event-driven): WHEN the engine completes an activity with a client wired, it shall set `ActivityResult.memory` by the D5 precedence.
- **[REQ-0044-08]** (Unwanted): IF the pending records exceed 10,000 lines or 50 MiB, THEN the holder of `drain.lock` shall drop the oldest pending records before it releases the lock, add them to the persisted dropped count, and log the total at `warn`.
- **[REQ-0044-09]** (Event-driven): WHEN the engine replays a session from its event log, it shall issue no `record` for a completed activity, and the first successful call after the restart shall send the records the outbox already held.
- **[REQ-0044-10]** (Ubiquitous): Each `record` call shall get one D9 `event_id` before its first send attempt, distinct from every other call's, and every resend shall carry it unchanged.
- **[REQ-0044-11]** (Unwanted): IF the outbox directory is a symlink, has a mode looser than `0700` or a foreign owner, THEN the client shall read and write nothing under it, `state.json` included, log once at `warn`, and mark every activity result `outbox(refused)`. IF an append or its fsync fails on an accepted directory, THEN the client shall log that record at `error` and mark that result `outbox(refused)`. In both cases `record` shall return `Ok` and the dropped count shall not change.
- **[REQ-0044-12]** (Optional feature): WHERE `--reverie-url` or `UAS_REVERIE_URL` is set, `uas` shall use that URL, the flag first, instead of `http://127.0.0.1:7437`.

## Plan

- [ ] the operator rules D2. Under A, amend the spec 0004 `reqwest` row (add `uas-engine`, behind `reverie-http`, `blocking`, `json`) and run `make pin-check`. Under B, no row changes.
- [ ] `uas-engine`: declare `reverie-http` (D1); add `MemoryStatus` and `ActivityResult.memory` with `#[serde(default)]` (D5). Check that the spec 0009 replay tests still read old events.
- [ ] `uas-engine`: `reverie::outbox` with the offset sidecar, cap, compaction, locks and mode checks (D6, D7, D8).
- [ ] `uas-engine`: `reverie::http::HttpReverieClient` with `ReverieDeadlines` (D3), probe and fail-open (D4), replay and backoff (D6), `event_id` (D9).
- [ ] Wire the client into the activity path that `uas harness run` drives: probe and search before an activity, record after it, memory status on the result. This closes the F2 gap left by spec 0008.
- [ ] `uas-cli`: forward `reverie-http`; add `--reverie-url` with `env = "UAS_REVERIE_URL"` (needs the clap `env` feature, spec 0042 Plan); pass the outbox directory (D10).
- [ ] Makefile and CI: a `reverie-http` leg in the spec 0033 feature matrix, when that matrix lands.

## Test

Every negative control runs against a fake server on `std::net::TcpListener` bound to
`127.0.0.1:0` in the test, with behaviour per route. `/health` answers `200` unless the case
is the probe itself, so the probe cannot short-circuit the route under test. That route refuses,
stalls before the head, trickles one byte a second, or answers `500`. No test touches port 7437:
the live daemon answers (F9), so a daemon-down control pointed at it could never go red. Tests
live under the `reverie` module path so the filter selects them.

| Requirement | Negative control, and how it goes red |
|---|---|
| REQ-0044-01 | `cargo nextest list` without the feature shows no `reverie::http` test; with it, it shows them. Red if the module compiles without the feature. |
| REQ-0044-02 | `/health` up, `/search` stalls before the head: the search returns within 2.5 s. Red if the head deadline is unset, since the call then waits 10 s. `/search` trickles one byte a second: the call returns within 10.5 s. Red if the deadline is a per-read idle timeout. A third test asserts the `ReverieDeadlines` default equals D3. |
| REQ-0044-03 | Closed port: `search` returns `Ok(empty)`, the result is `degraded`, and no search is sent. `/health` up and `/search` answering `500`: the same. Red if the error propagates or the status reads `live`. |
| REQ-0044-04 | `/health` up, `POST /observations` answering `500`: the server logs one `POST`, `record` returns `Ok`, and a separate file handle reads one line from `outbox.jsonl`. Red if `record` returns `Err` or the file is empty. |
| REQ-0044-05 | Pending r1 and r2, new r3, server up: it receives r1, r2, r3 in order. Server fails on r2: the offset points at r2 and r3 is at the tail. Red if r3 bypasses the outbox. Drain triggers, each with r1 pending and no backoff: a `search` answered `200`; a probe passing after a failed probe; the first passing probe of a fresh client; a `record` of r2. The server logs r1 before each call returns. Red if a trigger is removed, since r1 is then still pending on return. |
| REQ-0044-06 | With an injected clock, a record 0.5 s after a failed send causes no `POST`; the server counts `POST`s, not connections, since the probe connects every time. Red if the backoff gate is removed. |
| REQ-0044-07 | Four cases: a symlinked outbox directory and a closed port gives `outbox(refused)`; closed port gives `degraded`; server up, 2 pending and the backoff running on an injected clock gives `outbox(2)`; server up and none pending gives `live`. Red if the precedence is inverted. |
| REQ-0044-08 | Cap set to 3, five appends with no drainer: 3 pending, dropped count 2 in `state.json`, one `warn` captured. With `drain.lock` held by the test, the fifth append returns at once and the count lands when the lock holder finishes. Red if records drop with no count, or if the appender waits on the lock. |
| REQ-0044-09 | A pre-filled outbox directory from an earlier process: the first successful call sends its records; replaying a session with a completed activity sends no `POST`. Red if replay re-records. |
| REQ-0044-10 | One record whose direct send times out, then is replayed: both bodies carry the same `event_id`. Two different records with the same session, `topic_key` and `at`: their ids differ. Red if the id is derived from those fields or regenerated on resend. |
| REQ-0044-11, D8 | Directory mode `0755`, then a symlink to a test-owned directory, each with `record` against a closed port: `record` returns `Ok`, the result reads `outbox(refused)`, the directory listing is unchanged (for the symlink, the target's), and two records log one `warn`. Red if the mode or symlink check is removed, since `outbox.jsonl` then appears. A fault-injected fsync failure on an accepted directory: one `error`, that result reads `outbox(refused)`, and the dropped count in `state.json` is unchanged. Two threads appending 1,000 lines each yield 2,000 whole lines. Red if the count moves or `append.lock` is removed. The foreign-owner case needs a second uid and is not tested. |
| D8 lock order | A fixture takes `drain.lock` on the outbox handle, then asks for `append.lock`: the `assert!` panics, under `#[should_panic]`. Red if the assert is removed. The compaction tests run under the same assert, so a drainer that takes the locks reversed panics there. No deadlock fixture is used: `drain.lock` is only tried, never waited on, so a reversed order cannot hang and such a test could not go red. |
| REQ-0044-12 | In `uas-cli`, flag and variable both set: the flag's URL wins; variable alone: the variable wins. Red if the order flips. |

```bash
cargo nextest run -p uas-engine --features reverie-http reverie -j 2 && \
  cargo nextest run -p uas-cli --features reverie-http reverie -j 2
```

Passing means exit 0 with the `reverie::http` and `reverie::outbox` tests listed and run. Today
the command fails, because neither crate has a feature `reverie-http`; the same filter without
the feature runs 1 test (F14).

## Notes

- The guarantee covers records already appended. A record in flight when the process dies is
  in neither the daemon nor the outbox. Parked: a write-ahead mode that appends every record
  before sending, at one fsync per record.
- `search` returns `Vec<ReverieMemoryRecord>`, and each carries a mandatory `Attestation`.
  `/search` returns observations from any writer. The client keeps only hits that parse as uas
  records. Parked: a context result type, which changes the trait.
- Parked: TLS and non-loopback URLs. An `https` URL is refused at startup.
- Until the release build enables `reverie-http`, the shipped `uas` makes no reverie call (spec
  0033 D2). Turning it on is a separate ruling.
- The slot side stays in spec 0032. Both sides share the D3 numbers, so a harness degrades the
  same way inside and outside a slot.
- Not verified: that the running `reveried` includes the `event_id` contract of F10. Until it
  does, a crash resend may duplicate a row.
