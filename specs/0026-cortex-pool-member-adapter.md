---
status: planned
created: 2026-10-10
tags: [cortex, pool, adapter, grpc, cas-engine, lease]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0004-dependency-and-licence-policy, 0007-lease-rules, 0008-soma-cortex-saga-reverie-integration, 0019-pool-runtime-decision]
---

# 0026-cortex-pool-member-adapter

## Overview

CAS joins a Cortex agent pool as a member. The adapter takes pool work over the
gRPC service `cortex.mesh.v1.PoolWork`. It runs each unit in a CAS slot under a
lease, and it reports the outcome to Cortex.

Why now: the cortex lead relayed that the operator wants Cortex to integrate with
CAS for agent pools [reported, mesh 14ee8a7a, 2026-10-10]. A relay approves
nothing. the operator signs the integration before any transport code (D1).

In scope: the port traits, the lease bridge and the outcome classifier in
`cas-engine`, with fakes in tests. The gRPC transport is one `cas-engine` module
behind the cargo feature `cortex-grpc`, off by default. `cas-domain` gets no
network I/O, no clock and no runtime.

Out of scope: a real harness run in a slot waits on spec 0019 D2 (D8). Member
join tokens are cortex spec 0005, in the Director of Engineering's identity veto area (D7). The adapter
uses the existing join flow and adds no token logic.

Facts, 2026-10-10:

- **F1 [verified, read `vendor/cortex/proto/pool.proto`]:** `PoolWork` has `Recv`
  (a server stream of `PoolDelivery`), `Ack`, `Extend`, `Nack` and `Term`. The last
  four return `google.protobuf.Empty`. The file imports `cortex/mesh/v1/lane.proto`
  (`LaneMessage`) and `google/protobuf/empty.proto`. The pool comes from the
  member's registered membership (`mesh_peers.pool`). No request field carries a
  pool name (l.6-8).
- **F2 [verified, `pool.proto` l.25-29]:** `lease_ms` is the delivery lease, and
  "members time Extend from lease_ms/3". `delivery_attempt` is 1 on the first
  delivery. At `max_deliver` the backend dead-letters. `max_deliver` is a backend
  setting, not a field.
- **F3 [verified, read `crates/cas-domain/src/lease.rs` l.247-317]:** `Lease::renew(token,
  now, extend_by)` adds `extend_by` to the old `expires_at`. It refuses a wrong
  token, an expired lease and `extend_by = 0` (spec 0007 R2, R3). The domain has no
  clock: callers pass `now`. There is no `LeaseLedger` type.
- **F4 [verified, read `crates/cas-domain/src/slot.rs` l.103-230]:** the legal moves
  are `Cold -> Warm -> Leased -> Suspended -> Leased -> Warm`. `release` moves
  `Leased -> Warm`. `Draining` is final.
- **F5 [verified, read `crates/cas-engine/Cargo.toml` at 0d5d91f]:** `cas-engine`
  depends on `cas-domain`, `serde`, `serde_json`, `thiserror`, `toml` and `tracing`.
  It has no `tokio`.
- **F6 [verified, cortex@c7fdb85 `Cargo.lock` and `crates/cortex-api/build.rs`]:**
  cortex uses `tonic` 0.14.6, `tonic-prost` 0.14.6, `prost` 0.14.4 and `prost-types` 0.14.4 at run time. At
  build time it uses `tonic-prost-build` 0.14.6 with `protox` 0.9.1, so it needs no
  system `protoc`.
- **F7 [verified, `ls vendor/cortex/proto`]:** the vendored protos are flat
  (`vendor/cortex/proto/lane.proto`, `peer.proto`). The imports name
  `cortex/mesh/v1/lane.proto` and `cortex/mesh/v1/peer.proto`, so the codegen
  include root must map that path (P2).
- **F8 [verified, throwaway project with the spec 0004 pins, `cargo deny check`,
  2026-10-10T16:26Z]:** 106 packages, tonic `server` in dev, print "advisories ok,
  bans ok, licenses ok, sources ok". Warnings: duplicate `hashbrown`, `logos` (3
  crates) and `syn`, through `protox`.
- **F9 [reported, cortex spec 0025, handoff `cortex-integration-edges-2026-10-10.md`]:**
  the JSON v0 routes under `/v1/mesh/pool/*` are deprecated.
- **F10 [reported, `cerebral-work/revenant@a8549ed:crates/mesh-pool/src/lib.rs`]:**
  revenant mesh-pool is a reference dispatcher. Its timing is not quoted here.
- **F11 [verified, read `specs/0019-pool-runtime-decision.md`]:** 0019 D2, the
  `cas-pool` runtime, waits on a benchmark and the operator's ruling. 0019 D1 names
  `tokio` for crates that serve a socket, pending the operator.

## Design

### Requirements (EARS)

- **Ubiquitous:** The adapter shall live in `cas-engine`. `cas-domain` shall gain no
  network I/O, clock or runtime.
- **Ubiquitous:** The adapter shall send no pool name. Cortex takes the pool from
  membership (F1).
- **Ubiquitous:** The adapter shall use `PoolWork` gRPC only, never the JSON v0
  routes (F9).
- **Event-driven:** WHEN the adapter has free slots, it shall call `Recv` with
  `free_slots` equal to their number.
- **Event-driven:** WHEN `Recv` yields a `PoolDelivery`, the adapter shall check that
  the slot is free (R1). Then it issues a CAS lease of `ttl + 1` seconds (lease
  bridge) and moves the slot `Warm -> Leased`.
- **Unwanted behavior:** IF a `PoolDelivery` has `lease_ms` below 300 (proto3 sends 0
  when the field is unset), THEN the adapter shall call `Term` with a reason, start
  no `Extend` and lease no slot (D9).
- **State-driven:** WHILE a delivery runs, the adapter shall call `Extend` every
  `lease_ms / 3` and renew the CAS lease as the lease bridge says.
- **Event-driven:** WHEN a task succeeds, the adapter shall call `Ack` and release
  the slot (`Leased -> Warm`).
- **Event-driven:** WHEN a task fails with a recoverable error, the adapter shall
  call `Nack` with the D5 delay and release the slot.
- **Unwanted behavior:** IF a payload does not parse or breaks policy, THEN the
  adapter shall call `Term` with a reason and release the slot.
- **Unwanted behavior:** IF `Extend` fails twice in a row, or a CAS renew fails,
  THEN the adapter shall stop the task and release the slot. It sends no `Ack`, so
  Cortex redelivers after `lease_ms`.
- **Unwanted behavior:** IF the `Recv` stream ends, THEN the adapter shall stop
  `Recv` and reconnect with exponential backoff. Running tasks continue while
  `Extend` succeeds.
- **Optional feature:** WHERE the feature `cortex-grpc` is off, `cas-engine` shall
  build with no `tonic`, `prost`, `prost-types` or `tokio`. The build-dependencies
  `tonic-prost-build` and `protox` are optional and enabled by that feature, and
  `build.rs` does nothing unless `CARGO_FEATURE_CORTEX_GRPC` is set (P2).

### Wire contract

The adapter is a client of `cortex.mesh.v1.PoolWork` (F1). Code generation reads
`vendor/cortex/proto/pool.proto`, `lane.proto` and `peer.proto` (`lane.proto` l.12
imports `cortex/mesh/v1/peer.proto`), and the Google types `empty.proto`,
`struct.proto` and `timestamp.proto` (`lane.proto` l.13-15, `peer.proto` l.11)
[verified, read]. `Struct` and `Timestamp` map to `prost-types` (spec 0004).

| RPC | Request fields (tag) | Reply | Use |
|---|---|---|---|
| `Recv` | `peer_id` (1), `free_slots` (2) | stream of `PoolDelivery` | Take at most `free_slots` units. |
| `Ack` | `peer_id` (1), `delivery_tokens` (2, repeated) | `Empty` | The unit is done. |
| `Extend` | `peer_id` (1), `delivery_token` (2) | `Empty` | Keep the Cortex lease alive. |
| `Nack` | `peer_id` (1), `delivery_token` (2), `delay_ms` (3) | `Empty` | Redeliver after `delay_ms`. |
| `Term` | `peer_id` (1), `delivery_token` (2), `reason` (3) | `Empty` | Dead-letter now. |

`PoolDelivery` carries `delivery_token` (1), `stream_seq` (2), `message`
(`LaneMessage`, 3), `lease_ms` (4) and `delivery_attempt` (5). The
`delivery_token` is opaque and bound to this member.

### Lease bridge

Spec 0007 rules: R1, one live lease per slot (`check_slot_free`). R2, only the
matching `LeaseToken` renews. R3, a renewal extends by at least 1 s. R4, a
draining slot keeps its worktree. The bridge keeps both leases in step:

1. `now` is the adapter's monotonic seconds since its first issue. The clock lives
   in the adapter, never in `cas-domain` (F3).
2. `ttl = max(1, ceil(lease_ms / 1000))`. The CAS lease is issued for `ttl + 1`.
3. Every `lease_ms / 3`, the adapter calls `Extend`. Then it sets
   `target = now + ttl + 1`.
4. IF `expires_at < target`, THEN it renews with `extend_by = target - expires_at`,
   which is at least 1 (R3). Else it does not renew.

`now` counts whole seconds, and a lease is expired when `now >= expires_at`
(`lease.rs` l.275-276) [verified, read]. The extra second keeps `expires_at` above
`now` at each tick, so a renew never meets an expired lease. The CAS lease ends at
most `ttl + 1` seconds after `now`.

### Outcomes

| Outcome | RPC | Slot move |
|---|---|---|
| Success | `Ack` | `release`: `Leased -> Warm` |
| Recoverable error | `Nack`, delay by D5 | `release` |
| Payload does not parse, or breaks policy | `Term`, with a reason | `release` |
| `Extend` fails twice, renew fails, reconnect misses the lease | none: Cortex redelivers after `lease_ms` | `release` |
| Adapter shutdown | `Nack`, `delay_ms = 0`, for each running unit (D6) | `release`, then `drain` (final) |

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | Cortex integrates with CAS for agent pools through this adapter. No transport code lands before this is signed. | **the operator to rule.** Relayed only (mesh 14ee8a7a, 2026-10-10). |
| D2 | The port traits (`PoolWork`, `TaskRunner`), the lease bridge and the classifier go in `cas-engine` with no new dependency. The tonic client is one module behind the feature `cortex-grpc`, off by default. No layer break: `cas-engine` sits above `cas-domain` (spec 0001). | Architect w10:pP proposal (mesh 7ec5bb4d); cas-engineer, 2026-10-10 (the operator may override) |
| D3 | Pins: the versions cortex@c7fdb85 uses (F6), so both ends share one codegen. Newer `protox` 0.10.0 and `tokio` 1.53.2 exist and are not taken. Spec 0004 records them under "Cortex pool-member transport", with the licences read and the deny check (F8). | cas-engineer, 2026-10-10 (the operator may override) |
| D4 | The lease bridge issues for `ttl + 1` and renews up to `now + ttl + 1`, never by a fixed step. The extra second covers the whole-second clock (`now >= expires_at` is expired). Renew adds to the old expiry (F3). A fixed `ttl` step every `lease_ms / 3` grows the CAS lease about three times faster than time. A crashed adapter would then hold the slot after Cortex redelivers. | cas-engineer, 2026-10-10 (the operator may override) |
| D5 | The backend dead-letters at `max_deliver` (F2), so the adapter keeps no retry count. `Nack` uses `delay_ms = min(1000 * 2^(a - 1), 60000)` in saturating arithmetic, with `a = max(1, delivery_attempt)`. | cas-engineer, 2026-10-10 (the operator may override) |
| D6 | On shutdown the adapter stops `Recv`, Nacks each running unit with `delay_ms = 0`, releases the slots and drains them. `drain` happens only at shutdown, because `Draining` is final (F4). | cas-engineer, 2026-10-10 (the operator may override) |
| D7 | Member join tokens and their auth stay in cortex spec 0005 (CER-1988). This spec decides nothing there. | **the Director of Engineering's area** (identity services veto). Not decided here. |
| D8 | A real `TaskRunner` (a harness run in a slot) needs the `cas-pool` runtime. Until then the port has a fake only. | **the operator to rule**, as spec 0019 D2 (F11). |
| D9 | The minimum `lease_ms` is 300, so the `Extend` tick is at least 100 ms. A smaller value, 0 included (the proto3 default), is a broken delivery: the adapter Terms it. | cas-engineer, 2026-10-10, critic round 1 (the operator may override) |

## Plan

- [ ] the operator signs the integration (D1). P1 and P2 start only after that.
- [ ] P1, `cas-engine`, no feature, no new dependency: the `PoolWork` and
  `TaskRunner` port traits, the lease bridge (D4) and the classifier (D5, D6).
  Tests use a fake port and a fake clock, red first.
- [ ] P2, `cas-engine`, feature `cortex-grpc`: a `build.rs` that compiles the
  vendored protos with `protox` and `tonic-prost-build`, with the include root
  mapped for `cortex/mesh/v1/` (F7). The tonic client implements the port. Only
  the spec 0004 pins. `tonic-prost-build` and `protox` are optional build-dependencies
  that the feature enables, and `build.rs` returns at once unless
  `CARGO_FEATURE_CORTEX_GRPC` is set. Its done-when includes `cargo deny check`.
- [ ] A real `TaskRunner` after the operator rules spec 0019 D2 (D8).
- [ ] Join tokens: no work here (D7).

## Test

```bash
make spec-check
cargo nextest run -p cas-engine --build-jobs 2 cortex_pool
cargo nextest run -p cas-engine --build-jobs 2 --features cortex-grpc cortex_pool
cargo deny check
```

Red-first tests. P1 uses a fake port and a fake clock:

- `cortex_pool_recv_asks_free_slots`: `Recv` carries the number of free slots.
- `cortex_pool_ack_releases_to_warm`: success sends `Ack`; the slot is `Warm`.
- `cortex_pool_extend_every_third_of_lease`: `Extend` runs every `lease_ms / 3`.
- `cortex_pool_renew_tracks_wall_time`: after 30 ticks of a 3000 ms lease,
  `expires_at - now` is at most `ttl + 1` (D4). A fixed-step mutant must go red.
- `cortex_pool_renew_clamps_to_one_second`: `lease_ms = 900` gives `ttl = 1`.
- `cortex_pool_short_lease_survives_second_boundaries`: `lease_ms = 900`, 10 ticks
  over 3 s. Every renew is `Ok` and the slot stays `Leased`. A `target = now + ttl`
  mutant must go red.
- `cortex_pool_term_below_min_lease`: `lease_ms = 0` and `299` each send `Term`, no
  `Extend`, and lease no slot (D9).
- `cortex_pool_nack_delay_grows`: attempts 0, 1, 2, 8 and `u32::MAX` give 1000,
  1000, 2000, 60000 and 60000 ms (D5).
- `cortex_pool_term_on_bad_payload`: a payload that does not parse sends `Term`.
- `cortex_pool_extend_fails_twice_releases`: no `Ack`, the slot is `Warm`.
- `cortex_pool_shutdown_nacks_then_drains`: `Nack` with 0, then `Draining`.

P2 adds one test per RPC against an in-process fake server (tonic `server`, dev
only): `cortex_pool_grpc_recv`, `_ack`, `_extend`, `_nack` and `_term`. Each checks
the request fields and tags on the wire. A pass means every test above is green. The build without
the feature pulls no `tonic`. `cargo deny check` prints "advisories ok, bans ok,
licenses ok, sources ok".

## Notes

- Rejected: a fixed `extend_by` per tick (D4).
- Rejected: the JSON v0 routes (F9).
- Rejected: the transport on by default, or in `cas-domain`. Either one puts a
  network stack into builds that do not need it.
- `protox` 0.9.1 ships a resolver for the Google well-known types
  (`src/file/google.rs`) [verified, file read]. That it resolves `empty.proto`,
  `struct.proto` and `timestamp.proto` here is not yet run [asserted].
- Reference: revenant mesh-pool (F10).
