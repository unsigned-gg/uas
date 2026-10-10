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

Why now: Christian ruled this spec wave in session on 2026-10-10 [verified: his
own session, this lane]. The estate synthesis names enterprise agent identity and
on-behalf-of delegation as an architectural invariant that no spec owns
[verified: `grep -rilE 'keycloak|spire|oidc|svid' specs/` returned nothing on
`origin/development` at `6794d9b`, 2026-10-10].

In scope: the identity types in `cas-domain`, the delegation bridge in
`cas-engine`, the egress rule in `cas-sandbox`, and the audit record. All behind
a cargo feature, off by default.

Out of scope: choosing the identity provider, minting anything, and the mesh join
token flow, which is cortex spec 0005 and sits in Allen's identity veto area
(spec 0026 D7).

## Design

The estate runs Keycloak for OIDC and SPIRE for workload identity [reported: the
estate brief, platform stack section]. This spec treats both as candidate
sources and commits to neither.

### Decisions

| | Decision | Source |
|---|---|---|
| D1 | A run carries a **delegation assertion**: the initiating human subject, the executing agent subject, the lease id, and an expiry. The assertion is the unit the engine passes, the proxy checks and the audit record keeps. | CAS lane, 2026-10-10 |
| D2 | The effective permission of a run is the **intersection** of what the human may do and what the agent policy allows, never the union, and never the agent policy alone. | estate synthesis 4.12 [reported] |
| D3 | A credential lives no longer than the lease that carries it. Static long-lived tokens are refused in a slot. | estate synthesis 4.12 point 3 [reported]; spec 0007 lease rules |
| D4 | The worker never holds the credential. The proxy pattern of spec 0013 D5 is kept: the slot gets a socket, the proxy holds the material, and `Debug` redacts it. | spec 0013 D5 |
| D5 | `cas-domain` holds the assertion type, the subject type and the intersection rule as pure functions. Fetching, exchanging and verifying a token is adapter work in `cas-engine`, and the egress rule is `cas-sandbox`. The domain keeps zero I/O. | spec 0001 layering |
| D6 | The audit record is append-only and names **both** principals. A record that names one is a defect, not a degraded mode. | estate synthesis 4.12 point 4 [reported] |
| D7 | Ratification needs Allen Hackley. Identity services are his overriding veto, which only Christian and Patrick together can override. Christian ruled on 2026-10-10 that the spec may land at `status: planned` and that Allen reviews afterwards [verified: his own session, this lane]. A `planned` spec commits no code. | second-operator SOP section 3e; executive-override section 5 |
| D8 | The provider choice, the token-exchange grant and any new crate are deferred to an implementation spec, which must pin its dependencies in spec 0004 before first use. | AGENTS.md rule 12 |

### Requirements (EARS)

- **[REQ-0029-01]** (Ubiquitous): The CAS engine shall attach a delegation assertion to every unit of work it admits.
- **[REQ-0029-02]** (Ubiquitous): The delegation assertion shall name the initiating human subject, the executing agent subject, the lease id and an expiry instant.
- **[REQ-0029-03]** (Ubiquitous): The CAS engine shall compute the effective permission of a run as the intersection of the initiating human's permission and the agent policy.
- **[REQ-0029-04]** (Ubiquitous): The CAS domain shall express the assertion, the subjects and the intersection rule without network, filesystem, process or runtime access.
- **[REQ-0029-05]** (Ubiquitous): The audit record of a run shall name both the initiating human subject and the executing agent subject.
- **[REQ-0029-06]** (Ubiquitous): The audit record shall be append-only.
- **[REQ-0029-07]** (Event-driven): WHEN the engine grants a lease, the engine shall bind the slot to a credential whose expiry is at or before the lease expiry.
- **[REQ-0029-08]** (Event-driven): WHEN a credential reaches its expiry during a run, the engine shall stop admitting outbound requests from that slot until the credential is replaced.
- **[REQ-0029-09]** (Event-driven): WHEN the engine replaces a credential, the engine shall keep the assertion's human subject unchanged for the life of the run.
- **[REQ-0029-10]** (State-driven): WHILE a slot holds no valid delegation assertion, the sandbox shall deny that slot outbound network egress.
- **[REQ-0029-11]** (State-driven): WHILE the delegation feature is off, the engine shall run as specified in spec 0013 and shall not require an assertion.
- **[REQ-0029-12]** (Unwanted): IF a request arrives at the proxy with no assertion, an expired assertion, or a subject that does not match the lease holder, THEN the proxy shall answer `403` and record the attempt in the audit record.
- **[REQ-0029-13]** (Unwanted): IF a credential with no expiry, or an expiry beyond the lease, is offered for a slot, THEN the engine shall refuse to start that slot.
- **[REQ-0029-14]** (Unwanted): IF the audit record cannot be written, THEN the engine shall refuse to admit the unit of work.
- **[REQ-0029-15]** (Unwanted): IF a worker process can read the credential material, THEN the slot shall fail its start check.
- **[REQ-0029-16]** (Optional): WHERE a SPIFFE workload identity is reachable, the engine shall take the agent subject from its SVID.
- **[REQ-0029-17]** (Optional): WHERE only an OIDC provider is reachable, the engine shall obtain the run credential by a token exchange that records the human subject as the delegating party.

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
cargo test -p cas-domain -j 2 delegation && \
  cargo test -p cas-engine -j 2 --features delegation --test delegation && \
  cargo test -p cas-sandbox -j 2 --test egress_without_assertion
```

A pass means every requirement above has a test that fails without its code. The
suite must contain, and show as failing when the check is removed: a request with
no assertion answered `403`, a request whose subject differs from the lease holder
answered `403`, a slot refused when the credential outlives its lease, a slot
refused when the audit record cannot be written, and an intersection that is
empty when the human's permission is empty. A suite in which every case passes
with the checks removed proves nothing.

## Notes

- Evidence tiers are spec 0002. In this file, `verified` marks only what a named
  command or Christian's own session produced; the synthesis points are
  `reported`, because they are another lane's write-up.
- This spec is `planned` and commits no code. Ratification is Allen's, under his
  identity veto. Christian set the timing of any ask to him.
- The proxy already strips what a worker sends and injects its own header
  (spec 0013 D5). REQ-0029-12 adds a check before that injection; it does not
  move the credential closer to the worker.
- Reading: estate synthesis section 4.12
  (`~/projects/cerebral/research/SYNTHESIS-ESTATE-AND-SUBSTRATE.md`), spec 0013
  for the proxy, spec 0007 for leases, spec 0022 for custody landing, cortex
  spec 0005 for member join tokens.
