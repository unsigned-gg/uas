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

Why now: Christian ruled that CAS owns the pool-member adapter (D1) [reported,
relayed by the integrator, mesh 74f407df, 2026-10-10]. The cortex lead first
relayed the request (mesh 14ee8a7a).

In scope: the port traits, the lease bridge, the outcome classifier and the gRPC
transport, in `cas-engine`, all behind the cargo feature `cortex-grpc`, off by
default (D1, D2). Fakes test the ports and the bridge. `cas-domain` gets no
network I/O, no clock and no runtime.

A real harness run in a slot needs its own spec, 0027, first (D8). Member
join tokens are cortex spec 0005, in Allen's identity veto area (D7). The adapter
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
  include root must map that path (P1).
- **F8 [verified, throwaway project with the spec 0004 pins, `cargo deny check`,
  2026-10-10T16:26Z]:** 106 packages, tonic `server` in dev, print "advisories ok,
  bans ok, licenses ok, sources ok". Warnings: duplicate `hashbrown`, `logos` (3
  crates) and `syn`, through `protox`.
- **F9 [reported, cortex spec 0025, handoff `cortex-integration-edges-2026-10-10.md`]:**
  the JSON v0 routes under `/v1/mesh/pool/*` are deprecated.
- **F10 [reported, `cerebral-work/revenant@a8549ed:crates/mesh-pool/src/lib.rs`]:**
  revenant mesh-pool is a reference dispatcher. Its timing is not quoted here.
- **F11 [verified, read `specs/0019-pool-runtime-decision.md`]:** 0019 D2, the
  `cas-pool` runtime, waits on a benchmark and Christian's ruling. 0019 D1 names
  `tokio` for crates that serve a socket, pending Christian.

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
  bridge), with the host's task `Budget` (D10), and moves the slot `Warm -> Leased`.
- **Unwanted behavior:** IF a `PoolDelivery` finds no `Warm` slot, THEN the adapter
  shall call `Nack` with the D5 delay, lease no slot and start no task (D11).
- **Unwanted behavior:** IF a `PoolDelivery` has `lease_ms` below 300 (proto3 sends 0
  when the field is unset), THEN the adapter shall call `Nack` with the D5 delay and
  stop `Recv` with an error that names `lease_ms` and its value. The stop is final
  until a restart: no reconnect and no second `Recv`. It shall surface the error to
  its caller, and shall not call `Term`, start `Extend` or lease a slot (D9).
- **State-driven:** WHILE a delivery runs, the adapter shall call `Extend` every
  `lease_ms / 3` and renew the CAS lease as the lease bridge says.
- **Event-driven:** WHEN a task succeeds, the adapter shall call `Ack` and release
  the slot (`Leased -> Warm`).
- **Event-driven:** WHEN a task fails with a recoverable error, the adapter shall
  call `Nack` with the D5 delay and release the slot.
- **Unwanted behavior:** IF a payload does not parse or breaks policy, THEN the
  adapter shall call `Term` with a reason and release the slot.
- **Ubiquitous:** A round is one pass over the units whose `Extend` is due. Each due
  `Extend` has a deadline: its due time plus the quarter, the shortest `lease_ms / 4`
  among running units, fixed at the start of the pass. A call is bounded by the time
  left to its deadline, and a cut call is a failed `Extend`. A unit whose deadline has
  passed before its turn counts as failed with no call. The pass orders the due units:
  first those whose last `Extend` was skipped, then those whose last was cut, then the
  rest; ties go by slot order (D12).
- **Ubiquitous:** Every other call on the adapter thread (`Ack`, `Nack`, `Term`, the
  `Recv` open) shall be bounded by the smaller of its cap and the time left to the
  earliest pending `Extend` deadline (D12), where pending means the next deadline of every running unit, due or not (test `extend_deadline_bounds_other_rpcs`).
- **Ubiquitous:** In each loop step the adapter shall run the due `Extend` pass before any `Ack`, `Nack`, `Term` or `Recv` open, so each of those gets at least the quarter (D13).
- **Event-driven:** WHEN the adapter shuts down, it shall stop `Recv`, cancel every running task first, then Nack each unit with `delay_ms = 0`, bounded by the smaller of the cap and the time left on that unit's Cortex lease, skip the `Nack` of a unit whose lease has ended, release the slots and drain them (D6).
- **Unwanted behavior:** IF `Extend` fails twice in a row, or a CAS renew fails,
  THEN the adapter shall stop the task and release the slot. It sends no `Ack`, so
  Cortex redelivers after `lease_ms`.
- **Unwanted behavior:** IF the `Recv` stream ends before it delivered `free_slots`
  units, and not by a D9 stop, THEN the adapter shall stop `Recv` and reconnect after
  1 s, doubling to at most 60 s, and reset to 1 s after a delivery (D11). Running
  tasks continue while `Extend` succeeds.
- **Event-driven:** WHEN the `Recv` stream ends after it delivered `free_slots` units,
  the adapter shall call `Recv` again when a slot is free, with no backoff. It reads
  a filled stream until it ends or a slot frees, so a surplus unit reaches the D11
  `Nack`.
- **Event-driven:** WHEN a slot frees while a filled stream is still open, the adapter
  shall drop that stream and call `Recv` with the free count, with no backoff (D11).
- **Optional feature:** WHERE the feature `cortex-grpc` is off, `cas-engine` shall
  build with no `tonic`, `prost`, `prost-types` or `tokio`. The build-dependencies
  `tonic-prost-build` and `protox` are optional and enabled by that feature, and
  `build.rs` does nothing unless `CARGO_FEATURE_CORTEX_GRPC` is set (P1).

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
| `lease_ms` below 300 (D9) | `Nack` with the D5 delay, then stop `Recv` | none: no slot is leased |
| No `Warm` slot for the delivery (D11) | `Nack` with the D5 delay | none: no slot is leased |
| `Extend` fails twice, renew fails, reconnect misses the lease | none: Cortex redelivers after `lease_ms` | `release` |
| Adapter shutdown | cancel every task, then `Nack` with `delay_ms = 0`, bounded by the time left on each lease (D6) | `release`, then `drain` (final) |

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | CAS owns the pool-member adapter. The spec lands. The `tonic` 0.14 and `prost` 0.14 pins go into spec 0004. The first build package is the transport and the lease handling, behind a feature flag, off by default. | **Ruled:** Christian, in session, lane w25:p1 interview, 2026-10-10 [reported, relayed by the integrator, mesh 74f407df]. He named `tonic` and `prost`; the other 0004 rows are lane pins (D3). |
| D2 | All adapter code compiles only under the feature `cortex-grpc`, off by default (D1). Inside it, the port traits (`PoolWork`, `TaskRunner`), the lease bridge and the classifier use no tonic type, so fakes test them. The tonic client implements `PoolWork`. No layer break: `cas-engine` sits above `cas-domain` (spec 0001). | Architect w10:pP proposal (mesh 7ec5bb4d); cas-engineer, 2026-10-10; the feature scope follows D1 (Christian may override) |
| D3 | Pins: the versions cortex@c7fdb85 uses (F6), so both ends share one codegen. Newer `protox` 0.10.0 exists and is not taken. `tokio` is not new here: its 0004 row says 1.53.1, but the root `Cargo.toml` requirement is the caret `1.53.1`, so `Cargo.lock` has resolved 1.53.2 since 6ec6011 [verified, at 2c6a557]. That drift is a separate item, not fixed here. Spec 0004 records them under "Cortex pool-member transport", with the licences read and the deny check (F8). | cas-engineer, 2026-10-10 (Christian may override) Pin tonic and prost with `=`: the tokio caret pin already drifted (CER-2798). |
| D4 | The lease bridge issues for `ttl + 1` and renews up to `now + ttl + 1`, never by a fixed step. The extra second covers the whole-second clock (`now >= expires_at` is expired). Renew adds to the old expiry (F3). A fixed `ttl` step every `lease_ms / 3` grows the CAS lease about three times faster than time. A crashed adapter would then hold the slot after Cortex redelivers. | cas-engineer, 2026-10-10 (Christian may override) |
| D5 | The backend dead-letters at `max_deliver` (F2), so the adapter keeps no retry count. `Nack` uses `delay_ms = min(1000 * 2^(a - 1), 60000)` in saturating arithmetic, with `a = max(1, delivery_attempt)`. | cas-engineer, 2026-10-10 (Christian may override) |
| D6 | On shutdown the adapter stops `Recv` and cancels every running task first, so no unit runs while another's `Nack` hangs. Then it Nacks each unit with `delay_ms = 0`, bounded by the smaller of the cap and the time left on that unit's Cortex lease; a unit whose lease has ended gets no `Nack`, since Cortex redelivers it anyway. Then it releases the slots and drains them. `drain` happens only at shutdown, because `Draining` is final (F4). | cas-engineer, 2026-10-10; cancel-first and the bound, CER-2802 from the P1 Part B critic (Christian may override) |
| D7 | Member join tokens and their auth stay in cortex spec 0005 (CER-1988). This spec decides nothing there. | **Allen's area** (identity services veto). Not decided here. |
| D8 | A real `TaskRunner` (a harness run in a slot) needs the `cas-pool` runtime. That runtime is ruled: spec 0019 D2, Option A, OS threads as built (Christian, in session, lane w25:p1 interview, 2026-10-10 [reported, relayed by the integrator, mesh e42efbc2]). The harness runner is a new concept with open design, so it needs its own spec, 0027, before any brief; until then the port has a fake only. | Runtime: ruled by Christian (spec 0019 D2). Spec 0027 first: lane reading, cas-engineer, 2026-10-10 (Christian may override) |
| D9 | The minimum `lease_ms` is 300, so the `Extend` tick is at least 100 ms. A smaller value, 0 included (the proto3 default), is a wire or backend fault, not a work fault. The adapter Nacks the unit with the D5 delay and stops `Recv` with an error that names `lease_ms` and its value. The stop is final until a restart: a reconnect would Nack again until `max_deliver`, and the backend would dead-letter the backlog. It never Terms it: on a dropped proto3 field, `Term` would dead-letter the whole backlog. | Integrator ruling w25:p1, 2026-10-10, mesh 4653a43a (lane decision, Christian may override); the 300 floor: cas-engineer, critic round 1 |
| D10 | The lease `Budget` is the host's task cap, set in the adapter config. The adapter never derives it from `ttl`, and renewals move `expires_at` only. `max_wall_secs` is the task's wall-clock cap. Spec 0023 plans to freeze a slot whose budget runs out, so a budget of `ttl + 1` would freeze any task longer than one lease window. Nothing reads `max_wall_secs` today [verified, `git grep max_wall_secs` at 3009f8d]. | cas-engineer, 2026-10-10, P1 Part A critic (Christian may override) |
| D11 | Two paths the first build added, kept: (a) a delivery with no `Warm` slot gets `Nack` with the D5 delay, because the server sent more than `free_slots` (F1) and that is not a work fault; (b) the `Recv` reconnect backoff has D5's shape, 1 s doubling to 60 s, and resets after a delivery; (c) a `Recv` stream that ends after it delivered `free_slots` units is the normal end, not a fault, so it costs no backoff, and the adapter reads a filled stream until it ends or a slot frees, so a surplus unit gets the (a) `Nack`; when a slot frees first, it drops that stream and calls `Recv` with the free count, with no backoff, so a server that keeps a filled stream open cannot stall new work. | cas-engineer, 2026-10-10, P1 Part A departures 3 and 9; (c) P1 Part A critic item 2 (Christian may override) |
| D12 | All calls on the adapter thread run one after another, so their time adds up. A round is one pass over the units whose `Extend` is due; units not due are neither called nor counted. Each due `Extend` gets a deadline: its due time plus the quarter (the shortest `lease_ms / 4` among running units, fixed at the start of the pass). A call is bounded by the time left; a unit whose deadline passed before its turn counts as failed with no call. Order: last `Extend` skipped, then last cut, then the rest, ties by slot order. `Ack`, `Nack`, `Term` and the `Recv` open are bounded by the smaller of their cap and the time left to the earliest pending `Extend` deadline; a cut one follows D6 (logged, slot released), and a finished unit whose `Ack` never lands may be redelivered, as at-least-once allows. Traces (`lease_ms` 3000, quarter 750): two or three hung units are all released at 2750 ms; four hung, two at 3000 and two at 30000, release at 2750 and 27500; one hung unit A beside a healthy B: R1 cuts A and skips B, R2 calls B first, so B keeps its slot and A is released at 2750; A at 0 and B at 900, both hung, release A at 2750 and B at 3650 (a budget that starts at A's turn gives 3400); a hung `Ack` after A's first cut leaves A's release at 2750, not 3750. Slow broker, three units answering in 700 ms: R1 A answers at 1700, B is cut at 1750, C is skipped; R2 C answers at 2700, B is cut at 2750 and stops, A is skipped; later A and C alternate and no `Extend` lands late. At the D9 floor (quarter 75 ms, 29 ms calls) the cut unit goes first next pass, so no unit fails twice in a row. A unit stops by 11/12 of its lease, before Cortex redelivers. A healthy unit can still stop (B above; or behind two hung units): that is safe, since an early stop never runs a unit twice. A fixed 2 s bound broke D4. At the D9 floor the quarter is 75 ms: sane at the measured 29 ms average and 34 ms max to the broker [reported, critic, mesh 6d2b4eff], tight under broker load. The channel keeps 5 s connect and 2 s request timeouts as caps. The bounded `extend` lands with the tonic client, on top of the first part. Rejected: stating the limit and leaning on at-least-once. | cas-engineer, 2026-10-10, P1 Part B critic item B1 and three rounds; elapsed-time budget, per-unit order and the bound on other calls ruled by the integrator w25:p1, mesh 5c083d44 and 4d4d2235 (Christian may override) |
| D13 | Each loop step runs the due `Extend` pass first, then `Ack`, `Nack`, `Term` and the `Recv` open. After the pass no unit is past due, so the earliest pending deadline is at least the quarter away and those calls get at least the quarter. Without it, an `Ack` that runs while a unit is past due gets only the rest of that unit's quarter: at the D9 floor that can be under the 34 ms the critic measured, so the `Ack` is cut and a finished unit is redelivered. | cas-engineer, 2026-10-10, CER-2801 from the P1 Part B critic (Christian may override) |

## Plan

- [x] Christian ruled D1, 2026-10-10 [reported, mesh 74f407df].
- [ ] P1, `cas-engine`, feature `cortex-grpc`, off by default: the transport and
  the lease handling (D1). The `PoolWork` and `TaskRunner` port traits, the lease
  bridge (D4) and the classifier (D5, D6, D9), tested red first with a fake port
  and a fake clock. A `build.rs` that compiles the vendored protos with `protox` and `tonic-prost-build`, with the include root
  mapped for `cortex/mesh/v1/` (F7). The tonic client implements the port. Only
  the spec 0004 pins. `tonic-prost-build` and `protox` are optional build-dependencies
  that the feature enables, and `build.rs` returns at once unless
  `CARGO_FEATURE_CORTEX_GRPC` is set. Its done-when includes `cargo deny check`.
  Past 800 changed lines, split: the ports, the bridge and the classifier first,
  then the tonic client, both behind the feature. The bounded `extend` (D12) lands
  with the tonic client, on top of the first part.
- [ ] P2: spec 0027 (the harness runner: a real `TaskRunner` in a `cas-pool` slot), then its build (D8).
- [ ] Join tokens: no work here (D7).

## Test

```bash
make spec-check
T=$(mktemp); cargo tree -p cas-engine -e normal,build --prefix none > "$T"
! grep -E '^(tonic|prost)' "$T"
cargo tree -p cas-engine -e normal,build --prefix none --features cortex-grpc | grep -E '^tonic '
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
- `cortex_pool_nack_below_min_lease_stops_recv`: `lease_ms = 0` and `299` each send
  `Nack` with the D5 delay and no `Term`, start no `Extend` and lease no slot. `Recv`
  stops with an error that names `lease_ms` and the value, and no second `Recv` or
  reconnect follows (D9).
- `cortex_pool_nack_delay_grows`: attempts 0, 1, 2, 8 and `u32::MAX` give 1000,
  1000, 2000, 60000 and 60000 ms (D5).
- `cortex_pool_term_on_bad_payload`: a payload that does not parse sends `Term`.
- `cortex_pool_extend_fails_twice_releases`: no `Ack`, the slot is `Warm`.
- `cortex_pool_extend_bounded_by_shortest_tick`: units with `lease_ms` 30000 and 3000;
  a fake `Extend` that hangs is cut at 750 ms, and the 3000 ms unit keeps its 1000 ms
  tick. Two hung calls release that unit by 2750 ms, before its lease ends (D12).
- `cortex_pool_extend_round_budget_two_hung_units`: two 3000 ms units, every `Extend`
  hangs. Both are released by 2750 ms. A per-call-only mutant releases them at 3250
  and 4000 ms and must go red (D12).
- `cortex_pool_extend_round_rotates`: unit A hangs, unit B answers. B never reaches
  two failures in a row and keeps its slot; A is released before its lease ends.
- `cortex_pool_extend_slow_broker`: three 3000 ms units, every `Extend` answers in
  700 ms. No `Extend` lands after its unit's lease. A mutant where only a cut spends
  the budget lands the third at 3100 ms and must go red (D12).
- `cortex_pool_extend_deadline_from_due_time`: A delivered at 0, B at 900, every
  `Extend` hangs. A is released by 2750 ms. A mutant that starts A's budget when its
  turn comes releases A at 3400 ms and must go red (D12).
- `cortex_pool_extend_deadline_bounds_other_rpcs`: 3000 ms leases, broker hang. A's
  `Extend` is cut at 1750; an `Ack` for a finished unit X then hangs. A's second
  failure lands by 2750 ms, not 3750 ms (D12).
- `cortex_pool_shutdown_nacks_then_drains`: `Nack` with 0, then `Draining`.
- `cortex_pool_shutdown_cancels_all_first`: three running units, a hung broker. Every task is cancelled before the first `Nack`; each `Nack` is cut at the end of its unit's lease; a unit whose lease has ended gets no `Nack`. A mutant that cancels inside the `Nack` loop must go red (D6).
- `cortex_pool_ack_after_due_pass_not_cut`: `lease_ms` 300; a unit finishes while another's `Extend` is past due. The pass runs first and the `Ack` gets at least 75 ms, so a 34 ms `Ack` lands. A mutant that runs `Ack`s before the pass must go red (D13).
- `cortex_pool_task_error_nacks_and_releases`: a recoverable error sends `Nack` with
  the D5 delay; the slot is `Warm`.
- `cortex_pool_filled_stream_end_no_backoff`: `free_slots = 2`, the fake sends 2
  units and ends; once a slot is free, the next `Recv` follows with no backoff (D11).
- `cortex_pool_filled_stream_dropped_when_slot_frees`: the fake sends 2 of 2 units and
  keeps the stream open; when a slot frees, the adapter drops the stream and calls
  `Recv` with `free_slots = 1`, with no backoff (D11).
- `cortex_pool_stream_end_reconnects_with_backoff`: an early end reconnects after 1, 2, 4 ... 60
  and 60 s, then 1 s again after a delivery (D11).
- `cortex_pool_nack_when_no_warm_slot`: 3 slots, the fake sends one unit more; that
  unit gets `Nack` with the D5 delay, no lease and no task (D11).
- `cortex_pool_lease_budget_is_task_cap`: the lease carries the configured `Budget`,
  and renewals change `expires_at` only (D10).

P1 also adds one test per RPC against an in-process fake server (tonic `server`, dev
only): `cortex_pool_grpc_recv`, `_ack`, `_extend`, `_nack` and `_term`. Each checks
the request fields and tags on the wire. A pass means every test above is green,
the first `cargo tree` exits 0 and its output holds no `tonic` or `prost`, and the
`--features cortex-grpc` tree holds `tonic` (a positive control, after P1 lands). `cargo deny check` prints "advisories ok, bans ok,
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
