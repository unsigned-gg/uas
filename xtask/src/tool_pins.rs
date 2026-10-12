// SPDX-License-Identifier: Apache-2.0 OR MIT
//! CI tool table parser and Makefile checker (spec 0034 D1-D10, plan 0034 P3).

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRow {
    pub key: String,
    pub version: String,
    pub pin: String,
}

/// Parse the `### CI tools` table from `specs/0004-dependency-and-licence-policy.md`.
/// Keyed by the first backticked token of its first cell (decision A).
pub fn parse_tool_rows(spec: &str) -> BTreeMap<String, ToolRow> {
    let mut rows = BTreeMap::new();
    let mut in_table = false;
    for raw in spec.lines() {
        let line = raw.trim();
        if !line.starts_with('|') {
            if in_table && !line.is_empty() {
                in_table = false;
            }
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() >= 2 && cells[0] == "Tool" && cells[1] == "Version" {
            in_table = true;
            continue;
        }
        if in_table && cells.len() >= 2 {
            if let Some(key) = extract_backtick_token(cells[0]) {
                let version = if key == "rustc-stable" || key == "rustc-nightly" {
                    extract_backtick_token(cells[1]).unwrap_or_else(|| cells[1].to_owned())
                } else {
                    first_xyz(cells[1]).unwrap_or_else(|| cells[1].to_owned())
                };
                let pin = if cells.len() >= 5 {
                    cells[4].to_owned()
                } else {
                    String::new()
                };
                rows.insert(key.clone(), ToolRow { key, version, pin });
            }
        }
    }
    rows
}

fn extract_backtick_token(cell: &str) -> Option<String> {
    let start = cell.find('`')? + 1;
    let end = cell[start..].find('`')? + start;
    Some(cell[start..end].to_owned())
}

fn first_xyz(text: &str) -> Option<String> {
    for part in text.split(|c: char| !c.is_ascii_digit() && c != '.') {
        if is_valid_xyz(part) {
            return Some(part.to_owned());
        }
    }
    None
}

pub fn is_valid_xyz(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

const BUILTINS: &[&str] = &[
    "build", "b", "check", "c", "clippy", "doc", "d", "fmt", "run", "r", "test", "t", "update",
    "metadata", "tree", "xtask", "bench", "clean", "fetch", "vendor", "package", "publish",
    "install",
];

/// Check `Makefile` against `rows` (decision D).
/// Returns `(checked_count, misses)`.
pub fn check_makefile(makefile: &str, rows: &BTreeMap<String, ToolRow>) -> (usize, Vec<String>) {
    let mut checked = 0;
    let mut misses = Vec::new();

    for (idx, raw_line) in makefile.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Skip help text and comment patterns
        if line.contains("## ") || (line.starts_with('@') && line.contains("printf")) {
            continue;
        }

        let words: Vec<&str> = line.split_whitespace().collect();
        for (i, &word) in words.iter().enumerate() {
            let cmd = word.trim_matches(|c: char| {
                matches!(c, ';' | '&' | '|' | ')' | '(' | '`' | '"' | '\'' | '$')
            });
            let bare_cmd = cmd.rsplit('/').next().unwrap_or(cmd);
            if let Some(sub) = bare_cmd.strip_prefix("cargo-") {
                if !BUILTINS.contains(&sub) {
                    checked += 1;
                    check_subcommand(line_no, bare_cmd, rows, &mut misses);
                }
            } else if bare_cmd == "cargo" {
                let mut next_idx = i + 1;
                while next_idx < words.len() {
                    let w = words[next_idx];
                    if w.starts_with('+') {
                        next_idx += 1;
                        continue;
                    }
                    break;
                }
                if next_idx < words.len() {
                    let raw_sub = word_sub(words[next_idx]);
                    if raw_sub == "install" {
                        checked += 1;
                        let has_pin = words[next_idx..].iter().any(|w| {
                            w.starts_with("--rev") || w.starts_with("--version") || w.contains('@')
                        });
                        if !has_pin {
                            misses.push(format!(
                                "pin-check: Makefile line {line_no}: cargo install needs --rev, --version or @version"
                            ));
                        }
                    } else if !BUILTINS.contains(&raw_sub)
                        && !raw_sub.starts_with('-')
                        && !raw_sub.is_empty()
                    {
                        checked += 1;
                        let key = format!("cargo-{raw_sub}");
                        check_subcommand(line_no, &key, rows, &mut misses);
                    }
                }
            }
        }
    }

    (checked, misses)
}

fn word_sub(w: &str) -> &str {
    w.trim_matches(|c: char| matches!(c, ';' | '&' | '|' | ')' | '(' | '`' | '"' | '\''))
}

fn check_subcommand(
    line_no: usize,
    key: &str,
    rows: &BTreeMap<String, ToolRow>,
    misses: &mut Vec<String>,
) {
    match rows.get(key) {
        Some(row) if is_valid_xyz(&row.version) => {}
        Some(_) => {
            misses.push(format!(
                "pin-check: Makefile line {line_no}: {key} has no valid X.Y.Z version in specs/0004 CI tools"
            ));
        }
        None => {
            misses.push(format!(
                "pin-check: Makefile line {line_no}: {key} has no row in specs/0004 CI tools"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gh_row_with_parens_and_two_token_version() {
        let spec = r#"
### CI tools

| Tool | Version | Licence | Use | Pin |
|---|---|---|---|---|
| `gh` (cli/cli) | 2.101.0, published 2026-09-15 | MIT | use | env GH_VERSION |
"#;
        let rows = parse_tool_rows(spec);
        assert_eq!(
            rows.get("gh"),
            Some(&ToolRow {
                key: "gh".to_string(),
                version: "2.101.0".to_string(),
                pin: "env GH_VERSION".to_string(),
            })
        );
    }

    #[test]
    fn parse_toolchain_rows() {
        let spec = r#"
### CI tools

| Tool | Version | Licence | Use | Pin |
|---|---|---|---|---|
| `rustc-stable` (rust-toolchain.toml) | `1.94.1` | MIT | use | 1.94.1 |
| `rustc-nightly` (rustup channel) | `nightly-2026-10-06` | MIT | use | sha |
"#;
        let rows = parse_tool_rows(spec);
        assert_eq!(rows["rustc-stable"].version, "1.94.1");
        assert_eq!(rows["rustc-nightly"].version, "nightly-2026-10-06");
    }
}
