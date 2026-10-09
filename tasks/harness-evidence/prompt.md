# Task: a harness dry run must never claim verified evidence ({{ticket}})

You work in the cas-substrate Rust workspace. Change only `crates/cas-cli/src/main.rs` and `CHANGELOG.md`.

## The defect

`cas harness run` takes `--evidence`, with clap `default_value = "verified"`. With `--dry-run` it executes nothing, but `harness_run` still builds an `Attestation` with that tier. A simulated activity therefore records `EvidenceTier::Verified`. The estate rule: `verified` means a mechanical receipt, for example a command that ran and exited 0. A dry run has no receipt.

## The change

1. Make `--evidence` optional: `evidence: Option<String>`, with no `default_value`. Update the help text.
2. In `harness_run`, take `evidence: Option<&str>` and resolve the tier:
   - With a dry run, `None` gives `EvidenceTier::Heuristic`. `Some("verified")` or `Some("reported")` returns an error that says a dry run executes nothing and cannot record that tier. `Some("heuristic")` or `Some("asserted")` is allowed.
   - With a real run, `None` gives `EvidenceTier::Verified`, because the command ran and exited 0. `Some(tier)` is honoured as today.
3. The dry-run receipt text stays `dry-run simulated: <task>`.
4. Do not change the real-run code path otherwise.

## Tests (write them first and watch them fail)

Add unit tests in the existing test module of `main.rs`, or a new `#[cfg(test)]` module:
- `dry_run_defaults_to_heuristic`: `harness_run(.., evidence: None, dry_run: true)`, and the attestation tier is `Heuristic`.
- `dry_run_rejects_verified`: `Some("verified")` with `dry_run: true` returns `Err`.
- `real_run_defaults_to_verified`: task `true`, `None`, `dry_run: false`, and the tier is `Verified`.

## Gates

- `cargo fmt --all`, then `make check JOBS=2` and `make hygiene JOBS=2` must pass. Use `-j 2` for any cargo call.
- The workspace forbids `unwrap` and `expect` outside tests (clippy).
- Add one line under `## [Unreleased]` in `CHANGELOG.md` (`### Fixed`).
