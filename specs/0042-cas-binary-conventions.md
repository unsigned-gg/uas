---
status: planned
created: 2026-10-10
tags: [cli, daemon, config, logging, exit-codes, version]
priority: high
depends_on: [0001-cas-architecture-and-crate-layout, 0002-verification-gates-and-evidence, 0014-ebpf-priority-clamping-and-audit-probes, 0025-wave-execution-on-the-custody-chain, 0036-slot-arbitration-across-processes]
---

# 0042-cas-binary-conventions

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

The workspace ships two binaries, `cas` and `cas-ebpfd`, and specs 0038 and 0039 add
`uas-operator`. Each spec that adds a flag or an environment variable states its own, and no
spec states the rules every binary follows: where configuration comes from and in what order,
where logs go, what stdout is for, what the exit codes mean, and what `--version` prints
[reported: integrating lane w25:p1 gap analysis, 2026-10-10]. The two binaries already
disagree on one of these.

### Facts

Measured 2026-10-10 on ceres at development `bd31291`, debug build.

- **F1 [verified]:** `cas --version` prints `cas 0.1.0 (bd31291)`. `build.rs` stamps `CAS_BUILD_REV` from `git describe --always --dirty --abbrev=7` and uses `unknown` outside a git checkout (`crates/cas-cli/build.rs` lines 1-32; `main.rs` line 25). No spec names the format; spec 0002 line 40 only has `make smoke` run `cas --version`.
- **F2 [verified]:** `cas-ebpfd` has no `--version`: its `#[command]` sets `name` and `about` only (`crates/cas-ebpf/src/bin/cas-ebpfd.rs` line 14).
- **F3 [verified]:** `cas` logs through `tracing_subscriber::fmt` with `EnvFilter::from_default_env()` to stderr (`main.rs` lines 1083-1086). `cas-ebpfd` uses the same filter with no writer set (`cas-ebpfd.rs` lines 25-27), and `tracing_subscriber::fmt` writes to stdout unless `with_writer` is given [reported: tracing-subscriber 0.3 `fmt` docs].
- **F4 [verified]:** Under `bash`, `cas demo --slot slot-a` writes its JSON report to stdout and nothing to stderr; with `RUST_LOG=debug` one `DEBUG` line goes to stderr. Commands: `./target/debug/cas demo --slot slot-a 2>&1 >/dev/null | wc -l` prints 0; with `RUST_LOG=debug` the same pipe through `grep -c DEBUG` prints 1.
- **F5 [verified]:** No `cas-*` binary reads a configuration file. Command: `grep -rnE 'toml::|\.toml"|config_file|from_reader' crates/*/src --include=*.rs` prints the spec 0023 evaluator path list in `cas-domain`, test code in `cas-cli` (`main.rs` lines 1349-1379, inside the `#[cfg(test)]` module that starts at line 1191), and the wave flow files `cas wave flow` writes (`wave_flow.rs` line 301), which are spec 0025 data, not configuration.
- **F6 [verified]:** `cas` reads `CAS_WAVE_REPO`, `CAS_WAVE_STATE_DIR`, `CAS_WAVE_OPEN_PRS`, `XDG_STATE_HOME` and `HOME` (`crates/cas-cli/src/wave_flow.rs` lines 394-398), each behind a flag (spec 0025 lines 212-215). Spec 0036 adds `CAS_SLOT_DIR`; spec 0041 adds three more.
- **F7 [verified]:** `cas nonsense` exits 2 (clap usage error). `cas wave step` exits 2 when not ready (spec 0025 lines 110 and 139) and `cas wave merge-gate` exits 1 on its listed failures (line 126). `fn main() -> anyhow::Result<()>` exits 1 on `Err`. `cas-ebpfd` returns `ExitCode::FAILURE` (1) on error (`cas-ebpfd.rs` line 32).
- **F8 [verified]:** No `cas-*` crate emits metrics or traces over the network. Command: `grep -rnE 'opentelemetry|otlp|prometheus|metrics::' crates/*/src` prints nothing. The `[observability]` block in `blackwall.toml` configures blackwall's custody runs, not a `cas` binary.

## Design

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Configuration precedence is flag, then environment variable, then built-in default. No `cas-*` binary reads a configuration file; files a spec names as data (saga flows, receipts, state under `XDG_STATE_HOME`) are not configuration. Every environment variable a binary reads is declared on its clap argument with `env = "CAS_..."`, so `--help` lists it. The only variables without the `CAS_` prefix are the standard `RUST_LOG`, `XDG_STATE_HOME` and `HOME`. | F5 and F6 are already this shape; stating it stops a config file arriving by accident. Files belong to blackwall and saga. | integrating lane w25:p1, 2026-10-10 |
| D2 | Logs go through `tracing` to stderr, filtered by `RUST_LOG`. When `RUST_LOG` is unset the level is `info` for a daemon (`cas-ebpfd`, `uas-operator`) and `warn` for the `cas` CLI. `CAS_LOG_FORMAT=json` switches to the JSON formatter; the default is the human formatter. | A daemon with no default log level is silent in `journalctl`; a CLI with `info` default is noisy on a terminal. | proposed: integrating lane w25:p1, 2026-10-10 |
| D3 | stdout carries the command's result and nothing else: JSON for `demo` and `harness verify`, the flow path `cas wave flow` prints (`wave_flow.rs` line 370), nothing for a daemon. `merge-gate` writes its refusals to stderr (`wave_flow.rs` lines 516-540). A log line never goes to stdout. | A caller that pipes `cas` output parses stdout; F3 shows `cas-ebpfd` already breaks this. | integrating lane w25:p1, 2026-10-10 |
| D4 | Exit codes: 0 success; 1 attempted and failed; 2 nothing was done because the invocation or a precondition was wrong (clap usage, spec 0025 not-ready). A spec that needs another code enumerates it in its own table. A signal terminates with the shell convention (128 plus the signal). | F7 is already this set; the rule is that 2 means "safe to fix and retry", 1 means "look at the failure". | integrating lane w25:p1, 2026-10-10 |
| D5 | Every binary answers `--version` with `<binary> <semver> (<rev>)`, where `<rev>` is `git describe --always --dirty --abbrev=7` at build time or `unknown`. The stamp moves to a workspace build helper so `cas-ebpfd` and `uas-operator` share it. A `-dirty` suffix in a release artifact fails the release gate. | F1 and F2: one binary has it, the other does not. A verifier reads the rev to tie a run to a commit (spec 0002). | integrating lane w25:p1, 2026-10-10 |
| D6 | No `cas-*` binary exports metrics or traces until a spec adds the exporter. `tracing` spans are the seam; an OTLP layer, when wanted, attaches there. | F8: the substrate promises none today, and a half-wired exporter is worse than none. Phase 5 (`cas-deck`, spec 0018) is the first consumer. | integrating lane w25:p1, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0042-01]** (Ubiquitous): Each `cas-*` binary shall resolve every setting as flag, then environment variable, then built-in default, and shall read no configuration file.
- **[REQ-0042-02]** (Ubiquitous): Each environment variable a `cas-*` binary reads shall be declared on its clap argument and appear in `--help`, and shall carry the `CAS_` prefix unless it is `RUST_LOG`, `XDG_STATE_HOME` or `HOME`.
- **[REQ-0042-03]** (Ubiquitous): Each `cas-*` binary shall write log records to stderr through `tracing`, filtered by `RUST_LOG`.
- **[REQ-0042-04]** (Ubiquitous): The default log filter when `RUST_LOG` is unset shall be `info` for a daemon and `warn` for the `cas` CLI.
- **[REQ-0042-05]** (Event-driven): WHEN `CAS_LOG_FORMAT` is `json`, the binary shall emit one JSON object per log record.
- **[REQ-0042-06]** (Ubiquitous): Each `cas-*` binary shall write only the command's result to stdout.
- **[REQ-0042-07]** (Ubiquitous): Each `cas-*` binary shall exit 0 on success, 1 when it attempted the work and failed, and 2 when it did nothing because the invocation or a precondition was wrong.
- **[REQ-0042-08]** (Event-driven): WHEN invoked with `--version`, each `cas-*` binary shall print `<binary> <semver> (<rev>)` and exit 0, where `<rev>` is the build-time `git describe --always --dirty --abbrev=7` or `unknown`.
- **[REQ-0042-09]** (Unwanted): IF a release artifact's `<rev>` ends in `-dirty`, THEN the release gate shall fail.
- **[REQ-0042-10]** (Unwanted): IF no spec names an exporter, THEN a `cas-*` binary shall open no metrics or trace connection.

## Plan

- [ ] `cas-ebpfd`: add `version = VERSION` to the command, set `.with_writer(std::io::stderr)`, default filter `info`.
- [ ] `cas`: default filter `warn` when `RUST_LOG` is unset; `CAS_LOG_FORMAT` switch.
- [ ] Workspace: move the stamp from `crates/cas-cli/build.rs` to a shared `build-rev` helper used by every binary crate.
- [ ] Workspace `Cargo.toml`: add `env` to the `clap` features (line 33 has `derive` only), then `env = "CAS_WAVE_..."` on the three wave arguments, so `--help` lists them.
- [ ] `make release-notes`: refuse a `-dirty` rev.
- [ ] Spec 0038: `uas-operator` cites this spec for its flags, logging and exit codes.

## Test

| Requirement | Negative Control |
|---|---|
| REQ-0042-03, 06 | `timeout 1 ./target/debug/cas-ebpfd --socket /tmp/cas-0042.sock >/tmp/cas-0042.out 2>/dev/null; test ! -s /tmp/cas-0042.out`. The daemon may exit early with an error; stdout must still be empty. Expected red today, because `cas-ebpfd` sets no writer (F3). |
| REQ-0042-07 | `cas nonsense` exits 2; `cas wave step` in a repo with no `.blackwall/init-head` exits 2 (spec 0025 line 139); `cas harness verify --receipt /dev/null` exits 1. Goes red if any code changes. |
| REQ-0042-08 | `cas --version` and `cas-ebpfd --version` both match `^[a-z-]+ [0-9]+\.[0-9]+\.[0-9]+ \((unknown|[0-9a-f]{7,}(-dirty)?|v[^ ]+)\)$`. Goes red today for `cas-ebpfd` (F2). |
| REQ-0042-10 | `grep -rnE 'opentelemetry\|otlp\|prometheus' crates/*/src` prints nothing. Goes red when an exporter crate is added without a spec. |

```bash
./target/debug/cas --version | grep -E '^cas [0-9]+\.[0-9]+\.[0-9]+ \('
bash -c './target/debug/cas nonsense; test $? -eq 2'
cargo nextest run -p cas-cli conventions -j 2
```

## Notes

- Spec 0025 chose exit 2 for a not-ready step before this spec existed; D4 keeps it and gives it the general meaning, so no landed spec changes.
- The `cas demo` command prints no lease token (`main.rs` line 172); D3 keeps that, since a token is not a result.
- Clap's `env` attribute needs the `env` feature of `clap`; the workspace manifest enables `derive` only, and the Plan adds `env`. Spec 0004 has no `clap` row to amend.
- Out of scope: the content of log records and any redaction rule. Spec 0013 D5 covers the one secret the substrate handles.
