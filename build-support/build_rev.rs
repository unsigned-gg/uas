// Build-time version stamp, shared by build scripts through `include!`.
// Captures `git describe` into `UAS_BUILD_REV` so `--version` can print the
// commit. The value is `unknown` when the build tree is not a git checkout.
//
// Freshness: `git describe --dirty` reads the index and the working tree, so
// the stamp must rerun when they change. Cargo cannot watch the whole working
// tree without a walk, so the script watches HEAD, the branch ref and the git
// index. The index mtime changes on `git add`, `git commit`, `git checkout` and
// on `git status` refreshes, which makes it the cheapest reliable signal.
// Limit: a working-tree edit without `git add` leaves the stamp stale until the
// next git command refreshes the index (for example `git status`).

use std::process::Command;

/// Run git with `args`. Return trimmed stdout, or None on any failure.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Emit the `cargo:` lines that stamp `UAS_BUILD_REV` and rebuild on commit and index moves.
pub fn emit_build_rev() {
    // Rebuild when the checked-out commit moves. A worktree keeps HEAD in its
    // own git dir and branch refs in the common dir, so watch both.
    if let Some(git_dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={git_dir}/HEAD");
        println!("cargo:rerun-if-changed={git_dir}/index");
        if let Ok(head) = std::fs::read_to_string(format!("{git_dir}/HEAD")) {
            if let Some(reference) = head.trim().strip_prefix("ref: ") {
                let common = git(&["rev-parse", "--git-common-dir"]).unwrap_or(git_dir);
                println!("cargo:rerun-if-changed={common}/{reference}");
            }
        }
    }

    let rev =
        git(&["describe", "--always", "--dirty", "--abbrev=7"]).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=UAS_BUILD_REV={rev}");
}
