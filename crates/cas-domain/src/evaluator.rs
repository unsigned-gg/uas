//! Paths that a wave Builder may not change, and paths a Critic must not load (spec 0023 D4
//! and D6, spec 0025 D17).
//!
//! The lists are data. The matcher reads path components only. It does no I/O.
//!
//! Pattern forms:
//! - `P/**` matches a path under the directory `P`, with `P` as its first whole components.
//! - `**/X` matches `X` at the root and under any directory, by its last whole components.
//! - A bare path matches only at the root.

use std::ffi::OsStr;
use std::path::{Component, Path};

/// Evaluator paths: the verify command and the gates read them (spec 0023 D4 and D6).
pub const EVALUATOR_PATHS: &[&str] = &[
    "Makefile",
    ".github/workflows/**",
    "blackwall.toml",
    "deny.toml",
    "rust-toolchain.toml",
    "_typos.toml",
    ".config/nextest.toml",
    "xtask/**",
    "crates/cas-ebpf/ebpf/**",
    "**/Cargo.toml",
    "**/build.rs",
    "**/.cargo/config",
    "**/.cargo/config.toml",
    "**/clippy.toml",
    "**/.clippy.toml",
    "**/rustfmt.toml",
    "**/.rustfmt.toml",
    "typos.toml",
];

/// Critic inputs: the rule files and the MCP config that `claude -p` loads (spec 0025 D17).
pub const CRITIC_INPUT_PATHS: &[&str] = &[
    ".claude/**",
    ".mcp.json",
    "**/CLAUDE.md",
    "**/CLAUDE.local.md",
    "**/AGENTS.md",
];

/// True when a changed path is an evaluator path (spec 0023 D4 and D6).
pub fn is_evaluator_path(path: &Path) -> bool {
    matches_any(EVALUATOR_PATHS, path)
}

/// True when a changed path is a critic input (spec 0025 D17).
pub fn is_critic_input_path(path: &Path) -> bool {
    matches_any(CRITIC_INPUT_PATHS, path)
}

fn matches_any(patterns: &[&str], path: &Path) -> bool {
    let parts: Vec<&OsStr> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s),
            _ => None,
        })
        .collect();
    patterns.iter().any(|p| matches(p, &parts))
}

fn matches(pattern: &str, parts: &[&OsStr]) -> bool {
    if let Some(tail) = pattern.strip_prefix("**/") {
        let want: Vec<&str> = tail.split('/').collect();
        return parts.len() >= want.len() && same(&parts[parts.len() - want.len()..], &want);
    }
    if let Some(head) = pattern.strip_suffix("/**") {
        let want: Vec<&str> = head.split('/').collect();
        return parts.len() > want.len() && same(&parts[..want.len()], &want);
    }
    let want: Vec<&str> = pattern.split('/').collect();
    same(parts, &want)
}

fn same(parts: &[&OsStr], want: &[&str]) -> bool {
    parts.len() == want.len() && parts.iter().zip(want).all(|(a, b)| *a == OsStr::new(b))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{is_critic_input_path, is_evaluator_path};

    fn ev(p: &str) -> bool {
        is_evaluator_path(Path::new(p))
    }

    fn cr(p: &str) -> bool {
        is_critic_input_path(Path::new(p))
    }

    fn check(f: fn(&str) -> bool, yes: &[&str], no: &[&str]) {
        for p in yes {
            assert!(f(p), "expected a match: {p}");
        }
        for p in no {
            assert!(!f(p), "expected no match: {p}");
        }
    }

    #[test]
    fn evaluator_path_makefile() {
        check(ev, &["Makefile"], &["build.mk", "crates/x/Makefile"]);
    }

    #[test]
    fn evaluator_path_github_workflows() {
        check(
            ev,
            &[".github/workflows/ci.yml", ".github/workflows/c\u{e9}.yml"],
            &[".github/workflows", ".github/workflowsx/a.yml"],
        );
    }

    #[test]
    fn evaluator_path_blackwall_toml() {
        check(ev, &["blackwall.toml"], &["crates/x/blackwall.toml"]);
    }

    #[test]
    fn evaluator_path_deny_toml() {
        check(ev, &["deny.toml"], &["crates/x/deny.toml"]);
    }

    #[test]
    fn evaluator_path_rust_toolchain_toml() {
        check(
            ev,
            &["rust-toolchain.toml"],
            &["crates/x/rust-toolchain.toml"],
        );
    }

    #[test]
    fn evaluator_path_typos_underscore_toml() {
        check(ev, &["_typos.toml"], &["crates/x/_typos.toml"]);
    }

    #[test]
    fn evaluator_path_nextest_config() {
        check(ev, &[".config/nextest.toml"], &[".config/other.toml"]);
    }

    #[test]
    fn evaluator_path_xtask_dir() {
        check(ev, &["xtask/src/main.rs"], &["xtaskx/a.rs"]);
    }

    #[test]
    fn evaluator_path_ebpf_object_dir() {
        check(
            ev,
            &[
                "crates/cas-ebpf/ebpf/cas-ebpf.o",
                "crates/cas-ebpf/ebpf/cas-ebpf.o.sha256",
                "crates/cas-ebpf/ebpf/src/main.rs",
            ],
            &["crates/cas-ebpf/src/lib.rs", "crates/cas-ebpf/ebpfx/a"],
        );
    }

    #[test]
    fn evaluator_path_cargo_toml() {
        check(
            ev,
            &["Cargo.toml", "crates/x/Cargo.toml"],
            &["crates/x/notCargo.toml"],
        );
    }

    #[test]
    fn evaluator_path_build_rs() {
        check(
            ev,
            &["build.rs", "crates/cas-ebpf/build.rs"],
            &["crates/x/mybuild.rs"],
        );
    }

    #[test]
    fn evaluator_path_cargo_config() {
        check(
            ev,
            &[".cargo/config", "crates/x/.cargo/config"],
            &["crates/x/.cargo/configx"],
        );
    }

    #[test]
    fn evaluator_path_cargo_config_toml() {
        check(
            ev,
            &[".cargo/config.toml", "crates/x/.cargo/config.toml"],
            &["crates/x/.cargo/config.toml.bak"],
        );
    }

    #[test]
    fn evaluator_path_clippy_toml() {
        check(
            ev,
            &["clippy.toml", "crates/x/clippy.toml"],
            &["crates/x/aclippy.toml"],
        );
    }

    #[test]
    fn evaluator_path_dot_clippy_toml() {
        check(
            ev,
            &[".clippy.toml", "crates/x/.clippy.toml"],
            &["crates/x/.clippy.toml.bak"],
        );
    }

    #[test]
    fn evaluator_path_rustfmt_toml() {
        check(
            ev,
            &["rustfmt.toml", "crates/x/rustfmt.toml"],
            &["crates/x/arustfmt.toml"],
        );
    }

    #[test]
    fn evaluator_path_dot_rustfmt_toml() {
        check(
            ev,
            &[".rustfmt.toml", "crates/x/.rustfmt.toml"],
            &["crates/x/.rustfmt.toml.old"],
        );
    }

    #[test]
    fn evaluator_path_typos_toml_root_only() {
        check(ev, &["typos.toml"], &["crates/x/typos.toml"]);
    }

    #[test]
    fn evaluator_path_critic_input_claude_dir() {
        check(
            cr,
            &[".claude/agents/x.md", ".claude/settings.json"],
            &["docs/.claude/x.md"],
        );
    }

    #[test]
    fn evaluator_path_critic_input_mcp_json_root_only() {
        check(cr, &[".mcp.json"], &["crates/x/.mcp.json"]);
    }

    #[test]
    fn evaluator_path_critic_input_claude_md() {
        check(
            cr,
            &["CLAUDE.md", "crates/x/CLAUDE.md"],
            &["docs/CLAUDE.md.bak"],
        );
    }

    #[test]
    fn evaluator_path_critic_input_claude_local_md() {
        check(
            cr,
            &["CLAUDE.local.md", "crates/x/CLAUDE.local.md"],
            &["CLAUDE.local.md.bak"],
        );
    }

    #[test]
    fn evaluator_path_critic_input_agents_md() {
        check(cr, &["AGENTS.md", "docs/AGENTS.md"], &["AGENTS.md.bak"]);
    }

    #[test]
    fn evaluator_path_critic_input_is_not_evaluator_by_default() {
        // The two sets are separate: a rule file is a critic input, not an evaluator path.
        assert!(!ev("AGENTS.md"));
        assert!(!cr("Makefile"));
    }
}
