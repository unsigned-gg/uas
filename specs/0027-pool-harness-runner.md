---
status: planned
created: 2026-10-10
tags: [pool, harness, runner, sandbox, cgroup, taskrunner]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0004-dependency-and-licence-policy, 0007-lease-rules, 0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation, 0014-ebpf-priority-clamping-and-audit-probes, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0019-pool-runtime-decision, 0023-test-time-compute-and-verifiable-reward, 0026-cortex-pool-member-adapter]
---

# 0027-pool-harness-runner

## Overview

This spec runs an agent harness inside a worker slot. It implements the `TaskRunner` port
of spec 0026. Spec 0026 D8 keeps that port to fakes until the pool runtime is ruled.

Runtime premise, conditional: this spec assumes spec 0019 Option A, OS threads as built.
Christian ruled A on 2026-10-10 and re-opened D2 the same day for a benchmark of Options B
and C [reported, relayed by the integrator, mesh e42efbc2 and ca37e233]. So this spec is
held until he rules again. If D2 rules B or C, revisit the parts the Notes list.

In scope: a `SlotTaskRunner` in `cas-pool`; the sandbox built from existing primitives; the
cgroup freezer with its signal fallback; the priority clamp and the eBPF audit; the
wall-clock cap and post-exit token accounting; the worktree reset after every outcome.

Out of scope: join tokens and member auth (cortex spec 0005, Allen's area, 0026 D7);
rollout policy and metrics (saga, spec 0023 D1); token estimation (CAS never tokenizes,
spec 0023 D2).

Facts, 2026-10-10:

- **F1 [verified, read `crates/cas-engine/src/cortex_pool/port.rs` at 0c2335e]:**
  `TaskRunner` has `start(&mut self, &PoolDelivery) -> Result<(), TaskError>`, `poll(&mut
  self, &str) -> TaskStatus` and `cancel(&mut self, &str)`. `TaskStatus` is `Running`,
  `Succeeded` or `Failed(String)`; `TaskError` is `Payload(String)` or `Recoverable(String)`.
- **F2 [reported, see the Overview]:** spec 0019 D2 is re-opened. The interim choice is spec
  0015 D5: OS threads with `std::sync`.
- **F3 [verified, read `crates/cas-pool/Cargo.toml`]:** `cas-pool` depends on `cas-domain`,
  `cas-ebpf`, `gix`, `rustix`, `serde`, `thiserror` and `tracing`, and not on `cas-sandbox`.
- **F4 [verified, read spec 0001]:** the layering is `deck > acp > engine > pool > sandbox,
  ebpf > router > domain`, so `cas-pool` may use `cas-sandbox` and `cas-ebpf`.
- **F5 [verified, read `crates/cas-sandbox/src/netns.rs:40`, `launcher.rs:28`,
  `ruleset.rs:119`]:** `netns::isolate`, `launcher::jail` and `LandlockRulesetBuilder::worker`.
- **F6 [verified, read `crates/cas-pool/src/launch.rs:143`]:** `spawn_worker` holds the one
  `unsafe` block, with its SAFETY comment; it installs `enter_clamped_group` (process group
  and nice 19).
- **F7 [verified, `ls -ld /sys/fs/cgroup/cas`]:** ceres has no `/sys/fs/cgroup/cas`, and
  uid 1000 cannot write `/sys/fs/cgroup`. Cgroup steps need a delegated base (spec 0015).
- **F8 [verified, read `crates/cas-pool/src/freezer.rs:476`]:** `SignalFreezer` falls back
  to `SIGSTOP` and `SIGCONT`; it misses a child that leaves the process group.
- **F9 [verified, read `crates/cas-pool/src/worktree.rs:183`]:** `WorktreeSlot::reset_clean`
  reverts edits and removes untracked files through `gix`: disk I/O.
- **F10 [verified, read spec 0023 l.136]:** token accounting reads the harness report after
  exit; the cost source is `Reported` or `None`.
- **F11 [verified, read spec 0026]:** D5: the backend dead-letters at `max_deliver`, so the
  adapter keeps no retry count. D6: `drain` happens only at shutdown. D9: a wire or backend
  fault is Nacked, never Termed. D10: the lease `Budget { max_tokens, max_wall_secs }`. D12:
  every call on the adapter thread adds to the others.
- **F12 [verified, read rustix 1.1.5 `src/process/kill.rs:35`, `wait.rs:431,453`]:**
  `kill_process_group`, `waitpid` and `waitpgid` are in the `process` feature, which the
  workspace row enables. No new crate is needed.
- **F13 [asserted]:** the real harness binary, its packaging and its home paths on ceres are
  not known. D1 is Christian's for that reason.
- **F14 [reported, critic note on CER-2794]:** the clamp-proof tests in `cas-ebpf` need a
  nice below 19. A worker slot runs at nice 19, so a harness that runs `make check` in a
  slot would turn them red.

## Design

### Requirements (EARS)

- **Ubiquitous:** `SlotTaskRunner` shall live in `cas-pool` and use OS threads, with no async runtime (spec 0015 D5).
- **Ubiquitous:** `cas-domain` shall stay free of I/O and clocks.
- **Ubiquitous:** No runner step shall block the adapter thread (0026 D12). `start`, `poll` and `cancel` return at once; the kill, the reap and `reset_clean` run on the runner's own thread.
- **Event-driven:** WHEN `start` gets a `PoolDelivery`, the runner shall write the payload to `.cas/task.json` in the slot worktree and spawn the configured harness binary (D1).
- **Event-driven:** WHEN it builds the harness command, the runner shall apply `netns::isolate`, then `launcher::jail`, then `spawn_worker`, and add no `unsafe` block (rule 14).
- **Event-driven:** WHEN the child spawns, the runner shall put it in the delegated cgroup `<cgroup_base>/slot-<id>` if one is configured, else register it with `SignalFreezer` (D5).
- **Event-driven:** WHEN elapsed time reaches `max_wall_secs`, the runner shall send `SIGTERM` to the process group, start the reap on its own thread, and keep `poll` at `Running`.
- **Event-driven:** WHEN `kill_grace_ms` has passed after a `SIGTERM`, the reap thread shall send `SIGKILL` to the process group if any process remains, then reap them all.
- **Event-driven:** WHEN the harness exits, the runner shall read `.cas/receipt.json` for the token count (D4).
- **Event-driven:** WHEN a task ends (success, failure, timeout or cancel), the runner shall run `reset_clean` on its own thread, and `poll` shall report the end only after the reset is done; the slot stays `Leased` until then.
- **Unwanted behavior:** IF the cgroup freeze fails, THEN the runner shall send `SIGSTOP` to the process group.
- **Unwanted behavior:** IF the reported tokens exceed `max_tokens`, THEN `poll` shall return `Failed`, naming the token budget.
- **Unwanted behavior:** IF the harness binary is missing or the sandbox setup fails, THEN `start` shall return `TaskError::Recoverable`, so the adapter Nacks it (a host fault, never a `Term`; 0026 D9).
- **Optional feature:** WHERE `CAS_SLOT_ID` is set, a test that proves a priority clamp shall skip with a printed reason; outside a slot it fails at nice 19 (F14, CER-2794).

### Harness invocation

The wire never supplies an executable path. The host config does (D1):

```rust
pub struct SlotRunnerConfig {
    pub harness_binary: std::path::PathBuf,
    pub default_args: Vec<String>,
    pub base_worktree_dir: std::path::PathBuf,
    pub harness_home_dir: std::path::PathBuf,
    pub cgroup_base: Option<std::path::PathBuf>,
    pub ebpf_socket: std::path::PathBuf,
    pub kill_grace_ms: u64,
}
```

On `start`: the worktree is `<base_worktree_dir>/<slot-id>`; the runner writes the body to
`.cas/task.json` and `delivery_token`, `attempt` and `lease_ms` to `.cas/delivery.json`;
it sets `CAS_SLOT_ID`, `CAS_DELIVERY_TOKEN` and `CAS_TASK_FILE`.

### Sandbox and priority

1. Network (spec 0011): `netns::isolate(&mut cmd)` comes first, so the namespace exists
   before the syscall rules (0011 D1).
2. Filesystem (spec 0010): `LandlockRulesetBuilder::worker(workspace, toolchain)` plus
   read-write `harness_home_dir` and `/tmp/cas-slot-<slot-id>`; `/usr`, `/bin` and `/lib`
   read and execute only. Then `launcher::jail(&mut cmd, ruleset)`.
3. Priority (spec 0014): `spawn_worker` installs `enter_clamped_group`: its own process
   group at nice 19. The harness and all its children run at nice 19.
4. Cgroup and audit (specs 0015, 0020): with a cgroup base, the parent writes the PID to
   `cgroup.procs` and sets `cpu.max`; without one, it registers the PID with `SignalFreezer`.
   It registers the PID with `cas-ebpfd` at `ebpf_socket`; if that fails, it logs the task as
   unattested and goes on.

### Budget, tokens and timeout

1. Wall clock: `poll` compares the elapsed time with `max_wall_secs` (0026 D10). At the cap
   it starts the kill on the runner's thread and returns `Running`; after the reap and the
   reset it returns `Failed("wall-clock budget exhausted")`.
2. Timeout outcome (D3): until spec 0026 is amended, a timeout is `Failed`, which the
   adapter Nacks (0026 D5). The backend dead-letters at `max_deliver` (0026 F2, D5), so a
   timeout cannot loop forever. A Term on a repeated timeout would need a typed timeout
   status (F1) and an exception to 0026 D5; it does not touch 0026 D9 or D11.
3. Tokens (D4): after an exit, `.cas/receipt.json` holds `{ "tokens_used": N }`; the cost
   source is `Reported`. A missing or bad file gives 0 and `None`. A killed task's receipt
   is not read.

### Cancel

`cancel` returns at once. The runner's thread then: thaws the slot (`cgroup.freeze` 0, or
`SIGCONT`); sends `SIGTERM` to the process group; after `kill_grace_ms` sends `SIGKILL`;
reaps with `waitpgid` until none remain; sends `untrack` to `cas-ebpfd`; runs `reset_clean`.

### Outcomes

| Failure | Detected by | Runner | Adapter | Slot |
|---|---|---|---|---|
| Harness binary missing | `spawn` gives `ENOENT` | `TaskError::Recoverable` | `Nack` (D5 delay) | reset, `Warm` |
| Bad payload | JSON check fails | `TaskError::Payload` | `Term` with reason | reset, `Warm` |
| Sandbox setup fails | a `pre_exec` hook fails | `TaskError::Recoverable` | `Nack` | reset, `Warm` |
| Non-zero exit or a signal | `waitpid` | `Failed` | `Nack` | reset, `Warm` |
| Wall-clock cap | elapsed time | kill and reap on its thread, `Failed` | `Nack` (D3) | reset, `Warm` |
| Token budget exceeded | the receipt | `Failed` | `Nack` | reset, `Warm` |
| Success | exit 0 | `Succeeded` | `Ack` | reset, `Warm` |
| Cancel (two `Extend` failures, shutdown) | `cancel` | thaw, kill, reap on its thread | none or D6 `Nack` | reset, `Warm` |
| Cgroup freeze fails | the write fails | `SIGSTOP` the group | none | `Suspended` |
| `reset_clean` fails | its error | the slot leaves service (open: 0026 D6) | `Nack` | out of service |
| `cas-ebpfd` down | `track` fails | logs unattested, goes on | none | `Leased` |

### Crates

- `cas-pool` gains `cas-sandbox` (a path dependency; `pool > sandbox` is allowed).
- `cas-engine` gains the feature `pool-runner = ["cortex-grpc", "dep:cas-pool"]`. It needs
  `cortex-grpc`, because `TaskRunner` lives behind it (0026 D2).
- `crates/cas-engine/src/cortex_pool/slot_runner.rs` bridges `cas_pool::runner::SlotTaskRunner`
  to `TaskRunner`. No new external crate (F12), so spec 0004 gains no row.

### Decisions

| # | Decision | Who, when |
|---|---|---|
| D1 | The host config names `harness_binary`, `default_args` and `harness_home_dir`; the wire never carries a command line. The Landlock allow-list cannot run a real harness until these are named. | **Christian to rule** (F13) |
| D2 | `SlotTaskRunner` lives in `cas-pool`; `cas-engine` bridges it in `cortex_pool/slot_runner.rs` under `pool-runner`, which enables `cortex-grpc`. | Follows spec 0001 and 0026 D2; lane decision, spec designer and cas-engineer, 2026-10-10 (Christian may override) |
| D3 | A wall-clock cap is a `Failed` task: the kill and the reap run on the runner's thread, and `poll` stays non-blocking. A Term on a repeated timeout is proposed, pending a typed timeout status and a 0026 D5 exception. | Lane decision, 2026-10-10 (Christian may override) |
| D4 | Tokens come from `.cas/receipt.json` after exit only, as `Reported`, else `None`; a killed task's receipt is not read. | Follows spec 0023 D2; lane decision, 2026-10-10 |
| D5 | Without a delegated cgroup base, the freezer falls back to `SignalFreezer`, which misses a child that leaves the group. | Follows spec 0015 D4; lane decision, 2026-10-10 |
| D6 | No join-token code in the runner. | Allen's area (0026 D7); not decided here |
| D7 | The runtime premise is Option A as built, held until spec 0019 D2 is ruled again. | **Christian to rule**, as spec 0019 D2 |

## Plan

- [ ] Christian rules spec 0019 D2 again; revisit this spec if it rules B or C (D7).
- [ ] Christian rules D1 (the harness binary and its paths).
- [ ] Amend spec 0026 for the open items in the Notes before the rows they touch are built.
- [ ] `cas-pool`: add `cas-sandbox`; `runner::SlotTaskRunner` with the sandbox order, the
  non-blocking timeout, the cancel order and `reset_clean` on the runner's thread.
- [ ] `cas-engine`: the `pool-runner` feature and `cortex_pool/slot_runner.rs`.
- [ ] `crates/cas-pool/tests/harness_runner.rs` with a fake harness binary.

## Test

```bash
make spec-check
cargo nextest run -p cas-pool --build-jobs 2 --test harness_runner
cargo nextest run -p cas-engine --build-jobs 2 --features pool-runner cortex_pool
make check JOBS=2
```

Kernel steps probe their prerequisite and skip with a printed reason when it is absent
(no user namespaces, no delegated cgroup base). Red-first tests:

- `harness_runner_executes_to_success`: exit 0 with a receipt gives `Succeeded`; the worktree is clean.
- `harness_runner_non_zero_exit_fails`: exit 1 gives `Failed`; the worktree is clean.
- `harness_runner_wall_clock_timeout_nonblocking`: `poll` returns at once; the runner's thread kills and reaps; a later `poll` gives `Failed`.
- `harness_runner_cancels_background_forks`: the harness forks; `cancel` reaps the whole group.
- `harness_runner_landlock_blocks_outside_writes`: a write outside the worktree and home fails with `EACCES` (probes Landlock).
- `harness_runner_netns_blocks_external_network`: a connect out fails with `ENETUNREACH` (probes user and net namespaces).
- `harness_runner_priority_clamped_to_nice_19`: the harness reads nice 19.
- `harness_runner_reads_token_receipt`: the receipt count is `Reported`; no receipt gives `None`.
- `harness_runner_cgroup_freeze_and_thaw`: probes the cgroup base; else it checks the `SignalFreezer` path.
- `harness_runner_reset_before_end_reported`: `poll` reports the end only after `reset_clean`; the slot stays `Leased` until then.
- `harness_runner_missing_binary_is_recoverable`: a missing binary gives `TaskError::Recoverable`, never `Payload`.

## Notes

Open items for spec 0026, one each; none blocks this spec, each blocks the build of its row:
- Timeout Term (D3): needs a typed timeout in `TaskStatus` (F1) and an exception to 0026 D5.
- Missing binary: Nacked here (0026 D9's principle); the full fix is a typed host fault that also stops `Recv`, as D9 does.
- `reset_clean` failure: the slot must leave service, but 0026 D6 drains only at shutdown.
- `TaskStatus::Failed(String)` is untyped: the adapter cannot tell a timeout from another failure.
- 0026 D12 and bed4ae3: the runner's reset and reap must stay off the adapter thread (the EARS row above).
- CER-2794: its brief fails the clamp test at nice 19; with this spec it skips when `CAS_SLOT_ID` is set.

If spec 0019 D2 rules B (tokio): `start` and the hooks keep `std::process::Command`, but
tokio's child reaper would race the `waitpgid` reap, so the reap and the kill move to tokio
process handles or `spawn_blocking`; the reset runs in `spawn_blocking`; the runner's own
thread becomes a task; 0026's sync adapter thread (D12, D13) would need its own review.
If it rules C (heph): each slot becomes an actor; spawn, reap and the reset run in actor
code on a nightly-only runtime that does not build today (spec 0004, benchmark rows).
Unchanged under A, B or C: D1, the sandbox order, the Landlock allow-list, the payload
files, the budget and token rules, the cancel order and the outcome table.

Rejected: a shell command line from the wire (injection); a blocking sleep in `poll` (0026
D12); a freeze at the wall-clock cap (the slot would leak while the broker redelivers).
