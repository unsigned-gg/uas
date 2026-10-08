# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- The estate Rust monorepo template: a Cargo workspace with a zero-I/O domain crate, a CLI crate and an `xtask`; a `Makefile` with the gate targets; LeanSpec specs with `make spec-check`; CI workflows; moon tasks; lefthook and gitleaks; `AGENTS.md` with four skills; Claude Code hooks; issue and PR templates; and `make new-repo` to start a repo from it.

### Fixed
- `make new-repo` runs `cargo fmt --all` after the rename. A name that sorts before `clap`, such as `cas`, left the CLI imports out of order and failed `make check`.
