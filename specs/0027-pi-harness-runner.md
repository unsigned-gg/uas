---
status: planned
created: 2026-10-10
tags: [harness, pi, runner, slot, uas-pool, sandbox, rpc]
priority: high
depends_on: [0010-landlock-ruleset-and-abi-probe, 0011-network-namespace-isolation, 0013-llm-gateway-socket-proxy, 0015-slot-pool-cgroup-freezer-and-gitoxide, 0019-pool-runtime-decision, 0026-cortex-pool-member-adapter, 0034-tool-pinning-beyond-cargo]
---

# 0027-pi-harness-runner

## Overview

A slot needs a real agent in it. Spec 0026 D8 reserves this spec for the harness
runner: a real `TaskRunner` that runs a harness in a `uas-pool` slot. Until it
exists, the port has a fake only.

This spec picks the harness and the way UAS drives it. The harness is pi
(`@earendil-works/pi-coding-agent`). UAS starts pi in RPC mode inside the jailed
slot and drives it over stdin and stdout. Pi calls models only through the
slot's LLM socket (spec 0013). The worker never holds a model key.

Why now: pgray proposed a local demo on 2026-10-10. A frontier harness (Claude
Code or agy) drives a local UAS. UAS runs open-model workers in slots, through an
OpenAI-compatible provider behind a local LiteLLM gateway [reported: pgray, in
session, 2026-10-10]. This spec is the worker half of that demo.

In scope: the pi launch, its config, the env, the in-slot shim, the RPC
lifecycle, the role profiles, one UAS tool (`uas_ask`), and the receipt.

Out of scope: the MCP control surface and a `uas serve` daemon. They get their
own spec (D9). Further UAS tools for the worker. Identity and delegation (spec
0029, Allen's veto area).

### Facts

Measured 2026-10-10 on pgray's workstation at development `179694a`, unless noted.

- **F1 [verified]:** Spec 0026 D8 reserves 0027 for the harness runner. Command: `grep -n "0027" specs/0026-cortex-pool-member-adapter.md` prints lines 26, 188 and 209.
- **F2 [verified]:** No crate uses `uas-sandbox` yet. Command: `grep -rln uas_sandbox crates/*/src crates/*/Cargo.toml | grep -v '^crates/uas-sandbox'` prints only `crates/uas-router/src/uds_proxy.rs`, where the name sits in a doc comment.
- **F3 [verified]:** The worker preset grants the workspace every access, read and execute on `/usr`, `/lib`, `/bin` and each toolchain path, read on `/etc`, and write without execute on `/tmp` and three devices. Command: `sed -n 112,140p crates/uas-sandbox/src/ruleset.rs`.
- **F4 [verified]:** The netns hook brings `lo` up, so `127.0.0.1` works inside the slot. Command: `sed -n 60,80p crates/uas-sandbox/src/netns.rs` shows `loopback_up()` on both paths.
- **F5 [verified]:** The LLM proxy allows `POST /v1/chat/completions`, `POST /v1/embeddings` and `GET /v1/models`, drops any worker `Authorization`, adds its own, and forwards over plain TCP. Command: `sed -n 1,40p crates/uas-router/src/uds_proxy.rs`.
- **F6 [verified]:** pi 1.1.0 and Node v26.11.1 are installed. pi is a user-level npm global install under the home directory, outside the preset paths. Commands: `pi --version`, `node --version`, `readlink -f "$(command -v pi)"`.
- **F7 [verified]:** pi 1.1.0 has the flags this spec uses: `--mode rpc`, `--session-dir`, `--no-session`, `--tools`, `--no-extensions`, `--extension`, `--append-system-prompt`, `--no-context-files`, `--approve` and `--no-approve`. Command: `pi --help`.
- **F8 [reported]:** RPC mode reads JSON commands on stdin, one per LF-terminated line, and writes responses and events on stdout. Commands include `prompt`, `steer`, `follow_up`, `abort`, `get_state`, `get_session_stats`, `get_messages` and `bash`. `agent_settled` means pi will not continue on its own. Source: `docs/rpc.md` and `src/modes/rpc/rpc-types.ts`, earendil-works/pi main, read 2026-10-10.
- **F9 [reported]:** In RPC mode an extension's `ctx.ui.input`, `confirm`, `select` and `editor` emit an `extension_ui_request` on stdout and block until an `extension_ui_response` with the same `id` arrives on stdin. Source: `docs/rpc-extension-ui.md`.
- **F10 [reported]:** `PI_CODING_AGENT_DIR` moves the whole agent directory (settings, `models.json`, `mcp.json`, extensions, `SYSTEM.md`). `PI_CODING_AGENT_SESSION_DIR` sets session storage; `--session-dir` overrides it. `PI_OFFLINE`, `PI_SKIP_VERSION_CHECK` and `PI_TELEMETRY` stop pi's own network calls. Source: `docs/configuration.md`, `docs/environment-variables.md`.
- **F11 [reported]:** `--no-approve` skips project `.pi/` settings, MCP servers and extensions. pi still reads a project `sessionDir` before it resolves trust. `AGENTS.md` and `CLAUDE.md` load whatever the trust decision. Source: `docs/security.md`.
- **F12 [reported]:** A `models.json` provider with `api: "openai-completions"` and a `baseUrl` serves an OpenAI-compatible endpoint. `apiKey` must be set even when unused. Source: `docs/models.md`.
- **F13 [reported]:** pi's docs say pi is not a security boundary, and running the whole process in a sandbox is the strongest option. Source: `docs/security.md`, `docs/containerization.md`.

## Design

```
caller (engine TaskRunner, later the MCP daemon)
  |  PiRunner::run(task, role, slot)
  v
uas-pool PiRunner --stdin/stdout JSONL--> pi --mode rpc   (jailed slot)
  |                                         |-- read, bash, edit, write
  |                                         |-- uas extension: uas_ask
  |                                         `-- http://127.0.0.1:<port>/v1
  |                                               |
  |                                       uas slot-shim (in slot)
  |                                               |
  `-- precondition: proxy on <ws>/.ipc/llm.sock <-'
         `--> LiteLLM on the host (holds the key) --> provider
```

### Launch

1. The pool leases a slot with a fresh worktree (spec 0015). A pi run never
   reuses a worktree (D4).
2. The caller has started the spec 0013 proxy on `<ws>/.ipc/llm.sock`. The
   runner checks that the socket exists and refuses otherwise (D2).
3. The runner builds the worker ruleset with two toolchain paths: the pinned
   Node prefix and the pinned pi prefix (D8). The role profile directory and the
   run directory are added read-only and read-write.
4. Inside the jail, the runner starts `uas slot-shim --listen 127.0.0.1:<port>
   --socket <ws>/.ipc/llm.sock`, then pi:
   `pi --mode rpc --no-approve --no-extensions -e <profile>/extensions/uas
   --session-dir <run>/session --model uas/<model> --tools <role tools>`.
5. The env is cleared, then set to the D3 list only.

### Lifecycle

The runner owns three threads per run: one writes commands, one reads stdout,
one drains stderr to the run log (spec 0019 D2: OS threads, no tokio here).

1. Send `get_state`. A response proves pi is up.
2. Send `prompt` with the task text.
3. Relay every event to the caller's observer. Answer `extension_ui_request`
   dialogs through the caller (D7). Ignore fire-and-forget UI requests.
4. Wait for `agent_settled`.
5. Send `get_session_stats` and `get_last_assistant_text`.
6. Close stdin. After a grace period send SIGTERM, then SIGKILL, to the
   process group.

On the lease deadline or a caller cancel, the runner sends `abort`, waits a
grace period, then runs step 6. A pi exit before `agent_settled` is a failed
run.

### Role profiles

A role is a directory outside every workspace. The runner sets
`PI_CODING_AGENT_DIR` to it.

| Path in the profile | Content |
|---|---|
| `models.json` | One provider `uas`, `api: "openai-completions"`, `baseUrl` the shim, `apiKey: "placeholder"` |
| `settings.json` | Defaults only. No packages. |
| `mcp.json` | `{"mcpServers": {}}` |
| `APPEND_SYSTEM.md` | The role prompt |
| `extensions/uas/` | The UAS extension (D7) |

| Role | `--tools` |
|---|---|
| builder | `read,bash,edit,write,grep,find,ls` |
| reviewer | `read,grep,find,ls` |

New roles add a row here first.

### Receipt

The runner returns the exit outcome, the session stats, the last assistant
text, the pool's `WorkerAudit`, and the path of the session JSONL. pi's own
report is `reported`. A gate that the caller runs and that exits 0 is the only
`verified` evidence (AGENTS.md rule 3).

### Requirements (EARS)

- **[REQ-0027-01]** (Ubiquitous): The runner shall start pi only inside the worker ruleset (spec 0010) and a private network namespace (spec 0011).
- **[REQ-0027-02]** (Unwanted): IF `<ws>/.ipc/llm.sock` does not exist when a run starts, THEN the runner shall refuse the run and start no process.
- **[REQ-0027-03]** (Ubiquitous): The runner shall clear the child env and set only the D3 variables.
- **[REQ-0027-04]** (Ubiquitous): No file, argument or variable that the runner passes into the slot shall hold a model key.
- **[REQ-0027-05]** (Ubiquitous): The runner shall pass `--no-approve` and `--no-extensions`, and load only the role profile's UAS extension.
- **[REQ-0027-06]** (Event-driven): WHEN pi emits `agent_settled`, the runner shall collect the session stats and the last assistant text, then end the process.
- **[REQ-0027-07]** (Event-driven): WHEN the lease deadline passes or the caller cancels, the runner shall send `abort`, then end the process group after the grace period.
- **[REQ-0027-08]** (Unwanted): IF pi exits before `agent_settled`, THEN the runner shall report a failed run with the exit code and the stderr tail.
- **[REQ-0027-09]** (Event-driven): WHEN pi emits a dialog `extension_ui_request`, the runner shall pass it to the caller and write the caller's answer, or a cancel on timeout, as an `extension_ui_response` with the same `id`.
- **[REQ-0027-10]** (Ubiquitous): The runner shall keep the session JSONL in the run directory, outside the workspace.

### Decisions

| ID | Decision | Source |
|---|---|---|
| D1 | The worker harness is pi, in RPC mode, with the whole process inside the jail (F13). Codex `exec` was the runner-up. TypeScript and Node in the slot are accepted. | pgray, in session, 2026-10-10: "typescript is fine... pi is more pluggable for us" (Christian may override) |
| D2 | `PiRunner` lives in `uas-pool`, which already launches workers and sits above sandbox and router. The `TaskRunner` adapter of spec 0026 lives in `uas-engine`. The runner does not start the proxy: the proxy needs tokio, and spec 0004 does not approve tokio for `uas-pool`. The caller starts it. | uas lane, 2026-10-10 |
| D3 | Env list: `PATH` (the Node and pi prefixes, `/usr/bin`, `/bin`), `HOME=<run>/home`, `PI_CODING_AGENT_DIR=<profile>`, `PI_CODING_AGENT_SESSION_DIR=<run>/session`, `PI_OFFLINE=1`, `PI_SKIP_VERSION_CHECK=1`, `PI_TELEMETRY=0`, `LANG=C.UTF-8`. Nothing else crosses. | pgray, in session, 2026-10-10: "let's just set env vars well" |
| D4 | A pi run gets a fresh worktree. The F11 project `sessionDir` risk needs a reused worktree, so it does not arise. A run that wants history passes the old session directory on purpose. | pgray, in session, 2026-10-10: "i don't think we recycle sandboxes unless we want the history" |
| D5 | Context files load. pi reads `AGENTS.md` and `CLAUDE.md` from the worktree, so a worker follows the repo's own rules. The runner does not pass `--no-context-files`. | pgray, in session, 2026-10-10: "fine. let 'em load" |
| D6 | Pi reaches the socket through `uas slot-shim`, a hidden `uas-cli` subcommand on std threads. It accepts TCP on `127.0.0.1` inside the slot and copies bytes to `.ipc/llm.sock`. A pi provider that speaks to the socket directly may replace it later. | pgray, in session, 2026-10-10: "shim for now" |
| D7 | The UAS extension has one tool in v1: `uas_ask(question)`. It calls `ctx.ui.input`, which RPC mode turns into a dialog request on stdout (F9). The runner hands it to the caller, so the frontier harness can answer a worker's question. No socket is needed. More tools come in a later spec. | uas lane, 2026-10-10 |
| D8 | Node and pi are pinned in spec 0004, by spec 0034's rules, before first use (AGENTS.md rule 12). The pool installs them into fixed prefixes and passes those prefixes as toolchain paths. | AGENTS.md rule 12; spec 0034 |
| D9 | The MCP control surface and a `uas serve` daemon that hosts the proxy, the pool and this runner get their own spec, at the next free number. | uas lane, 2026-10-10 |

## Plan

- [ ] P0: Spike. Run pi 1.1.0 in RPC mode against a stub OpenAI endpoint, then under `unshare -Urn` with the shim. Turn F8 to F12 into `verified` or correct them. Find out whether pi writes into a read-only `PI_CODING_AGENT_DIR` (N1).
- [ ] P1: Pin Node and pi in spec 0004 (D8).
- [ ] P2: `uas slot-shim` in `uas-cli`, with tests over a temp socket (D6).
- [ ] P3: The builder and reviewer profiles and the UAS extension (D7).
- [ ] P4: `PiRunner` in `uas-pool`: launch, lifecycle and receipt (REQ-0027-01 to 10).
- [ ] P5: The spec 0026 `TaskRunner` adapter in `uas-engine`.
- [ ] P6: Escape-suite rows: a jailed pi reaches no network and reads no key (spec 0012).

## Test

`pi_runner` drives a real pi against a stub model on the host. It skips with a
printed reason when pi or Node is missing.

```bash
cargo test -p uas-pool --test pi_runner -j 2
```

- [ ] A run against the stub writes a file in the worktree and reaches `agent_settled`.
- [ ] A missing `llm.sock` refuses the run and starts no process.
- [ ] The child's `/proc/<pid>/environ` holds exactly the D3 variables.
- [ ] `curl` through pi's `bash` tool to any outside address fails.
- [ ] A deadline mid-run sends `abort`, and no process of the group survives the grace period.
- [ ] A `uas_ask` call reaches the caller, and the caller's answer reaches the model.

## Notes

N1, open: pi may write files into its agent directory (`trust.json`, `mcp.log`,
`auth.json`). If it fails on a read-only profile, the runner copies the profile
into `<run>/agent` per run. P0 decides.

N2, open: Node may need `/proc/self` and more under `/dev` than the preset grants.
P0 records what a jailed `node -e 1` needs.

N3, alternatives: Codex `exec --json` is a single binary with a JSONL stream,
but cannot be steered mid-run. `claude -p` speaks only the Anthropic API. The pi
SDK runs in-process and needs a Node host. A pi on the host with only its tools
in the jail (the Gondolin pattern) leaves pi and its extensions outside the
boundary (F13).

N4: A worker can commit a `.pi/` directory to its branch. `--no-approve` keeps it
inert for pi runs, but a person who opens the branch with pi would load it. Spec
0025 D17's halt set may need `.pi/`. That change belongs to spec 0025.
