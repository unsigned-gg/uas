---
status: complete
created: 2026-10-08
tags: [domain, lease, slot, rules]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0007-lease-rules

## Overview

`uas-domain` has slots and leases, but four rules about them were never
decided. A code review of the landed crates found them on 2026-10-08:

- One slot could hold two live leases. The ledger checked only the lease id.
- Anyone who knew a lease id could renew the lease. No token was checked.
- A renewal by zero seconds returned success and changed nothing.
- No test or document said whether a draining slot keeps its worktree.

Christian ruled on all four by interview in the UAS lane on 2026-10-08. He
chose the recommended option each time. This spec records the rules. The code
that enforces them lands in the same PR.

Why now: spec 0006 builds `uas-engine` on slots and leases. The engine must
rely on these rules, not on whatever the first adapter happens to do.

## Design

### Requirements (EARS)

- **Ubiquitous:** The `uas-domain` ledger shall permit at most one unexpired lease per slot at any point in time.
- **Event-driven:** WHEN renewing a lease, the caller shall provide a matching `LeaseToken` and an extension of at least one second.
- **State-driven:** WHILE a slot transitions to `Draining`, the slot shall retain its associated worktree reference.
- **Unwanted behavior:** IF a caller attempts to issue a lease for an occupied slot, THEN the ledger shall return `SlotLeased`.
- **Unwanted behavior:** IF a renewal request provides an incorrect token, THEN the domain shall return `TokenMismatch` without revealing lease state.

### Rules

| # | Rule | Before | Enforced by |
|---|---|---|---|
| R1 | One live lease per slot. | Two leases on one slot were accepted. | `check_slot_free`, called by every `LeaseLedger` adapter in `issue`. |
| R2 | Only the lease token renews a lease. | Any caller with the id renewed it. | `Lease::renew` and `LeaseLedger::renew` take a `&LeaseToken`. |
| R3 | A renewal extends by at least one second. | Zero returned success. | `Lease::renew`. |
| R4 | A draining slot keeps its worktree reference. | Unstated, untested. | `Slot::drain` and a test. |

R1 in detail:

- The ledger has no clock. It uses the new lease's `issued_at` as the time
  of the check.
- `issue` refuses a lease when another lease on the same slot is not
  expired at that time. The error is `DomainError::SlotLeased`.
- After that lease expires or is revoked, a new lease on the slot is allowed.
- The id check stays. A duplicate id still fails with `LeaseExists`, first.

R2 in detail:

- The token comparison takes the same time for any two tokens of the same
  length. A length mismatch returns false at once.
- A wrong token fails with `DomainError::TokenMismatch`. The error names the
  lease and never contains a token.

Check order in `Lease::renew`: the token, then expiry, then the zero
extension, then overflow. A caller without the token learns nothing about the
lease's expiry from the error. `LeaseLedger::renew` first looks the lease up,
so a missing id still fails with `LeaseNotFound`.

R4 in detail: the pool needs the worktree reference to tear the worktree
down after a drain. A slot drained from `Cold` has no worktree, which matches
the deserialization rule from PR #10.

### API changes

- `Lease::renew(&self, token: &LeaseToken, now: u64, extend_by: u64)`.
- `LeaseLedger::renew(&mut self, id: &LeaseId, token: &LeaseToken, now: u64, extend_by: u64)`.
- New: `LeaseToken::matches`, `Lease::holds_slot_at` and `check_slot_free`.
- New `DomainError` variants: `SlotLeased` and `TokenMismatch`.
  `DomainError` is not `#[non_exhaustive]`. A crate that matches it
  exhaustively must add the two arms. On 2026-10-08 no crate in the
  workspace does (checked with grep for `DomainError::` in `crates/`).

The only caller of `renew` and `issue` on 2026-10-08 is `uas demo` in
`uas-cli`. It is updated in the same PR.

### Not covered

- Public fields on `Lease`, `Budget` and `Envelope` still let a struct literal
  skip validation. Christian ruled on 2026-10-08 to keep them public while
  `uas-engine` is under construction. Revisit with spec 0006.
- Who may issue a lease. That is the capacity gate in swarm-control
  (spec 0001, integration edges), not the ledger.

## Plan

- [x] Record the four rulings in this spec.
- [x] R1: add `Lease::holds_slot_at` and `check_slot_free`. Call it from the in-memory ledger.
- [x] R2: add `LeaseToken::matches` and the token parameter on both `renew` methods.
- [x] R3: reject `extend_by == 0` in `Lease::renew`.
- [x] R4: document the rule on `Slot::drain` and pin it with a test.
- [x] Update `uas demo` to renew with the token.

## Test

Each rule has a test that failed, or did not compile, before the change. The
R4 test pins behaviour that already held.

```bash
cargo nextest run -p uas-domain -p uas-cli
```

Passing means exit 0. The rule tests are:

- R1: `lease::tests::check_slot_free_*` and `memory::tests::second_live_lease_on_a_slot_is_refused`.
- R2: `lease::tests::renew_requires_the_lease_token` and `lease::tests::token_mismatch_does_not_print_the_token`.
- R3: `lease::tests::renew_rejects_a_zero_extension`.
- R4: `slot::tests::drain_keeps_the_worktree`.

## Notes

Source of the findings: a read-only review of `uas-domain` and `uas-cli` at
`b529ead`, 2026-10-08. PR #10 fixed the deserialization findings from the
same review.

Alternative rejected for R1: refuse a lease only when its time range overlaps
another lease on the same slot. That rule allows a back-dated lease that ends
before the other begins. The ruled wording refuses it, because the other
lease is not expired at the new `issued_at`. The ruled wording is simpler and
needs only one time value, so this spec keeps it.
