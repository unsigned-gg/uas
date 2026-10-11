// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `cargo xtask text-check`: the CHANGELOG line, the commit subject and the PR body (spec 0053 D5).
//!
//! `cargo xtask text-check --base <ref> [--head <ref>=HEAD] [--body-file <path>] [--head-ref <name>] [--repo <dir>]`
//!
//! `--repo` aims every `git` call at another repository (the integration tests use a temp one);
//! it defaults to this repository.
//!
//! Everything is diffed from the merge base of `--base` and `--head` (D4). Uses `std` and the
//! `git` binary only: no `gh`, no network. The PR body comes only from `--body-file` (REQ-14).
//!
//! Exit 0 pass, 1 the PR is wrong, 2 the check could not run (usage, a failing `git` call, a
//! missing CHANGELOG.md). Findings print as `text-check: FAIL <kind>: <detail>`, body notes as
//! `text-check: warn: <detail>` and never fail (D9).

use std::path::Path;
use std::process::{Command, ExitCode};

/// Linear team keys (spec 0053 F12). A new team is one edit here.
const TICKET_KEYS: &[&str] = &[
    "CER", "OPS", "EST", "SEC", "ONB", "TOD", "RD", "BIZ", "BRAND", "VES",
];
/// Keep a Changelog 1.1.0 headings in the order D3 fixes.
const HEADINGS: &[&str] = &[
    "Added",
    "Changed",
    "Deprecated",
    "Removed",
    "Fixed",
    "Security",
];
/// The five PR template sections of spec 0053 F9, in order.
const BODY_SECTIONS: &[&str] = &[
    "Problem",
    "What this does",
    "Verified",
    "Gated steps after merge",
    "Rollback",
];
const USAGE: &str = "usage: cargo xtask text-check --base <ref> [--head <ref>] [--body-file <path>] [--head-ref <name>] [--repo <dir>]";

/// One failure: `kind` is `nolog`, `released`, `marker`, `ticket` or `headings`.
#[derive(Debug, PartialEq, Eq)]
pub struct Finding {
    pub kind: &'static str,
    pub detail: String,
}

/// Everything the checks read, gathered once.
#[derive(Debug, Default)]
pub struct Input {
    pub files: Vec<String>,
    pub base_text: String,
    pub head_text: String,
    /// `(sha, subject)` of each non-merge commit in `merge-base..head`.
    pub commits: Vec<(String, String)>,
    pub head_ref: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub findings: Vec<Finding>,
    pub warnings: Vec<String>,
}

/// A path that never asks for a CHANGELOG line (D6 rules 1 and 2).
pub fn exempt(path: &str) -> bool {
    if path.starts_with(".claude/") || path.starts_with(".agents/") {
        return false;
    }
    ["docs/", "specs/", "engineering/"]
        .iter()
        .any(|p| path.starts_with(p))
        || path.starts_with("tests/")
        || path.contains("/tests/")
        || (path.ends_with(".md") && path != "CHANGELOG.md")
}

/// Whether the changed files ask for a CHANGELOG line (D6 rule 2, before the roll exemption).
pub fn needs_log(files: &[String], head_ref: Option<&str>) -> bool {
    if head_ref.is_some_and(|r| r.starts_with("dependabot/")) {
        return false;
    }
    files.iter().any(|f| f != "CHANGELOG.md" && !exempt(f))
}

/// The lines of the `## [Unreleased]` section.
fn unreleased(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("## [Unreleased]") {
            inside = true;
        } else if inside && line.starts_with("## [") {
            break;
        } else if inside {
            out.push(line);
        }
    }
    out
}

/// The lines from the first `## [x.y.z]` heading to the end.
fn released_tail(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| {
        l.strip_prefix("## [")
            .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
    });
    start.map_or_else(Vec::new, |s| lines[s..].to_vec())
}

/// A line of head's `[Unreleased]` that base's lacks (blank lines do not count).
pub fn added_unreleased(base: &str, head: &str) -> bool {
    let old = unreleased(base);
    unreleased(head)
        .iter()
        .any(|l| !l.trim().is_empty() && !old.contains(l))
}

/// D6 rule 3: only manifests and the CHANGELOG changed, head's `[Unreleased]` holds no bullet and
/// a version heading that base lacks sits directly under it (the shape `make release-notes` writes).
pub fn roll_shape(files: &[String], base: &str, head: &str) -> bool {
    let manifests_only = files.iter().all(|f| {
        f == "Cargo.lock" || f == "CHANGELOG.md" || f == "Cargo.toml" || f.ends_with("/Cargo.toml")
    });
    if !manifests_only || unreleased(head).iter().any(|l| l.starts_with("- ")) {
        return false;
    }
    let is_version = |l: &str| {
        l.strip_prefix("## [")
            .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
    };
    let heading = head
        .lines()
        .skip_while(|l| !l.starts_with("## [Unreleased]"))
        .skip(1)
        .find(|l| !l.trim().is_empty());
    heading.is_some_and(|h| is_version(h) && !base.lines().any(|l| l == h))
}

/// D4: every line of base from its first released heading must end head. Err names the first
/// base line that differs.
pub fn released_tail_ok(base: &str, head: &str) -> Result<(), String> {
    let old = released_tail(base);
    let new: Vec<&str> = head.lines().collect();
    let aligned = new.len().checked_sub(old.len()).map(|s| &new[s..]);
    let at = aligned.map_or(0, |a| {
        old.iter()
            .zip(a)
            .position(|(o, n)| o != n)
            .unwrap_or(old.len())
    });
    if at >= old.len() && aligned.is_some() {
        return Ok(());
    }
    Err(format!(
        "released section edited; first differing base line: {:?}",
        old.get(at).copied().unwrap_or_default()
    ))
}

/// D8: line numbers (1-based) of conflict marker lines.
pub fn markers(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| {
            ["<<<<<<<", "=======", ">>>>>>>", "|||||||"]
                .iter()
                .any(|m| l.starts_with(m))
        })
        .map(|(i, _)| i + 1)
        .collect()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// D7: a subject carries a ticket ID. The forms git writes, `Merge ` and `Revert \"`, are skipped.
pub fn ticket_in_subject(subject: &str) -> bool {
    if subject.starts_with("Merge ") || subject.starts_with("Revert \"") {
        return false;
    }
    TICKET_KEYS.iter().any(|key| {
        subject.match_indices(key).any(|(i, _)| {
            let before = subject[..i].chars().next_back();
            let rest = subject[i + key.len()..].strip_prefix('-');
            let digits = rest.map_or(0, |r| r.chars().take_while(char::is_ascii_digit).count());
            let after = rest.and_then(|r| r[digits..].chars().next());
            !before.is_some_and(is_word) && digits > 0 && !after.is_some_and(is_word)
        })
    })
}

/// `Part of CER-<digits>` or `Part of spec NNNN`, the whole line.
fn is_part_of(line: &str) -> bool {
    let Some(rest) = line.trim_end().strip_prefix("Part of ") else {
        return false;
    };
    let all_digits = |s: &str, n: Option<usize>| {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && n.is_none_or(|n| s.len() == n)
    };
    rest.strip_prefix("CER-")
        .is_some_and(|d| all_digits(d, None))
        || rest
            .strip_prefix("spec ")
            .is_some_and(|d| all_digits(d, Some(4)))
}

/// D9: notes for a body that lacks a template section or the `Part of` line.
pub fn body_warnings(body: &str) -> Vec<String> {
    let lines: Vec<&str> = body.lines().collect();
    let mut warnings = Vec::new();
    let mut last = 0;
    let mut disordered = false;
    for name in BODY_SECTIONS {
        let heading = format!("## {name}");
        match lines.iter().position(|l| l.starts_with(&heading)) {
            None => warnings.push(format!("PR body lacks the section `{heading}`")),
            Some(p) => {
                disordered |= p < last;
                last = last.max(p);
            }
        }
    }
    if disordered {
        warnings.push("PR body sections are out of template order".to_string());
    }
    if !lines.iter().any(|l| is_part_of(l)) {
        warnings.push("PR body lacks a line `Part of CER-N` or `Part of spec NNNN`".to_string());
    }
    warnings
}

/// D3: each heading at most once, in order. Err names the first offender.
pub fn heading_order(unreleased_lines: &[&str]) -> Result<(), String> {
    let mut last: Option<usize> = None;
    for name in unreleased_lines
        .iter()
        .filter_map(|l| l.strip_prefix("### "))
    {
        let name = name.trim();
        let Some(idx) = HEADINGS.iter().position(|h| *h == name) else {
            return Err(format!("`### {name}` is not a Keep a Changelog heading"));
        };
        if last.is_some_and(|l| idx <= l) {
            return Err(format!("`### {name}` repeats or is out of order"));
        }
        last = Some(idx);
    }
    Ok(())
}

/// Run every check over gathered input. Pure: no git, no files.
pub fn evaluate(input: &Input) -> Outcome {
    let mut out = Outcome::default();
    let mut fail = |kind, detail: String| out.findings.push(Finding { kind, detail });
    let (base, head) = (input.base_text.as_str(), input.head_text.as_str());

    if needs_log(&input.files, input.head_ref.as_deref())
        && !added_unreleased(base, head)
        && !roll_shape(&input.files, base, head)
    {
        fail(
            "nolog",
            "the PR changes code but adds no line under `## [Unreleased]` (spec 0053 D6)".into(),
        );
    }
    if let Err(why) = released_tail_ok(base, head) {
        fail("released", why);
    }
    for n in markers(head) {
        fail(
            "marker",
            format!("CHANGELOG.md line {n} is a conflict marker"),
        );
    }
    for (sha, subject) in input.commits.iter().filter(|(_, s)| ticket_in_subject(s)) {
        fail(
            "ticket",
            format!("{sha} carries a ticket ID in its subject: {subject}"),
        );
    }
    if heading_order(&unreleased(base)).is_ok() {
        if let Err(why) = heading_order(&unreleased(head)) {
            fail("headings", why);
        }
    }
    match &input.body {
        Some(body) => out.warnings.extend(body_warnings(body)),
        None => out.warnings.push("no body".to_string()),
    }
    out
}

/// `git -C root <args>`; Err names the command and its stderr.
fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    // The repo is named by `-C`; a GIT_DIR inherited from a hook must not redirect it.
    let out = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("could not run `git {}`: {e}", args.join(" ")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn changelog_at(root: &Path, rev: &str) -> Result<String, String> {
    git(root, &["show", &format!("{rev}:CHANGELOG.md")])
        .map_err(|e| format!("CHANGELOG.md missing or unreadable at {rev}: {e}"))
}

/// Read the diff, the two CHANGELOG texts and the commit subjects from git.
pub fn gather(root: &Path, base: &str, head: &str) -> Result<Input, String> {
    let mb = git(root, &["merge-base", base, head])?.trim().to_string();
    let files = git(root, &["diff", "--name-only", "--no-renames", &mb, head])?
        .lines()
        .map(str::to_string)
        .collect();
    let base_text = changelog_at(root, &mb)?;
    let head_text = changelog_at(root, head)?;
    let range = format!("{mb}..{head}");
    let commits = git(root, &["log", "--no-merges", "--format=%H%x09%s", &range])?
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(sha, s)| (sha.to_string(), s.to_string()))
        .collect();
    Ok(Input {
        files,
        base_text,
        head_text,
        commits,
        ..Input::default()
    })
}

struct Args {
    base: String,
    head: String,
    body_file: Option<String>,
    head_ref: Option<String>,
    repo: Option<String>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let (mut base, mut head, mut body_file, mut head_ref, mut repo) =
        (None, "HEAD".to_string(), None, None, None);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it.next().cloned().ok_or(format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--base" => base = Some(value),
            "--head" => head = value,
            "--body-file" => body_file = Some(value),
            "--head-ref" => head_ref = Some(value),
            "--repo" => repo = Some(value),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Args {
        base: base.ok_or("--base is required")?,
        head,
        body_file,
        head_ref,
        repo,
    })
}

pub fn run(root: &Path, args: &[String]) -> ExitCode {
    let usage_error = |why: String| {
        eprintln!("text-check: error: {why}\n{USAGE}");
        ExitCode::from(2)
    };
    let args = match parse_args(args) {
        Ok(a) => a,
        Err(why) => return usage_error(why),
    };
    let root = args.repo.as_deref().map_or(root, Path::new);
    let mut input = match gather(root, &args.base, &args.head) {
        Ok(i) => i,
        Err(why) => {
            eprintln!("text-check: error: {why}");
            return ExitCode::from(2);
        }
    };
    input.head_ref = args.head_ref;
    if let Some(path) = &args.body_file {
        match std::fs::read_to_string(path) {
            Ok(body) => input.body = Some(body),
            Err(e) => return usage_error(format!("cannot read --body-file {path}: {e}")),
        }
    }
    let outcome = evaluate(&input);
    for w in &outcome.warnings {
        println!("text-check: warn: {w}");
    }
    for f in &outcome.findings {
        println!("text-check: FAIL {}: {}", f.kind, f.detail);
    }
    if outcome.findings.is_empty() {
        println!("text-check: ok ({} warning(s))", outcome.warnings.len());
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
#[path = "text_check_tests.rs"]
mod tests;
