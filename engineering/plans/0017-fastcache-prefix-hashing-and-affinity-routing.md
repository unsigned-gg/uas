<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plan: 0017-fastcache-prefix-hashing-and-affinity-routing

Spec: `specs/0017-fastcache-prefix-hashing-and-affinity-routing.md` (status planned, 5 requirements, 6 decisions, 1 Test row). Base: `origin/development` `1bf73c5`, the merge of PR #200.
Planner: `uas-planner`, 2026-10-11. Every path below was read in a worktree cut from that base.
Tiers: `verified` = I ran or read it at the path or command named; `reported` = the spec says it; `asserted` = my judgement.
The spec has no requirement ids. This plan calls them R1 to R5 in the order of the spec's Requirements list: R1 prefix hash, R2 rendezvous choice, R3 `<think>` split, R4 circuit breaker after two failures, R5 failover.

## Findings the builders must know

1. **Half of the spec is built [verified: `ls crates/uas-router/src`, `cargo nextest run -p uas-router -j 2` printed `26 tests run: 26 passed`].** `prefix.rs` (195 lines) holds `PrefixInput`, `ToolDef`, `PrefixHash` and the D1 length-prefixed canonical form. `affinity.rs` (376 lines) holds `AffinityTable` with `ranked` (healthy replicas, best first), `choose` (affinity, then the D5 fallback) and `set_healthy`. Their tests cover permutation, collision, known-answer, 1/N movement and fallback. `multiplexer.rs` and `health.rs` do not exist. `lib.rs` says so in its doc comment.
2. **No Cargo change is needed for `sha2` or `futures-util` [verified: `Cargo.toml` line 54 holds `sha2 = "0.11.0"` and `crates/uas-router/Cargo.toml` already depends on it; spec 0004 lines 202 and 204 pin `sha2` 0.11.0 and `futures-util` 0.3.34 for `uas-router`].** The first two Plan boxes are stale or unneeded. `futures-util` is absent from the root `Cargo.toml`. Nothing in this plan needs a `Stream`: the multiplexer is a synchronous state machine over byte slices. AGENTS.md says to add a dependency only when real code needs it, and `make machete` fails on an unused one. So no package adds `futures-util`. P2 strikes the box.
3. **One pin edit is needed: `serde_json` for `uas-router` [verified: spec 0004 line 197 lists `uas-acp, uas-cli, uas-deck, uas-ebpf, uas-engine, uas-pool` and not `uas-router`; `crates/uas-router/Cargo.toml` has no `serde_json`].** The request parser (R1) and the SSE chunk parser (R3) both read JSON. The crate is pinned and in the workspace table (`Cargo.toml` line 27), so only the "Used by" cell changes. P1 does it. P4 is the first package to add the dependency edge and waits for P1. `pin-check` compares versions only, so a wrong cell would not fail a gate; the cell is still the rule (AGENTS.md rule 12).
4. **`serde_json` object key order depends on feature unification [verified: `cargo tree -e features -i serde_json` prints `serde_json feature "preserve_order"` in the workspace graph].** A `Value` therefore keeps the order of the request, and `to_string` would hash two equal tools with different key order to different values. P4 writes its own key-sorted serialiser for tool JSON. It does not depend on that feature.
5. **The spec Test section has no command that fails before the work [verified].** Its only Test row, `cargo test -p uas-router -j 2`, passes at the base today. P2 replaces it with six rows: one filter per requirement. Each filter matches zero tests today, so `cargo nextest run` exits non-zero [asserted: nextest exits 4 on "no tests to run"; the builder confirms the code is non-zero]. Package Done-whens use those filters.
6. **Names in the spec differ from the tree [verified: `grep -n 'pub struct\|pub fn' crates/uas-router/src/*.rs`].** The spec says `PrefixHasher` and `RendezvousRouter`. The tree has `PrefixInput::hash` and `AffinityTable`. P2 renames the spec. No code rename: both are pub and exported by `lib.rs`, and `uas-sandbox` depends on the crate (`crates/uas-sandbox/Cargo.toml` line 21).
7. **Spec invariant 2 contradicts D5 [verified: spec lines "falls back to round-robin" and D5].** D5 and `choose` use a deterministic index from the hash, not a rotating counter. P2 fixes the invariant wording.
8. **Four points a builder would otherwise decide alone; P2 records them as UAS-lane decisions marked "Christian may override".** (a) Health path: probe `GET /healthz`; a `404` answer is retried once on `/health` in the same probe. D5 says "`/healthz` or `/health`" and the component section says `/healthz`. (b) Restore needs 3 consecutive successes (component section 4); D5 omits it, so P2 copies it into D5. (c) A closing `</think>` with no opening tag: the tag is dropped and the text before it stays clean. (d) Request JSON shape: system parts are `messages[]` whose `role` is `system` or `developer`, with `content` as a string or an array of `{"type":"text","text":...}` parts; a tool's name is `tools[].function.name`.
9. **Nothing forwards requests through the router [verified: `uds_proxy.rs` connects to one fixed `upstream` address, `TcpStream::connect(config.upstream)` near line 572; no file outside `affinity.rs` and `prefix.rs` mentions `AffinityTable`].** The spec Plan lists no wiring into the proxy, and neither does this plan. R5 is closed at the table: when the health monitor marks the chosen replica down, `choose` returns the next one. "Fail over in-flight requests" and the telemetry destination of thought chunks (Cortex audit, Reverie trace in the spec diagram) need a later spec. See Open.
10. **The builder never edits `CHANGELOG.md` [reported: spec 0050 REQ-0050-03].** The integrator writes the one line under `## [Unreleased]` for the wave that adds user-visible router behaviour (AGENTS.md rule 8).
11. **No unsafe, no new test feature.** `tokio` is pinned `=1.53.1` with features `rt-multi-thread`, `macros`, `net`, `time`, `sync`, `io-util` (`Cargo.toml` line 44). It has no `test-util`, so no test can pause the clock. Health tests use a real `TcpListener` on `127.0.0.1:0` and millisecond intervals taken from a config struct.

## Plan items to packages

| Spec 0017 Plan item | Package |
|---|---|
| `sha2` in workspace dependencies | none: already there (finding 2) |
| `futures-util` in workspace dependencies | none: not needed (finding 2); P2 strikes the box |
| `PrefixHasher` with JSON canonicalization | P4 (the JSON reader; the hash exists) |
| `RendezvousRouter` with node health weights | none: `AffinityTable` exists; P2 renames the spec; P8 tests it against the monitor |
| `DualStreamMultiplexer` state machine | P5 (tag splitter), P7 (SSE framing and the type itself) |
| `HealthMonitor` background prober | P6 |
| Unit and property tests: permutations | already in `prefix.rs`; P4 adds the JSON-order case |
| Unit and property tests: 1/N key movement | already in `affinity.rs` (`removing_a_replica_moves_only_its_keys`) |
| Unit and property tests: `<think>` splitter | P5, P7 |
| `specs/README.md` and roadmap artifacts | P9 |
| Not in the spec Plan: error variants, `serde_json` pin row, spec text repairs | P3, P1, P2 |

## Requirement coverage

| Requirement | Closed by |
|---|---|
| R1 serialize and hash prefixes deterministically | P4 adds the request-JSON entry point; the hash and canonical form are built (finding 1) |
| R2 rendezvous replica choice | no package: built and tested at the base (finding 1). P2 gives it a Test row so the claim is checkable |
| R3 split `<think>` from clean tokens | P5 (splitter), P7 (SSE) |
| R4 trip after two consecutive probe failures | P6 |
| R5 fail over to the next healthy replica | P8 (end-to-end with the monitor); `ranked` and `choose` are built |

4 of 5 requirements are closed by a package. R2 needs none. The gaps in finding 9 are not spec requirements and no package closes them.

## Waves

| Wave | Packages | Disjoint because | State after the wave |
|---|---|---|---|
| First | P1, P2, P3 | P1: `specs/0004-dependency-and-licence-policy.md`. P2: `specs/0017-fastcache-prefix-hashing-and-affinity-routing.md`. P3: `crates/uas-router/src/lib.rs`, `crates/uas-router/src/affinity.rs`, `crates/uas-router/src/multiplexer.rs` (new), `crates/uas-router/src/health.rs` (new) | `serde_json` row names `uas-router`. Spec Test rows exist and match the tree. The two module files and the error variants exist |
| Second | P4, P5, P6 | P4: `crates/uas-router/src/prefix.rs`, `crates/uas-router/Cargo.toml`, `Cargo.lock`. P5: `crates/uas-router/src/multiplexer.rs`. P6: `crates/uas-router/src/health.rs`. `lib.rs` is untouched after P3 | JSON requests hash. The splitter and the monitor exist and pass |
| Third | P7, P8 | P7: `crates/uas-router/src/multiplexer.rs`. P8: `crates/uas-router/tests/failover.rs` (new) | SSE stream splits. Failover proven end to end |
| Fourth | P9 | `specs/0017-fastcache-prefix-hashing-and-affinity-routing.md`, `specs/README.md`, `crates/uas-router/README.md` | Spec status moves |

Order that keeps gates green: P1 before P4, because P4 adds the dependency edge the cell describes. P3 before P5, P6 and P7, because they fill files P3 creates and use the variants P3 adds. P5 before P7, because P7 builds on the splitter. P6 before P8. P2 before every Done-when below, because the filters are the rows P2 writes; a builder may run the filter without waiting for the spec row.

The spec status moves to `in-progress` with the first landed wave: the integrator changes the frontmatter, the body status line and the registry row in `specs/README.md` in one commit. P9 does the move to `complete`.

## Packages

### P1: spec 0004 `serde_json` row names uas-router (wave 1)
- **Persona:** `uas-engineer` (spec amendment).
- **Files:** `specs/0004-dependency-and-licence-policy.md` (exists). Line 197 is the table row; line 150 is the matching prose entry. About 4 changed lines.
- **Change:** add `uas-router` to the "Used by" cell of the `serde_json` row (line 197) and to the prose entry (line 150), with the text "request-body and SSE chunk parsing in uas-router, spec 0017 (UAS lane amendment, Christian corrects it in place)". Leave the version `1.0.151` and the root requirement `"1"` unchanged. Add nothing for `futures-util`: its row already names `uas-router`.
- **Done-when:** `test "$(grep -c '^| .serde_json. .*uas-router' specs/0004-dependency-and-licence-policy.md)" = 1 && make spec-check JOBS=2 && make pin-check && make typos` exits 0.
- **Red-first:** the first count prints 0 before the edit. After green, delete `uas-router` from the cell in a scratch copy and watch the count print 0.

### P2: amend spec 0017 so its Test rows and wording match the tree (wave 1)
- **Persona:** `uas-engineer` (spec amendment).
- **Files:** `specs/0017-fastcache-prefix-hashing-and-affinity-routing.md` (exists, about 190 lines). About 90 changed lines.
- **Change:**
  (a) Number the five requirements `[REQ-0017-01]` to `[REQ-0017-05]` in the form spec 0050 uses.
  (b) Replace the Test section with a table. Rows: REQ-0017-01 `cargo nextest run -p uas-router prefix:: -j 2`; REQ-0017-02 `cargo nextest run -p uas-router affinity:: -j 2`; REQ-0017-03 `cargo nextest run -p uas-router multiplexer:: -j 2`; REQ-0017-04 `cargo nextest run -p uas-router health:: -j 2`; REQ-0017-05 `cargo nextest run -p uas-router --test failover -j 2`; all `cargo test -p uas-router -j 2` (the old row, kept as the last row).
  (c) Plan section: tick `sha2`; replace the `futures-util` box with a note "not needed, no `Stream` in the design (plan 0017 finding 2)"; rename `PrefixHasher` to `PrefixInput::hash` and `RendezvousRouter` to `AffinityTable`; tick the two built Plan items; leave the rest open.
  (d) Invariant 2: replace "round-robin over every registered replica" with the D5 wording (deterministic index from the hash).
  (e) D5: add the restore rule (3 consecutive successes) and the probe path rule (finding 8a, 8b), marked "UAS lane, 2026-10-11, Christian may override". Add the stray-closing-tag rule (8c) to D4 and the request shape (8d) to the Prefix Canonicalizer component.
  (f) Add one sentence to Notes: the router does not yet forward requests; wiring the table into the proxy and the telemetry sink need a follow-up spec (finding 9).
- **Done-when:** `make spec-check JOBS=2 && make typos && test "$(grep -c 'REQ-0017-0[1-5]' specs/0017-fastcache-prefix-hashing-and-affinity-routing.md)" -ge 10` exits 0. The count is the five requirement lines plus the five Test rows, at least ten [asserted: the builder adjusts the number to the file it writes].
- **Red-first:** the count prints 0 before the edit. After green, remove one requirement id in a scratch copy and run `make spec-check JOBS=2`; if it stays green, the count leg is the control and must go red.

### P3: error variants and empty module files (wave 1)
- **Persona:** `uas-builder`.
- **Files:** `crates/uas-router/src/affinity.rs` (exists; `RouterError` at line 22), `crates/uas-router/src/lib.rs` (exists, 24 lines), `crates/uas-router/src/multiplexer.rs` (new), `crates/uas-router/src/health.rs` (new). About 60 changed lines.
- **Change:** add three variants to `RouterError` after `InvalidReplicaId`: `InvalidPayload` ("malformed request payload", carries a `String` reason), `UpstreamTimeout` ("upstream did not respond in time"), `StreamTerminated` ("upstream closed the stream mid-completion"). Keep `PartialEq, Eq`. Create `multiplexer.rs` and `health.rs`, each with the Apache-2.0 OR MIT SPDX line 1, a `//!` doc comment naming the spec and decision, and nothing else. In `lib.rs` add `pub mod health;` and `pub mod multiplexer;` in alphabetical order, and drop "The token multiplexer and the health probe are still to come." from the doc comment only when P5 to P7 land (P9 does that, not this package). Re-export nothing yet.
- **Done-when:** `cargo nextest run -p uas-router router_error -j 2 && make fmt-check clippy JOBS=2` exits 0. The new test `router_error_display_names_each_variant` asserts the `to_string()` of all five variants.
- **Red-first:** write the test first and watch the compile fail on the missing variants. After green, change the `StreamTerminated` text in a scratch copy and watch the test fail.

### P4: build a PrefixInput from a chat-completions request body (wave 2, needs P1)
- **Persona:** `uas-builder`.
- **Files:** `crates/uas-router/src/prefix.rs` (exists; add after `PrefixInput::hash`, tests in the existing `mod tests`), `crates/uas-router/Cargo.toml` (add `serde_json = { workspace = true }`), `Cargo.lock` (the `uas-router` entry gains `serde_json`; no version moves). About 200 changed lines.
- **Change:** `impl PrefixInput { pub fn from_request(body: &[u8]) -> Result<Self, RouterError> }`. Parse with `serde_json::from_slice`; any parse failure, a non-object root, a missing or non-array `messages`, or a `tools` entry with no string `function.name` returns `RouterError::InvalidPayload(reason)`. Per finding 8d: collect `system` from each message whose `role` is `system` or `developer`, in request order, `content` read as a string or as the concatenation of the `text` parts of an array; non-text parts are skipped. `tools` absent means no tools. Each `ToolDef.json` is the tool value serialised with object keys sorted at every depth by a private recursive function (finding 4), compact, no whitespace. Unit tests: (1) the known-answer request from the existing `prefix_hash_known_vector` input expressed as JSON hashes to `7f8091d619689020318e8e928b216c76` [verified: the vector in `prefix.rs`]; (2) the same request with tools reordered, and again with keys inside a tool reordered, hashes the same; (3) a `developer` message counts as system and a `user` message does not; (4) array content with a non-text part; (5) each `InvalidPayload` case; (6) a request with a changed system text hashes differently.
- **Done-when:** `cargo nextest run -p uas-router prefix:: -j 2 && make fmt-check clippy machete JOBS=2` exits 0. Spec row REQ-0017-01.
- **Red-first:** write the six tests against `from_request` returning `Ok(PrefixInput::default())` and watch tests 1, 3 and 4 fail. After green, replace the key-sorting function with `serde_json::to_string(&v)` and watch the key-order case of test 2 fail (finding 4 says this holds while `preserve_order` is on; if it does not fail, record that in the PR and keep the sorter). Remove `serde_json` from `Cargo.toml` and watch the build fail.

### P5: `<think>` tag splitter with a fixed window (wave 2, needs P3)
- **Persona:** `uas-builder`.
- **Files:** `crates/uas-router/src/multiplexer.rs` (created by P3). About 250 changed lines, tests included.
- **Change:** `pub enum Phase { Initial, Thinking, Completion }` and `pub struct ThinkSplitter` holding the phase and a `[u8; 7]` window with a length (7 is `len("</think>") - 1`). `pub enum Piece<'a> { Clean(&'a str), Thought(&'a str) }`. `pub fn push<'a>(&mut self, text: &'a str, emit: &mut impl FnMut(Piece<'_>))` and `pub fn finish(&mut self, emit: &mut impl FnMut(Piece<'_>))`. Rules: `<think>` (any phase) switches to `Thinking`; `</think>` in `Thinking` switches to `Completion`; `</think>` outside `Thinking` is dropped (finding 8c); neither tag is ever emitted. A trailing slice that is a prefix of the tag the current phase can still see is held in the window and joined to the next `push`; if the next text shows it was not a tag, the held bytes are emitted first, in the phase they were read in. `finish` emits held bytes: as `Thought` in `Thinking`, as `Clean` otherwise. No `Vec`, `String` or `Box` in the type or the methods (D6); the held bytes are ASCII so the window slice is valid `str` [asserted: a prefix of an ASCII tag is ASCII; the builder uses `str::from_utf8` on the window and a unit test pins it]. Tests: whole tags in one push; every split point of `a<think>b</think>c` across two pushes and across three pushes (a loop over all byte positions) yields the same concatenated clean text `ac` and thought text `b`; a lone `<` or the text `<thin` at the end then `finish` is emitted as clean; unclosed `<think>` text is emitted as thought, never clean; `</think>` with no opener is dropped; two think blocks in one stream; multibyte text around a tag; empty push.
- **Done-when:** `cargo nextest run -p uas-router multiplexer:: -j 2 && make fmt-check clippy JOBS=2` exits 0. Spec row REQ-0017-03. Also `! grep -nE 'Vec|String|Box|to_string|format!' crates/uas-router/src/multiplexer.rs` over the non-test lines: the builder runs it with `sed '/#\[cfg(test)\]/,$d'` and records the output (empty means D6 holds).
- **Red-first:** write the split-point loop first against a splitter that emits everything as clean and watch it fail. After green, shrink the window to 6 and watch the split loop fail on `</think>`. Add a `String` to the splitter and watch the grep leg print the line.

### P6: health monitor with a pure counter and a TCP probe (wave 2, needs P3)
- **Persona:** `uas-builder`.
- **Files:** `crates/uas-router/src/health.rs` (created by P3). About 330 changed lines, tests included. Split off the probe into its own package if the count passes 400.
- **Change:** (a) `pub struct HealthCounter` with `fn record(&mut self, ok: bool) -> Option<bool>`: 2 consecutive failures flip a healthy replica to unhealthy and return `Some(false)`, 3 consecutive successes flip an unhealthy one back and return `Some(true)`, anything else returns `None`; a success resets the failure run and the reverse. New counters start healthy (the table inserts replicas healthy, `affinity.rs` `insert`). (b) `pub struct HealthConfig { interval: Duration, timeout: Duration }` with `Default` 5000 ms and 1000 ms. (c) `pub async fn probe(addr: SocketAddr, cfg: &HealthConfig) -> bool`: connect, `GET /healthz HTTP/1.1` with `Host`, `Connection: close`; a `2xx` status is success; a `404` retries once on `/health`; connect, write and read together are bounded by `timeout`; any error or other status is failure. Raw `tokio::net::TcpStream` with `io-util`, as `uds_proxy.rs` does; no HTTP client crate. (d) `pub struct HealthMonitor` with `pub fn spawn(table: Arc<Mutex<AffinityTable>>, replicas: Vec<(ReplicaId, SocketAddr)>, cfg: HealthConfig) -> HealthHandle` that spawns one tokio task: every `interval`, probe all replicas, feed each result to its counter, and call `set_healthy` on a flip. `HealthHandle::stop` aborts the task. Use `std::sync::Mutex`, held only for `set_healthy`, never across an await. Tests (real `TcpListener` on `127.0.0.1:0`, intervals of 20 ms): counter table of sequences (F, FF flips, FFS resets, SSS restores, S S F S S S); `probe` true on `200`, false on `503`, false on a closed port, false on a server that never answers (timeout), `404` on `/healthz` then `200` on `/health` is true; monitor marks a replica unhealthy after its server starts returning `503` and healthy after it returns `200` again, waiting with a bounded poll loop, not a fixed sleep.
- **Done-when:** `cargo nextest run -p uas-router health:: -j 2 && make fmt-check clippy JOBS=2` exits 0. Spec row REQ-0017-04.
- **Red-first:** write the counter table first against a stub that returns `None` and watch it fail. After green, change the failure threshold from 2 to 3 in a scratch copy and watch the `FF` case and the monitor test fail. Point the timeout at 0 ms and watch the slow-server and success tests change.

### P7: SSE framing around the splitter (wave 3, needs P4, P5)
- **Persona:** `uas-builder`.
- **Files:** `crates/uas-router/src/multiplexer.rs` (extends P5), `crates/uas-router/src/lib.rs` (re-exports only: `DualStreamMultiplexer`, `ThinkSplitter`). About 300 changed lines.
- **Change:** `pub struct DualStreamMultiplexer` with `fn feed(&mut self, bytes: &[u8], out: &mut impl FnMut(Output))` and `fn finish(&mut self, out: &mut impl FnMut(Output)) -> Result<(), RouterError>`; `pub enum Output { Client(Vec<u8>), Thought(String) }`. It keeps a line buffer for a partial SSE line (allocation allowed here; D6 covers tag detection only), splits on `\n`, and handles `\r\n`. For each `data: {json}` line it parses with `serde_json`, reads `choices[0].delta.content`, runs it through the splitter, and emits one `Client` line with `content` replaced by the clean text (re-serialised) and one `Thought` per thought piece. If the clean text is empty and no other delta field remains, emit no client line; other delta fields (`role`, `tool_calls`, `reasoning_content`) pass through untouched. `data: [DONE]`, comments, blank lines and non-JSON data lines pass through as `Client`. `finish` flushes the splitter and returns `Err(StreamTerminated)` if `[DONE]` was never seen. Tests: a canned stream with a think block split across three events yields clean events whose concatenated `content` has no tag and no thought text, and thought events with the thought text; the stream cut in the middle of a line and in the middle of a tag at every byte offset gives the same output; `[DONE]` passes; EOF without `[DONE]` is `StreamTerminated`; a tool-call delta passes unchanged; invariant 3 as a property over the split offsets: no `Client` bytes contain `<think>` or `</think>`.
- **Done-when:** `cargo nextest run -p uas-router multiplexer:: -j 2 && make fmt-check clippy machete JOBS=2` exits 0. Spec row REQ-0017-03.
- **Red-first:** write the every-byte-offset test first against a pass-through `feed` and watch it fail. After green, make `finish` return `Ok(())` always and watch the `StreamTerminated` test fail. Skip the splitter for the client path and watch the no-tag property fail.

### P8: failover test, monitor to table to choice (wave 3, needs P6)
- **Persona:** `uas-builder`.
- **Files:** `crates/uas-router/tests/failover.rs` (new). About 120 changed lines.
- **Change:** an integration test using the public API only. Three fake replicas are `TcpListener` tasks answering `/healthz` with `200` or `503` on a switch. Build an `AffinityTable` with three `ReplicaId`s, share it with `HealthMonitor::spawn` at 20 ms. Pick a `PrefixHash` from `PrefixInput::from_request` on a fixed JSON body. Assert: `choose` returns the HRW winner while all are up; after the winner's server turns to `503` and two probe intervals pass (bounded poll, 5 s cap), `choose` returns `ranked(hash)[1]` as `ReplicaChoice::Affinity`; after all three are down, `choose` returns `ReplicaChoice::Fallback`; after the first recovers (3 successes), `choose` returns it as `Affinity` again. Keys that did not map to the failed replica keep their replica throughout (R2 minimal disruption, checked on 50 hashes).
- **Done-when:** `cargo nextest run -p uas-router --test failover -j 2 && make fmt-check clippy JOBS=2` exits 0. Spec row REQ-0017-05.
- **Red-first:** before the file exists the command fails ("no test target"). After green, change the monitor threshold to 3 in P6's code in a scratch copy and watch the 2-interval assertion time out.

### P9: spec status move and stale text (wave 4, needs P1 to P8)
- **Persona:** `uas-engineer`.
- **Files:** `specs/0017-fastcache-prefix-hashing-and-affinity-routing.md` (frontmatter `status`, body status line if any, Plan ticks), `specs/README.md` (registry row, line 25), `crates/uas-router/README.md` (the Status row says "Placeholder. No code yet." and "Will depend on | uas-domain"; both are false at the base [verified]), `crates/uas-router/src/lib.rs` (delete "The token multiplexer and the health probe are still to come. See `README.md` in this crate." and add the module list). About 40 changed lines.
- **Change:** status `planned` to `complete` in the frontmatter and the registry row in one commit; tick every Plan box the packages closed; README Status row states what exists (prefix hash, rendezvous choice, think splitter, health monitor) and what does not (proxy wiring, telemetry sink, finding 9). If a package is not landed, status becomes `in-progress` and the file lists what is open.
- **Done-when:** `make spec-check JOBS=2 && grep -c '^status: complete' specs/0017-fastcache-prefix-hashing-and-affinity-routing.md | grep -qx 1 && grep -q '0017-fastcache-prefix-hashing-and-affinity-routing.md) | FastCache prefix hashing and affinity routing | complete' specs/README.md && cargo test -p uas-router -j 2` exits 0 on `development`.
- **Red-first:** before the edit the second and third legs fail (status is `planned`). After green, set the registry row back to `planned` in a scratch copy and watch the third leg fail.

## Open

1. **Request forwarding and the telemetry sink are not specified [verified: finding 9].** The router chooses a replica and splits a stream; nothing calls either. The spec diagram shows thought traces going to "Cortex Audit & Reverie Trace", and `uds_proxy.rs` has one fixed upstream. The integrator decides whether to file a follow-up spec. This plan does not close "fail over in-flight requests" or "zero-interruption" beyond the table.
2. **`nextest` exit code for an empty filter [asserted].** Done-whens rely on it being non-zero. P4 to P7 confirm it on the red-first run.
3. **The four finding-8 decisions are planner choices.** They are written into spec 0017 by P2 as "Christian may override". If any is overruled, P4 (8d), P5 (8c) or P6 (8a, 8b) changes.
4. **Line counts are estimates [asserted].** P6 is nearest to the 400-line limit. The builder splits the probe from the monitor if the diff passes 400.
5. **`make spec-check` and `make typos` on this file** ran in the planning worktree; their results are in the planner's return message, not here.
