//! Repo automation. Run with `cargo xtask <command>`.
//!
//! Commands:
//! - `spec-check`: validate `specs/` (see `spec_check.rs`).
//! - `toolchain-check`: verify the active Rust matches `rust-toolchain.toml`.
//! - `pin-check`: verify the spec 0004 pin tables against the lockfiles (see `pin_check.rs`).

mod pin_check;
mod spec_check;
mod toolchain;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The repo root: the first directory, from `start` upward, whose `Cargo.toml` has a
/// `[workspace]` table and that holds an `xtask/` directory. The eBPF crate has its own
/// `[workspace]` and no `xtask/`, so the walk passes it.
fn find_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| is_repo_root(dir))
        .map(Path::to_path_buf)
}

fn is_repo_root(dir: &Path) -> bool {
    let has_workspace = std::fs::read_to_string(dir.join("Cargo.toml"))
        .is_ok_and(|text| text.lines().any(|l| l.trim() == "[workspace]"));
    has_workspace && dir.join("xtask").is_dir()
}

fn main() -> ExitCode {
    let arg = std::env::args().nth(1);
    let root = std::env::current_dir()
        .ok()
        .and_then(|cwd| find_root(&cwd))
        .unwrap_or_else(|| {
            eprintln!("xtask: run from inside the repo");
            std::process::exit(2)
        });
    match arg.as_deref() {
        Some("spec-check") => spec_check::run(&root),
        Some("toolchain-check") => toolchain::run(&root),
        Some("pin-check") => pin_check::run(&root),
        other => {
            if let Some(name) = other {
                eprintln!("xtask: unknown command {name:?}");
            }
            eprintln!("usage: cargo xtask <spec-check|toolchain-check|pin-check>");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn find_root_skips_the_nested_workspace() {
        let dir = std::env::temp_dir().join(format!("xtask-root-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let nested = dir.join("crates/x/ebpf");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(dir.join("xtask")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"xtask\"]\n",
        )
        .unwrap();
        fs::write(
            nested.join("Cargo.toml"),
            "[workspace]\n\n[package]\nname = \"e\"\n",
        )
        .unwrap();
        assert_eq!(find_root(&nested), Some(dir.clone()));
        assert_eq!(find_root(&dir), Some(dir.clone()));
        // Control: a directory outside any repo finds nothing.
        let outside = std::env::temp_dir().join(format!("xtask-none-{}", std::process::id()));
        fs::create_dir_all(&outside).unwrap();
        assert_eq!(find_root(&outside), None);
        fs::remove_dir_all(&dir).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }
}
