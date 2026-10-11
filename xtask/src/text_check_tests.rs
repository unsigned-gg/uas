// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Unit tests for `text_check.rs`, split out to keep that file small (plan 0053 P4).

use super::*;

const REL: &str = "## [0.1.0] - 2026-01-01\n### Added\n- first\n";

fn changelog(unreleased_body: &str, released: &str) -> String {
    format!("# Changelog\n\n## [Unreleased]\n{unreleased_body}\n{released}")
}

fn base() -> String {
    changelog("### Added\n- old\n", REL)
}

fn with(files: &[&str], head: String) -> Input {
    Input {
        files: files.iter().map(ToString::to_string).collect(),
        base_text: base(),
        head_text: head,
        body: Some(String::new()),
        ..Input::default()
    }
}

fn kinds(i: &Input) -> Vec<&'static str> {
    evaluate(i).findings.iter().map(|f| f.kind).collect()
}

const CODE: &str = "crates/uas-cli/src/main.rs";

#[test]
fn text_check_code_without_a_line_is_nolog() {
    let i = with(&[CODE], base());
    assert_eq!(kinds(&i), ["nolog"]);
}

#[test]
fn text_check_code_with_a_new_line_passes() {
    let i = with(&[CODE], changelog("### Added\n- old\n- new\n", REL));
    assert!(kinds(&i).is_empty());
}

#[test]
fn text_check_markdown_only_needs_no_line() {
    let i = with(
        &[
            "README.md",
            "docs/a.rs",
            "specs/x.md",
            "engineering/p.md",
            "crates/a/tests/t.rs",
            "tests/t.rs",
        ],
        base(),
    );
    assert!(kinds(&i).is_empty());
}

#[test]
fn text_check_agent_files_are_never_exempt() {
    assert!(!exempt(".claude/agents/uas-planner.md"));
    assert!(!exempt(".agents/x.md"));
    let i = with(&[".claude/agents/uas-planner.md"], base());
    assert_eq!(kinds(&i), ["nolog"]);
}

#[test]
fn text_check_exempt_list() {
    for p in [
        "docs/a",
        "specs/a",
        "engineering/a",
        "tests/a",
        "crates/x/tests/a",
        "NOTES.md",
    ] {
        assert!(exempt(p), "{p}");
    }
    for p in [
        CODE,
        "Cargo.toml",
        "Cargo.lock",
        "CHANGELOG.md",
        "docs_extra/a.rs",
        "contests/a.rs",
    ] {
        assert!(!exempt(p), "{p}");
    }
}

#[test]
fn text_check_dependabot_head_ref_is_exempt() {
    let mut i = with(&["Cargo.lock"], base());
    assert_eq!(kinds(&i), ["nolog"]);
    i.head_ref = Some("dependabot/cargo/x".into());
    assert!(kinds(&i).is_empty());
    i.head_ref = Some("feature/dependabot/x".into());
    assert_eq!(kinds(&i), ["nolog"]);
}

#[test]
fn text_check_edited_released_line_fails_and_names_it() {
    let head = changelog(
        "### Added\n- old\n- new\n",
        &REL.replace("- first", "- EDITED"),
    );
    let i = with(&[CODE], head);
    let out = evaluate(&i);
    assert_eq!(kinds(&i), ["released"]);
    assert!(
        out.findings[0].detail.contains("- first"),
        "{:?}",
        out.findings[0]
    );
}

#[test]
fn text_check_released_tail_cases() {
    assert!(released_tail_ok(&base(), &base()).is_ok());
    let grown = changelog(
        "### Added\n- a\n",
        &format!("## [0.2.0] - 2026-02-01\n- x\n\n{REL}"),
    );
    assert!(released_tail_ok(&base(), &grown).is_ok());
    assert!(released_tail_ok(&base(), "# Changelog\n").is_err());
    assert!(released_tail_ok("# no released section\n", "anything\n").is_ok());
}

fn rolled() -> String {
    changelog(
        "",
        &format!("## [9.9.9] - 2026-10-11\n### Added\n- old\n\n{REL}"),
    )
}

#[test]
fn text_check_release_roll_needs_no_line_but_runs_the_suffix_test() {
    let files = [
        "CHANGELOG.md",
        "Cargo.lock",
        "Cargo.toml",
        "crates/a/Cargo.toml",
    ];
    assert!(kinds(&with(&files, rolled())).is_empty());
    let edited = rolled().replace("- first", "- EDITED");
    assert_eq!(kinds(&with(&files, edited)), ["released"]);
}

#[test]
fn text_check_roll_shape_is_restricted() {
    let base = base();
    assert!(roll_shape(&["Cargo.toml".into()], &base, &rolled()));
    assert!(
        !roll_shape(&["Cargo.toml".into(), CODE.into()], &base, &rolled()),
        "code file"
    );
    assert!(
        !roll_shape(&["Cargo.toml".into()], &base, &base),
        "no version heading"
    );
    assert!(
        !roll_shape(&["Cargo.toml".into()], &base, &changelog("- bullet\n", REL)),
        "bullet left"
    );
    assert!(
        !roll_shape(&["Cargo.toml".into()], &base, &changelog("", &base)),
        "heading already in base"
    );
}

#[test]
fn text_check_roll_with_code_still_needs_a_line() {
    let files = ["CHANGELOG.md", "Cargo.toml", CODE];
    assert_eq!(kinds(&with(&files, rolled())), ["nolog"]);
}

#[test]
fn text_check_manifest_only_pr_is_not_exempt() {
    let i = with(&["Cargo.toml", "Cargo.lock"], base());
    assert_eq!(kinds(&i), ["nolog"]);
}

#[test]
fn text_check_conflict_markers_fail() {
    for m in ["<<<<<<< HEAD", "=======", ">>>>>>> x", "||||||| base"] {
        let head = changelog(&format!("### Added\n- old\n- new\n{m}\n"), REL);
        assert_eq!(kinds(&with(&[CODE], head)), ["marker"], "{m}");
    }
    assert!(markers("a <<<<<<< not at line start\n").is_empty());
}

#[test]
fn text_check_ticket_ids_in_subjects() {
    assert!(ticket_in_subject("feat(uas): CER-123 add x"));
    assert!(ticket_in_subject("fix: VES-1 y"));
    assert!(ticket_in_subject("fix: y (BRAND-42)"));
    assert!(!ticket_in_subject("feat(uas): add x"));
    assert!(!ticket_in_subject("docs(spec): SHA-256 note"));
    assert!(!ticket_in_subject("fix: METHOD-1 and CER-x and CER-"));
    assert!(!ticket_in_subject("Merge branch 'build/CER-1'"));
    assert!(!ticket_in_subject("Revert \"feat: CER-1 x\""));
}

#[test]
fn text_check_ticket_finding_prints_the_sha() {
    let mut i = with(&["README.md"], base());
    i.commits = vec![
        ("abc123".into(), "feat(uas): CER-123 add x".into()),
        ("def456".into(), "feat: ok".into()),
    ];
    let out = evaluate(&i);
    assert_eq!(kinds(&i), ["ticket"]);
    assert!(out.findings[0].detail.starts_with("abc123"));
}

#[test]
fn text_check_heading_order() {
    assert!(heading_order(&["### Added", "### Changed", "### Fixed"]).is_ok());
    assert!(heading_order(&["### Added", "### Changed", "### Added"]).is_err());
    assert!(heading_order(&["### Fixed", "### Deprecated"]).is_err());
    assert!(heading_order(&["### Added", "### Deprecated", "### Fixed"]).is_ok());
    assert!(heading_order(&["### Misc"]).is_err());
}

#[test]
fn text_check_duplicate_headings_fail_only_when_the_base_was_valid() {
    let dup = changelog("### Added\n- old\n- new\n### Changed\n### Added\n", REL);
    assert_eq!(kinds(&with(&[CODE], dup.clone())), ["headings"]);
    let mut i = with(&[CODE], dup);
    i.base_text = changelog("### Added\n- old\n### Changed\n### Added\n", REL);
    assert!(kinds(&i).is_empty(), "the base already had the duplicate");
}

#[test]
fn text_check_part_of_line() {
    assert!(is_part_of("Part of CER-9"));
    assert!(is_part_of("Part of spec 0053"));
    assert!(is_part_of("Part of CER-9\r"));
    for bad in [
        "Part of nothing",
        "Part of CER-",
        "Part of spec 53",
        "Part of CER-9 x",
        "part of CER-9",
    ] {
        assert!(!is_part_of(bad), "{bad}");
    }
}

#[test]
fn text_check_body_warnings_are_advisory() {
    let empty = body_warnings("nothing\n");
    assert_eq!(empty.len(), 6);
    let template = "## Problem\n## What this does\n## Verified\n## Gated steps after merge\n## Rollback\n\nPart of CER-9\n";
    assert!(body_warnings(template).is_empty());
    let swapped = "## Verified\n## Problem\n## What this does\n## Gated steps after merge\n## Rollback\nPart of CER-9\n";
    assert_eq!(body_warnings(swapped).len(), 1);
    let mut i = with(&["README.md"], base());
    i.body = Some("nothing".into());
    let out = evaluate(&i);
    assert!(out.findings.is_empty(), "body checks never fail (D9)");
    assert_eq!(out.warnings.len(), 6);
}

#[test]
fn text_check_missing_body_file_warns_without_a_body() {
    let mut i = with(&["README.md"], base());
    i.body = None;
    assert_eq!(evaluate(&i).warnings, ["no body"]);
}

#[test]
fn text_check_args() {
    let a = |v: &[&str]| parse_args(&v.iter().map(ToString::to_string).collect::<Vec<_>>());
    assert!(a(&[]).is_err());
    assert!(a(&["--base"]).is_err());
    assert!(a(&["--base", "x", "--bogus", "y"]).is_err());
    let ok = a(&["--base", "b", "--head-ref", "r", "--body-file", "f"]).unwrap();
    assert_eq!((ok.base.as_str(), ok.head.as_str()), ("b", "HEAD"));
}

fn tmp(name: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("xtask-text-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn sh(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        st.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&st.stderr)
    );
}

#[test]
fn text_check_git_failure_is_an_error_not_a_pass() {
    let dir = tmp("norepo");
    let err = gather(&dir, "origin/development", "HEAD").unwrap_err();
    assert!(err.contains("git merge-base"), "{err}");
    let repo = tmp("unknownref");
    sh(&repo, &["init", "-q"]);
    std::fs::write(repo.join("CHANGELOG.md"), base()).unwrap();
    sh(&repo, &["add", "."]);
    sh(&repo, &["commit", "-q", "-m", "init"]);
    assert!(gather(&repo, "no-such-ref", "HEAD")
        .unwrap_err()
        .contains("no-such-ref"));
    assert!(gather(&repo, "HEAD", "HEAD").is_ok());
}

#[test]
fn text_check_missing_changelog_is_an_error() {
    let repo = tmp("nolog");
    sh(&repo, &["init", "-q"]);
    std::fs::write(repo.join("a.txt"), "x").unwrap();
    sh(&repo, &["add", "."]);
    sh(&repo, &["commit", "-q", "-m", "init"]);
    let err = gather(&repo, "HEAD", "HEAD").unwrap_err();
    assert!(err.contains("CHANGELOG.md missing"), "{err}");
}
