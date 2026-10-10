---
status: planned
created: 2026-10-10
tags: [identity, delegation, oidc, spiffe, audit, cas-engine, cas-sandbox]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0004-dependency-and-licence-policy, 0007-lease-rules, 0013-llm-gateway-socket-proxy, 0022-custody-landing]
---

# 0029-agent-identity-and-on-behalf-of-delegation

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

CAS runs work for a named human. Nothing in CAS binds a run to that human today.
A slot reaches the model gateway through the socket proxy of spec 0013, and the
proxy injects one estate credential that every slot shares. The run record names
the lease, not the person who asked for the work.

Two consequences follow. A permission check cannot narrow a run to what the
initiating human may do, because the outbound identity is the platform's, not
theirs. An audit trail cannot answer who asked, because only the executing side
is recorded.

This spec defines the delegated identity a run carries, how the effective
permission is computed from the human and the agent policy, and what the audit
record holds. It states intent. It picks no crate and no provider.

Why now: the operator ruled this spec wave in session on 2026-10-10 [reported: his
own session, this lane, interview answer "All four gaps"]. A ruling is not
command-provable, so it carries the reported tier.

No spec owns delegated identity, which is narrower than saying identity is
absent: spec 0004 lines 190 to 192 already pin `ed25519-dalek`, `jsonschema` and
`governor` with the purpose "identity" at phase 7, each marked as waiting for
the Director of Engineering [verified: read `specs/0004-dependency-and-licence-policy.md` lines 190 to
192]. What is missing is the delegation rule, not the crate list. The absence
claim carries its own control: `git grep -ilE 'keycloak|spire|oidc|svid'
6794d9b -- specs/` exits 1 with no match, while the same pattern matches this
file once it exists, so the pattern works and the absence was real [verified:
both runs]. The estate synthesis names enterprise agent identity and
on-behalf-of delegation as an architectural invariant that no spec owns
[verified: `grep -rilE 'keycloak|spire|oidc|svid' specs/` returned nothing on
`origin/development` at `6794d9b`, 2026-10-10].

In scope: the identity types in `cas-domain`, the delegation bridge in
`cas-engine`, the egress rule in `cas-sandbox`, and the audit record. All behind
a cargo feature, off by default.

Out of scope: deploying or configuring an identity provider, minting anything,
and the mesh join token flow, which is cortex spec 0005 and sits in the Director of Engineering's
identity veto area (spec 0026 D7). REQ-0029-16 and REQ-0029-17 say what the
engine does with whichever provider is reachable; they do not select one, and
the grant and crate choices stay deferred to D8.

## Design

The estate runs Keycloak for OIDC and SPIRE for workload identity [reported: the
estate brief, platform stack section]. This spec treats both as candidate
sources and commits to neither.

### Decisions

| | Decision | Source |
|---|---|---|
| D1 | A run carries a **delegation assertion**: the initiating human subject, the executing agent subject, the lease id, and an expiry. The assertion is the unit the engine passes, the proxy checks and the audit record keeps. | CAS lane, 2026-10-10 |
| D2 | The effective permission of a run is the **intersection** of what the human may do and what the agent policy allows, never the union, and never the agent policy alone. | estate synthesis 4.12 [reported] |
| D3 | A credential lives no longer than the lease that carries it, and a renewed lease renews the credential before the old one expires. Static long-lived tokens are refused in a slot. Spec 0007 owns the lease and its `LeaseToken`; it says nothing about credentials, and the binding is new here. | estate synthesis 4.12 point 3 [reported]; lease and renewal semantics from spec 0007 |
| D3a | The **lease holder** is the agent subject named in the assertion, and it is the subject that holds the `LeaseToken` of spec 0007 for that slot. REQ-0029-12 compares against this subject. | CAS lane, 2026-10-10 |
| D4 | The worker never holds the credential, exactly as spec 0013 D5 requires. This spec **amends** 0013: the proxy no longer takes one gateway key at start through `with_key_file` or `with_bearer_token`, but takes a per-slot credential supplied by the engine for the life of a lease. The invariant that the material stays outside the workspace and that `Debug` redacts it is unchanged. | spec 0013 D1, D5; amended here |
| D4a | The proxy lives in `cas-router` (spec 0013 D1), below the engine. The engine therefore cannot be called by it. `cas-domain` defines an audit sink trait and a credential source trait; `cas-router` depends only on those, and `cas-engine` implements them. No lower layer calls a higher one. | spec 0001 layering; spec 0013 D1 |
| D5 | `cas-domain` holds the assertion type, the subject type and the intersection rule as pure functions. Fetching, exchanging and verifying a token is adapter work in `cas-engine`, and the egress rule is `cas-sandbox`. The domain keeps zero I/O. | spec 0001 layering |
| D6 | The audit record is append-only and names **both** principals. A record that names one is a defect, not a degraded mode. | estate synthesis 4.12 point 4 [reported] |
| D7 | Ratification needs the Director of Engineering. Identity services are his overriding veto, and the operator alone cannot override it: that takes the operator and the CTO together. the operator ruled on 2026-10-10 that the spec may land at `status: planned` and that the Director of Engineering reviews afterwards [reported: his own session, this lane]. Landing a `planned` spec exercises no override, because it commits no code and decides no provider. | second-operator SOP section 3e; executive-override section 5 |
| D8 | The provider choice, the token-exchange grant and any new crate are deferred to an implementation spec, which must pin its dependencies in spec 0004 before first use. | AGENTS.md rule 12 |

### Requirements (EARS)

Every requirement below applies while the delegation feature is on. REQ-0029-11
states what happens when it is off, and it is the only exception.

- **[REQ-0029-01]** (Ubiquitous): The CAS engine shall attach a delegation assertion to every unit of work it admits.
- **[REQ-0029-02]** (Ubiquitous): The delegation assertion shall name the initiating human subject, the executing agent subject, the lease id and an expiry instant.
- **[REQ-0029-03]** (Ubiquitous): The CAS engine shall compute the effective permission of a run as the intersection of the initiating human's permission and the agent policy.
- **[REQ-0029-04]** (Ubiquitous): The CAS domain shall express the assertion, the subjects and the intersection rule without network, filesystem, process or runtime access.
- **[REQ-0029-05]** (Ubiquitous): The audit record of an admitted run shall name both the initiating human subject and the executing agent subject.
- **[REQ-0029-05a]** (Unwanted): IF a request is refused because its assertion is absent or unreadable, THEN the record shall hold the refusal, the slot and the lease id, and shall mark the human subject unknown rather than omit the record.
- **[REQ-0029-06]** (Ubiquitous): The audit record shall be append-only.
- **[REQ-0029-07]** (Event-driven): WHEN the engine grants a lease, the engine shall bind the slot to a credential whose expiry is at or before the lease expiry, and shall set the assertion's expiry at or before the lease expiry.
- **[REQ-0029-08]** (Event-driven): WHEN a credential reaches its expiry during a run, the engine shall stop admitting outbound requests from that slot until the credential is replaced.
- **[REQ-0029-09]** (Event-driven): WHEN the engine replaces a credential, the engine shall keep the assertion's human subject unchanged for the life of the run.
- **[REQ-0029-10]** (State-driven): WHILE a slot holds no valid delegation assertion, the proxy shall refuse every request from that slot, and the slot shall be given no other egress path. The refusal is the proxy's, not the jail's: spec 0013 records that no Landlock ABI mediates `connect` on a pathname socket and that `CLONE_NEWNET` scopes abstract sockets only, so the proxy's allowlist is the boundary. Spec 0011 owns whatever the namespace can enforce.
- **[REQ-0029-11]** (State-driven): WHILE the delegation feature is off, the engine shall run as specified in spec 0013 and shall not require an assertion.
- **[REQ-0029-12]** (Unwanted): IF a request arrives at the proxy with no assertion, an expired assertion, or a subject that does not match the lease holder, THEN the proxy shall answer `403` and record the attempt in the audit record.
- **[REQ-0029-13]** (Unwanted): IF a credential with no expiry, or an expiry beyond the lease, is offered as a slot's delegated credential, THEN the engine shall refuse to start that slot. This does not reach the estate gateway key the proxy holds under spec 0013 D5, which is not a slot credential and which this spec replaces only when the D4 amendment is implemented.
- **[REQ-0029-14]** (Unwanted): IF the audit record cannot be written at admission, THEN the engine shall refuse to admit the unit of work.
- **[REQ-0029-20]** (Unwanted): IF the audit record cannot be written during a run, THEN the engine shall stop admitting outbound requests from that slot and shall mark the run incomplete, rather than continue unrecorded.
- **[REQ-0029-15]** (Unwanted): IF the credential material appears in the worker's open file descriptors, its environment, or any path the worker's Landlock ruleset allows it to read, THEN the slot shall fail its start check.
- **[REQ-0029-18]** (Event-driven): WHEN the engine renews a lease, the engine shall issue the replacement credential before the current one expires, and shall keep the new expiry at or before the renewed lease expiry.
- **[REQ-0029-19]** (Ubiquitous): The CAS domain shall define the audit sink and the credential source as traits, and the engine shall provide the implementations.
- **[REQ-0029-16]** (Optional): WHERE a SPIFFE workload identity is reachable, the engine shall take the agent subject from its SVID.
- **[REQ-0029-17]** (Optional): WHERE only an OIDC provider is reachable, the engine shall obtain the run credential by a token exchange that records the human subject as the delegating party, and shall take the agent subject from the client identity it authenticated as.

## Plan

- [ ] Add the assertion, subject and permission types to `cas-domain`, with the intersection as a pure function. `make smoke` still proves zero I/O.
- [ ] Property test the intersection rule: the result is never wider than either input, and it is empty when either input is empty.
- [ ] Add the expiry rule to the lease path in `cas-engine`, behind a cargo feature that is off by default.
- [ ] Add the assertion check to the socket proxy of spec 0013, with the `403` path and its audit write.
- [ ] Add the egress denial for a slot with no valid assertion in `cas-sandbox`.
- [ ] Add the append-only audit record and the refusal when it cannot be written.
- [ ] Write the negative tests first and watch each fail before the code exists: absent assertion, expired assertion, mismatched subject, credential outliving the lease, unwritable audit record.
- [ ] Open the implementation spec for the provider choice, and pin its dependencies in spec 0004 before first use.

## Test

```bash
cargo test -p cas-domain -j 2 --features delegation delegation && \
  cargo test -p cas-engine -j 2 --features delegation --test delegation && \
  cargo test -p cas-router -j 2 --features delegation --test delegation_proxy && \
  cargo test -p cas-engine -j 2 --test delegation_feature_off
```

Every leg that exercises a delegation rule passes `--features delegation`. A leg
without it compiles the types out and exits 0 having run nothing, which is the
decorative control spec 0002 rejects. The feature-off leg is the one deliberate
exception, and it asserts that a run needs no assertion.

The proxy cases run in `cas-router`, because spec 0013 D1 puts the proxy there.
There is no `cas-sandbox` leg: spec 0013 records that the jail cannot mediate
`connect` on the proxy's socket, so REQ-0029-10 is tested at the proxy.

A pass requires each of these cases to exist and to go red when its own check is
deleted:

| Case | Crate | Requirement |
|---|---|---|
| a request with no assertion is answered `403` | cas-router | REQ-0029-12 |
| a request whose subject differs from the lease holder is answered `403` | cas-router | REQ-0029-12 |
| a slot is refused when the credential outlives its lease | cas-engine | REQ-0029-13 |
| a renewed lease gets its credential before the old expiry | cas-engine | REQ-0029-18 |
| admission is refused when the audit record cannot be written | cas-engine | REQ-0029-14 |
| the intersection is empty when the human's permission is empty | cas-domain | REQ-0029-03 |
| every request is refused with no assertion | cas-router | REQ-0029-10 |
| a refusal is recorded with the human subject unknown | cas-router | REQ-0029-05a |
| with the feature off, a run needs no assertion | cas-engine | REQ-0029-11 |

Requirements 01, 02, 04, 05, 06, 09, 15, 16, 17 and 19 are covered by the
implementation spec's own tests, not by this list; this spec does not claim they
are covered here. A suite in which every case still passes with the checks
removed proves nothing.

## Notes

- Evidence tiers are spec 0002, and this file does not redefine them. Spec 0002
  gives `verified` as "a command ran and the output proves the claim", so an
  operator ruling is `reported`, however direct it was. The synthesis points are
  `reported` as another lane's write-up.
- This spec is `planned` and commits no code. Ratification is the Director of Engineering's, under his
  identity veto. the operator set the timing of any ask to him.
- The proxy already strips what a worker sends and injects its own header
  (spec 0013 D5). REQ-0029-12 adds a check before that injection; it does not
  move the credential closer to the worker.
- Reading: estate synthesis section 4.12
  (`<home>/projects/cerebral/research/SYNTHESIS-ESTATE-AND-SUBSTRATE.md`), spec 0013
  for the proxy, spec 0007 for leases, spec 0022 for custody landing, cortex
  spec 0005 for member join tokens.
