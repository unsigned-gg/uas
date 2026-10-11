// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Repo automation. Run with `cargo xtask <command>`.
//!
//! Commands:
//! - `spec-check`: validate `specs/` (see `spec_check.rs`).
//! - `toolchain-check`: verify the active Rust matches `rust-toolchain.toml`.
//! - `pin-check`: verify the spec 0004 pin tables against the lockfiles (see `pin_check.rs`).
//! - `fetch-contracts`: fetch the pinned estate contracts into `.contracts/` (see `fetch_contracts.rs`).
//! - `advisory-check`: validate the `deny.toml` ignore entries of spec 0045 D2 (see `advisory_check.rs`).
//! - `text-check`: the CHANGELOG line, commit subjects and PR body of spec 0053 (see `text_check.rs`).

mod advisory_check;
mod fetch_contracts;
mod pin_check;
mod spec_check;
mod text_check;
mod toolchain;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The repo root: the parent of the `xtask` directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn main() -> ExitCode {
    let arg = std::env::args().nth(1);
    let root = repo_root();
    match arg.as_deref() {
        Some("spec-check") => spec_check::run(&root),
        Some("toolchain-check") => toolchain::run(&root),
        Some("pin-check") => pin_check::run(&root),
        Some("fetch-contracts") => {
            let rest: Vec<String> = std::env::args().skip(2).collect();
            fetch_contracts::run(&root, &rest)
        }
        Some("text-check") => text_check::run(&root, &std::env::args().skip(2).collect::<Vec<_>>()),
        Some("advisory-check") => {
            advisory_check::run(&root, &std::env::args().skip(2).collect::<Vec<_>>())
        }
        other => {
            if let Some(name) = other {
                eprintln!("xtask: unknown command {name:?}");
            }
            eprintln!("usage: cargo xtask <spec-check|toolchain-check|pin-check|text-check|advisory-check|fetch-contracts>");
            ExitCode::from(2)
        }
    }
}
