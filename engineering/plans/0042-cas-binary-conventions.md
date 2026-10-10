# Build plan: 0042-cas-binary-conventions

Spec: `specs/0042-cas-binary-conventions.md` (status planned). Base: `origin/development` `7b984d0`.
Planner: `uas-planner`, 2026-10-10. Every path below was read in a worktree cut from that base.
Tiers: `verified` = I ran or read it at the path named; `reported` = the spec says it; `asserted` = my judgement.

## Findings the builders must know

1. **Spec 0004 has no row for `clap`, `tracing-subscriber` or `tracing-serde` [verified]:** `grep -n -i 'clap\|tracing' specs/0004-dependency-and-licence-policy.md` prints no pin row for any of them (only unrelated `feature` hits). The spec 0042 note is right about `clap`. It misses `UAS_LOG_FORMAT=json`: the `json` feature of `tracing-subscriber` 0.3.23 pulls `tracing-serde`, which `Cargo.lock` does not hold (`grep -c 'name = "tracing-serde"' Cargo.lock` prints 0). Rule 7 of the persona applies: P1 amends spec 0004 first. `tracing-serde` 0.2.0 is in the local registry [verified: `ls ~/.cargo/registry/src/*/`].
2. **What `pin-check` needs [verified: `xtask/src/pin_check.rs` lines 126-158, 160-188]:** it reads every table headed `| Crate | Version |`, and checks each pinned crate that a manifest declares directly against the lockfile. A crate with no row is skipped, so `make pin-check` passes today and stays green without a row. The rows are for rule 12 and for `cargo deny`, not to make `pin-check` pass. Locked versions to pin: `clap` 4.6.7, `tracing-subscriber` 0.3.23 [verified: `Cargo.lock` lines 499-500 and 3457-3458].
3. **`UAS_WAVE_OPEN_PRS` has no clap argument [verified]:** `grep -n 'open_prs\|open-prs' crates/uas-cli/src/*.rs` shows it only in `WaveEnv` (`wave_flow.rs` lines 383, 396, 584). The spec Plan says "the three wave arguments" and F6 says each variable is behind a flag. The real clap sites are `--repo` and `--state-dir` on `Step` and on `MergeGate` (`main.rs` lines 83-87 and 102-106): four arguments, two variables. REQ-0042-02 needs `UAS_WAVE_OPEN_PRS` declared on an argument, which means a new `--open-prs` flag. That is a new CLI surface a builder would have to design. P6 does the four existing arguments only. The flag is BLOCKED (see Blocked).
4. **The spec's red-first control for `uas-ebpfd` is green today unless `RUST_LOG` is set [verified by reading `uas-ebpfd.rs` lines 25-27]:** `EnvFilter::from_default_env()` with no `RUST_LOG` defaults to ERROR, and the daemon logs its `host probe` line at INFO. So `test ! -s out` passes on the old code. P5 must run the control with `RUST_LOG=info` in the environment to watch it go red, and add a second test with `RUST_LOG` unset that expects the `host probe` line on stderr (red today, green after the `info` default).
5. **REQ-0042-05 covers every binary, the Plan lists only `uas` [verified: spec lines 80 and 98].** P5 adds the same `UAS_LOG_FORMAT` switch to `uas-ebpfd`. No new crate carries it: two ten-line init blocks, because a shared logging crate would be a new dependency edge the spec does not ask for.
6. **Build-rev helper shape (my choice, asserted):** a plain source file `build-support/build_rev.rs` that each `build.rs` pulls in with `include!`. It adds no crate, no pin and no layering edge. `uas-ebpf` already has a `build.rs` (the BPF object hash check, `crates/uas-ebpf/build.rs`), so the helper must be a function that file calls, not a second build script. `publish = false` is set (`Cargo.toml` line 11), so a path outside the crate is safe.
7. **REQ-0042-10 needs no package [verified]:** `grep -rnE 'opentelemetry|otlp|prometheus' crates/*/src | wc -l` prints 0. The spec's control stays a row. Every wave's closing check runs it.

## Requirement coverage

| Requirement | Package |
|---|---|
| REQ-0042-01, 02 | P4 (feature), P6 (attributes); `UAS_WAVE_OPEN_PRS` part of 02 is BLOCKED (P8) |
| REQ-0042-03, 06 | P5 |
| REQ-0042-04, 05 | P5 (`uas-ebpfd`), P6 (`uas`) |
| REQ-0042-07 | P6 (conventions test pins the existing codes) |
| REQ-0042-08 | P3 (helper and `uas`), P5 (`uas-ebpfd`) |
| REQ-0042-09 | P7 |
| REQ-0042-10 | none; holds today (finding 7) |

Closed by a package: 9 of 10 fully, REQ-0042-02 partly (P8 waits on a ruling). The spec 0038 citation is P2.

## Waves

| Wave | Packages | Files overlap |
|---|---|---|
| 1 | P1, P3, P7 | none |
| 2 | P4, P2 | none (P4 needs P1) |
| 3 | P5, P6 | none (both need P3 and P4) |
| later | P8 | BLOCKED |

The integrator owns the single `CHANGELOG.md` line under `## [Unreleased]` for the feature PR. No builder edits it, to avoid a conflict across the wave.

## Packages

### P1: spec 0004 pin rows (wave 1)
- **Persona:** `uas-engineer` (spec amendment).
- **Files:** `specs/0004-dependency-and-licence-policy.md` (exists). Add rows: `clap` 4.6.7, `MIT OR Apache-2.0`, used by uas-cli, uas-ebpf, condition "`derive`, `env`"; `tracing-subscriber` 0.3.23, `MIT`, "`env-filter`, `json`"; `tracing-serde` 0.2.0, `MIT`, "transitive through the `json` feature; spec 0042 D2". Read each licence from `cargo metadata --locked` or the crate's `Cargo.toml` before writing it; do not copy the guesses above.
- **Done-when:** `make spec-check && make pin-check` exit 0.
- **Red-first:** set the `clap` row to 4.6.6, run `make pin-check`, watch it exit non-zero with `pin-check: clap pinned 4.6.6`, then restore 4.6.7.

### P2: spec 0038 cites spec 0042 (wave 2)
- **Persona:** `uas-engineer`.
- **Files:** `specs/0038-uas-operator-and-crds.md` (exists; it has no mention of 0042 today: `grep -n 0042` prints nothing). Add `0042-cas-binary-conventions` to `depends_on`, and one decision or requirement line that `uas-operator` follows spec 0042 for flags (D1), logging (D2, D3) and exit codes (D4). Do not change spec 0038's telemetry exporter text (spec 0042 D6 says it names the exporter).
- **Done-when:** `make spec-check` exits 0.
- **Red-first:** write the new `depends_on` entry with a wrong stem (`0042-cas-binary-convention`), run `make spec-check`, watch it fail with "depends_on ... is not a spec" (`xtask/src/spec_check.rs` line 507), then fix it.

### P3: shared build-rev helper, `uas` adopts it (wave 1)
- **Persona:** `uas-builder`.
- **Files:** `build-support/build_rev.rs` (new; the `git` function and the `cargo:rerun-if-changed` plus `cargo:rustc-env=UAS_BUILD_REV` logic moved out of `crates/uas-cli/build.rs`, as one `fn emit_build_rev()`); `crates/uas-cli/build.rs` (exists; shrinks to `include!("../../build-support/build_rev.rs"); fn main() { emit_build_rev(); }`).
- **Done-when:** `cargo build -p uas-cli --locked -j 2 && ./target/debug/uas --version | grep -E '^uas [0-9]+\.[0-9]+\.[0-9]+ \((unknown|[0-9a-f]{7,}(-dirty)?)\)$'` exits 0, and `make smoke JOBS=2` exits 0.
- **Red-first:** after the move, change the `rustc-env` name in the helper to `UAS_BUILD_REVX`, build, watch `env!("UAS_BUILD_REV")` fail to compile in `main.rs`, then restore.

### P4: workspace manifest features (wave 2, after P1 landed)
- **Persona:** `uas-builder`.
- **Files:** `Cargo.toml` (exists; line 33 `clap` gains `env`; line 37 `tracing-subscriber` gains `json`); `Cargo.lock` (exists; regenerated, gains `tracing-serde`).
- **Done-when:** `cargo build --workspace --locked -j 2` exits 0 after `cargo update -w`, and `make deny pin-check JOBS=2` exits 0 (no new licence or advisory from `tracing-serde`).
- **Red-first:** before the manifest edit, put `env = "UAS_SCRATCH"` on one scratch clap argument and watch `cargo check -p uas-cli` fail (no `env` method without the feature); discard the scratch edit, then add the features and watch the same line compile.

### P5: `uas-ebpfd` conventions (wave 3)
- **Persona:** `uas-ebpf-builder` (changes `crates/uas-ebpf`; the kernel object is untouched, so `make ebpf-build` is not needed and `ebpf/uas-ebpf.o.sha256` must stay as is).
- **Files:** `crates/uas-ebpf/src/bin/uas-ebpfd.rs` (exists; `#[command(... version = VERSION)]` with `VERSION` built as in `crates/uas-cli/src/main.rs` line 25; `.with_writer(std::io::stderr)`; default filter `info` via `EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))`; `UAS_LOG_FORMAT=json` selects `.json()`; read the variable once in `main`, since it is a logging switch and not a clap argument, and cite spec 0042 D2 in the doc comment); `crates/uas-ebpf/build.rs` (exists; add `include!("../../build-support/build_rev.rs")` and one `emit_build_rev();` call at the top of `main`, keep the SHA-256 check as is); `crates/uas-ebpf/tests/conventions.rs` (new, uses `env!("CARGO_BIN_EXE_uas-ebpfd")`).
- **Done-when:** `cargo nextest run -p uas-ebpf conventions -j 2` exits 0. Tests: `conventions_ebpfd_version_matches` (the REQ-0042-08 regex from the spec Test table); `conventions_ebpfd_stdout_is_empty` (spec rows 03 and 06 command: run with `--socket` under a temp dir, `RUST_LOG=info`, assert stdout empty whatever the exit code); `conventions_ebpfd_default_level_is_info` (no `RUST_LOG`, assert stderr contains `host probe`); `conventions_ebpfd_json_log` (`UAS_LOG_FORMAT=json`, assert the first stderr line parses as a JSON object).
- **Red-first:** run the new tests against the unmodified `uas-ebpfd.rs` (write tests first). Expect `version` red (no `--version`), `stdout_is_empty` red only with `RUST_LOG=info` (finding 4), `default_level` red, `json` red. Then implement.
- **Note:** `uas-ebpfd` exits 2 with a one-line reason on a host that cannot load the program (`uas-ebpfd.rs` lines 41-44), already matching D4. Tests must not assume a loadable host.

### P6: `uas` CLI conventions (wave 3)
- **Persona:** `uas-builder`.
- **Files:** `crates/uas-cli/src/main.rs` (exists): in `fn main` (line 1082) the default filter becomes `warn` when `RUST_LOG` is unset, `UAS_LOG_FORMAT=json` selects `.json()`, the writer stays stderr; in `WaveCommand::Step` and `WaveCommand::MergeGate` add `env = "UAS_WAVE_REPO"` to both `repo` arguments and `env = "UAS_WAVE_STATE_DIR"` to both `state_dir` arguments (lines 83, 86, 102, 105). Leave `WaveEnv::from_process` in `wave_flow.rs` unchanged: the precedence flag, env, default is the same either way. `crates/uas-cli/tests/conventions.rs` (new).
- **Done-when:** the spec's closing block: `./target/debug/uas --version | grep -E '^uas [0-9]+\.[0-9]+\.[0-9]+ \('`, `bash -c './target/debug/uas nonsense; test $? -eq 2'`, and `cargo nextest run -p uas-cli conventions -j 2`. Tests: `conventions_default_level_is_warn` (`uas demo --slot slot-a` with no `RUST_LOG`, stderr empty, stdout parses as JSON); `conventions_debug_goes_to_stderr` (F4 shape, `RUST_LOG=debug`, stdout still parses as JSON); `conventions_json_log` (`UAS_LOG_FORMAT=json` with `RUST_LOG=debug`, every stderr line parses as JSON); `conventions_exit_codes` (`uas nonsense` is 2; `uas harness verify --receipt /dev/null` is 1); `conventions_help_lists_wave_env` (`uas wave step --help` and `uas wave merge-gate --help` contain `UAS_WAVE_REPO` and `UAS_WAVE_STATE_DIR`). The `wave step` exit 2 case already exists as `wave_step_no_init_head_exits_2` (`tests/wave_flow.rs` line 683); do not duplicate it.
- **Red-first:** write the tests first and run them. `help_lists_wave_env` and `json_log` are red today. `default_level_is_warn` cannot go red on the level alone, because `uas demo` logs nothing below warn today (F4: one line only with `RUST_LOG=debug`). Say so in the verdict. To keep a real control, assert with `RUST_LOG=debug` that the `DEBUG` line from F4 appears on stderr and not on stdout; that part goes red if the writer or filter breaks. Do not add a log call to make the default test fail.

### P7: `make release-notes` refuses a dirty rev (wave 1)
- **Persona:** `uas-builder`.
- **Files:** `Makefile` (exists; target `release-notes`, lines 101-111): before the `sed` that edits `Cargo.toml`, add `rev=$$(git describe --always --dirty --abbrev=7); case "$$rev" in *-dirty) echo "release-notes: refusing a dirty tree ($$rev)" >&2; exit 1;; esac`. Exit 2: nothing was done because a precondition was wrong (D4), the same code as the usage error on line 102.
- **Done-when:** no spec Test row exists for REQ-0042-09 [verified: Test table lines 82-87]. The command is `asserted`: in a throwaway copy, `git worktree add /tmp/uas-rn-test HEAD`, modify a tracked file there, run `make release-notes VERSION=9.9.9` and expect exit 2 with the refusal text and an untouched `Cargo.toml`; on a clean copy it must pass the new check. Remove the throwaway worktree after.
- **Red-first:** run the same dirty-tree command before the Makefile edit and watch it rewrite `Cargo.toml` and exit 0 (do this only in the throwaway worktree, never in the shared checkout).
- **Open:** ask the spec to add a REQ-0042-09 Test row so this done-when stops being `asserted`.

### P8: `UAS_WAVE_OPEN_PRS` declared on a clap argument (BLOCKED)
- **Blocked on:** a ruling. REQ-0042-02 requires the variable on a clap argument, and none exists (finding 3). The proposed shape is `--open-prs` (bool flag, `env = "UAS_WAVE_OPEN_PRS"`) on `Step`, which the executor reads at `wave_flow.rs` line 584. A builder would have to choose the subcommands that carry it and whether `1` stays the only true value (clap booleans parse `true`). Spec 0025 D16 owns the semantics. Unblocks when the integrator rules on the flag, or amends REQ-0042-02 to exempt `UAS_WAVE_OPEN_PRS`.
- **Persona once ruled:** `uas-builder`, file `crates/uas-cli/src/main.rs` plus `crates/uas-cli/src/wave_flow.rs`, wave after P6 (same file).

## Dependency summary

P4 needs P1 (rule 7). P5 needs P3 (helper) and P4 (`json` feature). P6 needs P4 (`env` and `json` features). P2 needs nothing but follows the spec-only work. P8 needs a ruling and P6.
