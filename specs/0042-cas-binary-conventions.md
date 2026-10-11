---
status: complete
created: 2026-10-10
tags: [cli, daemon, config, logging, exit-codes, version]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0002-verification-gates-and-evidence, 0014-ebpf-priority-clamping-and-audit-probes, 0025-wave-execution-on-the-custody-chain, 0036-slot-arbitration-across-processes]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0042-cas-binary-conventions

> **Status**: complete, **Priority**: high, **Created**: 2026-10-10

## Overview

The workspace ships two binaries, `uas` and `uas-ebpfd`, and specs 0038 and 0039 add
`uas-operator`. Each spec that adds a flag or an environment variable states its own, and no
spec states the rules every binary follows: where configuration comes from and in what order,
where logs go, what stdout is for, what the exit codes mean, and what `--version` prints
[reported: integrating lane w25:p1 gap analysis, 2026-10-10]. The two binaries already
disagree on one of these.

### Facts

Measured 2026-10-10 on ceres at development `bd31291`, debug build.

- **F1 [verified]:** `uas --version` prints `uas 0.1.0 (bd31291)`. `build.rs` stamps `UAS_BUILD_REV` from `git describe --always --dirty --abbrev=7` and uses `unknown` outside a git checkout (`crates/uas-cli/build.rs` lines 1-32; `main.rs` line 25). No spec names the format; spec 0002 line 40 only has `make smoke` run `uas --version`.
- **F2 [verified]:** `uas-ebpfd` has no `--version`: its `#[command]` sets `name` and `about` only (`crates/uas-ebpf/src/bin/uas-ebpfd.rs` line 14).
- **F3 [verified]:** `uas` logs through `tracing_subscriber::fmt` with `EnvFilter::from_default_env()` to stderr (`main.rs` lines 1083-1086). `uas-ebpfd` uses the same filter with no writer set (`uas-ebpfd.rs` lines 25-27), and `tracing_subscriber::fmt` writes to stdout unless `with_writer` is given [reported: tracing-subscriber 0.3 `fmt` docs].
- **F4 [verified]:** Under `bash`, `uas demo --slot slot-a` writes its JSON report to stdout and nothing to stderr; with `RUST_LOG=debug` one `DEBUG` line goes to stderr. Commands: `./target/debug/uas demo --slot slot-a 2>&1 >/dev/null | wc -l` prints 0; with `RUST_LOG=debug` the same pipe through `grep -c DEBUG` prints 1.
- **F5 [verified]:** No `uas-*` binary reads a configuration file. Command: `grep -rnE 'toml::|\.toml"|config_file|from_reader' crates/*/src --include=*.rs` prints the spec 0023 evaluator path list in `uas-domain`, test code in `uas-cli` (`main.rs` lines 1349-1379, inside the `#[cfg(test)]` module that starts at line 1191), and the wave flow files `uas wave flow` writes (`wave_flow.rs` line 301), which are spec 0025 data, not configuration.
- **F6 [verified]:** `uas` reads `UAS_WAVE_REPO`, `UAS_WAVE_STATE_DIR`, `UAS_WAVE_OPEN_PRS`, `XDG_STATE_HOME` and `HOME` (`crates/uas-cli/src/wave_flow.rs` lines 394-398), each behind a flag (spec 0025 lines 212-215). Spec 0036 adds `UAS_SLOT_DIR`; spec 0041 adds three more.
- **F7 [verified]:** `uas nonsense` exits 2 (clap usage error). `uas wave step` exits 2 when not ready (spec 0025 lines 110 and 139) and `uas wave merge-gate` exits 1 on its listed failures (line 126). `fn main() -> anyhow::Result<()>` exits 1 on `Err`. `uas-ebpfd` returns `ExitCode::FAILURE` (1) on error (`uas-ebpfd.rs` line 32).
- **F8 [verified]:** No `uas-*` crate emits metrics or traces over the network. Command: `grep -rnE 'opentelemetry|otlp|prometheus|metrics::' crates/*/src` prints nothing. The `[observability]` block in `blackwall.toml` configures blackwall's custody runs, not a `uas` binary.

## Design

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Configuration precedence is flag, then environment variable, then built-in default. No `uas-*` binary reads a configuration file of its own; files a spec names as data (saga flows, receipts, state under `XDG_STATE_HOME`) are not configuration, and a library's own credential and endpoint discovery is not either: the kube client of `uas-operator` reads `KUBECONFIG` or the in-cluster service account files, and `gix` reads git's config, as those libraries define (plan author, mesh a10df88d). Every environment variable a binary reads is declared on its clap argument with `env = "UAS_..."`, so `--help` lists it. The only variables without the `UAS_` prefix are the standard `RUST_LOG`, `XDG_STATE_HOME` and `HOME`. | F5 and F6 are already this shape; stating it stops a config file arriving by accident. Files belong to blackwall and saga. | integrating lane w25:p1, 2026-10-10 |
| D2 | Logs go through `tracing` to stderr, filtered by `RUST_LOG`. When `RUST_LOG` is unset the level is `info` for a daemon (`uas-ebpfd`, `uas-operator`) and `warn` for the `uas` CLI. `UAS_LOG_FORMAT=json` switches to the JSON formatter; the default is the human formatter. | A daemon with no default log level is silent in `journalctl`; a CLI with `info` default is noisy on a terminal. | proposed: integrating lane w25:p1, 2026-10-10 |
| D3 | stdout carries the command's result and nothing else: JSON for `demo` and `harness verify`, the flow path `uas wave flow` prints (`wave_flow.rs` line 370), nothing for a daemon. `merge-gate` writes its refusals to stderr (`wave_flow.rs` lines 516-540). A log line never goes to stdout. | A caller that pipes `uas` output parses stdout; F3 shows `uas-ebpfd` already breaks this. | integrating lane w25:p1, 2026-10-10 |
| D4 | Exit codes: 0 success; 1 attempted and failed; 2 nothing was done because the invocation or a precondition was wrong (clap usage, spec 0025 not-ready). A spec that needs another code enumerates it in its own table. A signal terminates with the shell convention (128 plus the signal). | F7 is already this set; the rule is that 2 means "safe to fix and retry", 1 means "look at the failure". | integrating lane w25:p1, 2026-10-10 |
| D5 | Every binary answers `--version` with `<binary> <semver> (<rev>)`, where `<rev>` is `git describe --always --dirty --abbrev=7` at build time or `unknown`. The stamp moves to a workspace build helper so `uas-ebpfd` and `uas-operator` share it. A `-dirty` suffix in a release artifact fails the release gate. | F1 and F2: one binary has it, the other does not. A verifier reads the rev to tie a run to a commit (spec 0002). | integrating lane w25:p1, 2026-10-10 |
| D6 | No `uas-*` binary exports metrics or traces until a spec names the exporter for that binary. `tracing` spans are the seam; an OTLP layer, when wanted, attaches there. Spec 0038 names the exporter for `uas-operator` through the `unsigned-k8s-core` telemetry it adopts (plan author, mesh a10df88d); `uas` and `uas-ebpfd` have none. | F8: the substrate promises none today, and a half-wired exporter is worse than none. Phase 5 (`uas-deck`, spec 0018) is the first consumer on the substrate side. | integrating lane w25:p1, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0042-01]** (Ubiquitous): The `uas-*` binary shall resolve every setting as flag, then environment variable, then built-in default, and shall read no configuration file of its own; a library's own credential discovery, such as the kube client's `KUBECONFIG`, is not the binary's configuration.
- **[REQ-0042-02]** (Ubiquitous): The environment variable a `uas-*` binary reads shall be declared on its clap argument and appear in `--help`, and shall carry the `UAS_` prefix unless it is `RUST_LOG`, `XDG_STATE_HOME` or `HOME`.
- **[REQ-0042-03]** (Ubiquitous): The `uas-*` binary shall write log records to stderr through `tracing`, filtered by `RUST_LOG`.
- **[REQ-0042-04]** (Ubiquitous): The default log filter when `RUST_LOG` is unset shall be `info` for a daemon and `warn` for the `uas` CLI.
- **[REQ-0042-05]** (Event-driven): WHEN `UAS_LOG_FORMAT` is `json`, the binary shall emit one JSON object per log record.
- **[REQ-0042-06]** (Ubiquitous): The `uas-*` binary shall write only the command's result to stdout.
- **[REQ-0042-07]** (Ubiquitous): The `uas-*` binary shall exit 0 on success, 1 when it attempted the work and failed, and 2 when it did nothing because the invocation or a precondition was wrong.
- **[REQ-0042-08]** (Event-driven): WHEN invoked with `--version`, each `uas-*` binary shall print `<binary> <semver> (<rev>)` and exit 0, where `<rev>` is the build-time `git describe --always --dirty --abbrev=7` or `unknown`.
- **[REQ-0042-09]** (Unwanted): IF a release artifact's `<rev>` ends in `-dirty`, THEN the release gate shall fail.
- **[REQ-0042-10]** (Unwanted): IF no spec names an exporter for a given `uas-*` binary, THEN that binary shall open no metrics or trace connection.

## Plan

- [x] `uas-ebpfd`: add `version = VERSION` to the command, set `.with_writer(std::io::stderr)`, default filter `info`.
- [x] `uas`: default filter `warn` when `RUST_LOG` is unset; `UAS_LOG_FORMAT` switch.
- [x] Workspace: move the stamp from `crates/uas-cli/build.rs` to a shared `build-rev` helper used by every binary crate.
- [x] Workspace `Cargo.toml`: add `env` to the `clap` features (line 33 has `derive` only), then `env = "UAS_WAVE_REPO"` on `--repo`, `env = "UAS_WAVE_STATE_DIR"` on `--state-dir`, and a new `--open-prs` flag with `env = "UAS_WAVE_OPEN_PRS"` on `wave step`, since that variable has no argument today (plan 0042 finding 3; ruled by the integrating lane 2026-10-10 as within REQ-0042-02, not a new decision).
- [x] `make release-notes`: refuse a `-dirty` rev.
- [x] Spec 0038: `uas-operator` cites this spec for its flags, logging and exit codes.

## Test

| Requirement | Negative Control |
|---|---|
| REQ-0042-03, 06 | `timeout 1 ./target/debug/uas-ebpfd --socket /tmp/uas-0042.sock >/tmp/uas-0042.out 2>/dev/null; test ! -s /tmp/uas-0042.out`. The daemon may exit early with an error; stdout must still be empty. Expected red today, because `uas-ebpfd` sets no writer (F3). |
| REQ-0042-07 | `uas nonsense` exits 2; `uas wave step` in a repo with no `.blackwall/init-head` exits 2 (spec 0025 line 139); `uas harness verify --receipt /dev/null` exits 1. Goes red if any code changes. |
| REQ-0042-08 | `uas --version` and `uas-ebpfd --version` both match `^[a-z-]+ [0-9]+\.[0-9]+\.[0-9]+ \((unknown|[0-9a-f]{7,}(-dirty)?|v[^ ]+)\)$`. Goes red today for `uas-ebpfd` (F2). |
| REQ-0042-09 | In a throwaway worktree, modify a tracked file and run `make release-notes VERSION=9.9.9`: exit 2 (spec 0042 D4: nothing was done because a precondition was wrong) with `release-notes: refusing a dirty tree`, and `Cargo.toml` unchanged. Goes red against the Makefile before plan 0042 P7, which exits 0 and rewrites `Cargo.toml`. |
| REQ-0042-10 | `grep -rnE 'opentelemetry\|otlp\|prometheus' crates/*/src` prints nothing. Goes red when an exporter crate is added without a spec. |

```bash
./target/debug/uas --version | grep -E '^uas [0-9]+\.[0-9]+\.[0-9]+ \('
bash -c './target/debug/uas nonsense; test $? -eq 2'
cargo nextest run -p uas-cli conventions -j 2
```

## Notes

- Spec 0025 chose exit 2 for a not-ready step before this spec existed; D4 keeps it and gives it the general meaning, so no landed spec changes.
- The `uas demo` command prints no lease token (`main.rs` line 172); D3 keeps that, since a token is not a result.
- Clap's `env` attribute needs the `env` feature of `clap`; the workspace manifest enables `derive` only, and the Plan adds `env`. Spec 0004 has no `clap` row to amend.
- Status moved to in-progress on 2026-10-10: plan 0042's packages P1, P3 to P9 landed in PRs #129 to #132 on unsigned-gg/uas; P2 (the spec 0038 citation) rides in PR #119.
- Status moved to complete on 2026-10-11: every package of plan 0042 landed and the Test block ran green (the three commands exit 0, `conventions` 6 passed, the ebpfd and release-notes rows as expected).
- Plan 0042 (PR #127) found that `UAS_WAVE_OPEN_PRS` is read with no clap argument and that REQ-0042-09 had no Test row; both are fixed in this amendment, 2026-10-10.
- Out of scope: the content of log records and any redaction rule. Spec 0013 D5 covers the one secret the substrate handles.
- Amended 2026-10-11: opening words of the flagged requirement bullets reworded to begin with a standard EARS keyword; no change of meaning (spec 0052 P6).
