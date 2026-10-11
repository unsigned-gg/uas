// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Integration tests for `cargo xtask text-check` (spec 0053, plan 0053 P5).
//!
//! Each case builds a throwaway git repository, seeds it with this repository's real
//! `CHANGELOG.md` released tail, `release-plz.toml` and PR template, commits a base on `main`,
//! branches to `feature`, applies one mutation, and runs the built `xtask` binary at it with
//! `--repo`. Every failing case is paired with a passing control that differs by one change.
//! Git runs with an empty global config and no system config, so nothing is signed or hooked.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// The real repository root (the parent of `xtask/`).
fn real_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent")
        .to_path_buf()
}

/// The real CHANGELOG with `[Unreleased]` replaced by one known `### Added` bullet, so the
/// cases do not depend on what the live section holds. Everything from the first released
/// heading on is the real file.
fn seed_changelog() -> String {
    let real = fs::read_to_string(real_root().join("CHANGELOG.md")).expect("CHANGELOG.md");
    let start = real.find("## [Unreleased]").expect("[Unreleased] heading");
    let released = real
        .find("\n## [0.")
        .or_else(|| real.find("\n## [1."))
        .expect("a released heading")
        + 1;
    format!(
        "{}## [Unreleased]\n### Added\n- base entry\n\n{}",
        &real[..start],
        &real[released..]
    )
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn count(&self, needle: &str) -> usize {
        self.stdout.lines().filter(|l| l.contains(needle)).count()
    }
}

struct Fixture {
    top: PathBuf,
    repo: PathBuf,
}

impl Fixture {
    /// A repository with the base commit on `main`, checked out on `feature`.
    fn new(name: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let top = std::env::temp_dir().join(format!("xtask-it-{}-{n}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&top);
        let repo = top.join("repo");
        fs::create_dir_all(&repo).unwrap();
        fs::write(top.join("empty.gitconfig"), "").unwrap();
        let f = Self { top, repo };
        f.git(&["init", "-q", "-b", "main"]);
        f.write("CHANGELOG.md", &seed_changelog());
        let root = real_root();
        f.write("release-plz.toml", &read(&root.join("release-plz.toml")));
        f.write(
            ".github/pull_request_template.md",
            &read(&root.join(".github/pull_request_template.md")),
        );
        f.write("Cargo.toml", "[workspace]\nversion = \"0.1.0\"\n");
        f.write("Cargo.lock", "version = 3\n");
        f.write("README.md", "# fixture\n");
        f.write("crates/uas-cli/src/main.rs", "fn main() {}\n");
        f.write(".claude/agents/uas-planner.md", "# agent\n");
        f.commit("chore: base");
        f.git(&["checkout", "-q", "-b", "feature"]);
        f
    }

    /// `git -C repo args`, panicking with stderr on failure. The environment is cleared of every
    /// variable a hook could leave behind.
    fn git_cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new("git");
        for var in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_PREFIX"] {
            c.env_remove(var);
        }
        c.env("GIT_CONFIG_GLOBAL", self.top.join("empty.gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .arg("-C")
            .arg(&self.repo)
            .args(args);
        c
    }

    fn git(&self, args: &[&str]) -> String {
        let out = self.git_cmd(args).output().expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn write(&self, path: &str, text: &str) {
        let p = self.repo.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn read(&self, path: &str) -> String {
        read(&self.repo.join(path))
    }

    fn commit(&self, subject: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", subject]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Append a line to a tracked file.
    fn append(&self, path: &str, line: &str) {
        let mut text = self.read(path);
        text.push_str(line);
        text.push('\n');
        self.write(path, &text);
    }

    /// Add a bullet directly under the first `### Added` of `[Unreleased]`.
    fn add_bullet(&self) {
        let text =
            self.read("CHANGELOG.md")
                .replacen("### Added\n", "### Added\n- feature entry\n", 1);
        self.write("CHANGELOG.md", &text);
    }

    /// Apply the roll `make release-notes` makes: a version heading directly under `[Unreleased]`
    /// (the awk of the Makefile `release-notes` target) and the version bump.
    fn roll(&self, version: &str) {
        let text = self.read("CHANGELOG.md");
        let rolled = text.replacen(
            "## [Unreleased]\n",
            &format!("## [Unreleased]\n\n## [{version}] - 2026-10-12\n"),
            1,
        );
        self.write("CHANGELOG.md", &rolled);
        self.write(
            "Cargo.toml",
            &format!("[workspace]\nversion = \"{version}\"\n"),
        );
        self.write("Cargo.lock", &format!("version = 3\n# {version}\n"));
    }

    /// Edit one released line (a bullet of the first released section).
    fn edit_released(&self) {
        let text = self.read("CHANGELOG.md");
        let at = text.find("\n## [0.").expect("released heading");
        let (head, tail) = text.split_at(at);
        self.write(
            "CHANGELOG.md",
            &format!("{head}{}", tail.replacen("- ", "- EDITED ", 1)),
        );
    }

    fn check(&self, extra: &[&str]) -> Run {
        self.check_base("main", extra)
    }

    fn check_base(&self, base: &str, extra: &[&str]) -> Run {
        let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_CONFIG_GLOBAL", self.top.join("empty.gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["text-check", "--base", base, "--repo"])
            .arg(&self.repo)
            .args(extra)
            .output()
            .expect("xtask runs");
        Run {
            code: out.status.code().expect("exit code"),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.top);
    }
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn code_change(f: &Fixture) {
    f.append("crates/uas-cli/src/main.rs", "// change");
}

// Spec Test row 1 and its control, row 2.
#[test]
fn code_without_a_changelog_line_fails() {
    let f = Fixture::new("row1");
    code_change(&f);
    f.commit("feat(uas): add x");
    let r = f.check(&[]);
    assert_eq!(r.code, 1, "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("FAIL nolog"), "{}", r.stdout);
}

#[test]
fn code_with_a_changelog_line_passes() {
    let f = Fixture::new("row2");
    code_change(&f);
    f.add_bullet();
    f.commit("feat(uas): add x");
    let r = f.check(&[]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
    assert!(!r.stdout.contains("FAIL"), "{}", r.stdout);
}

// Row 3: a docs-only change is exempt (control: row 1 fails on a code file).
#[test]
fn docs_only_change_is_exempt() {
    let f = Fixture::new("row3");
    f.append("README.md", "more");
    f.write("docs/guide.md", "# guide\n");
    f.write("specs/0099-x.md", "# x\n");
    f.commit("docs: add notes");
    let r = f.check(&[]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
}

// D6: a dependabot head ref is exempt; the same change without the ref fails.
#[test]
fn dependabot_head_ref_is_exempt_and_others_are_not() {
    let f = Fixture::new("dependabot");
    f.append("Cargo.lock", "# bump");
    f.commit("build(deps): bump x");
    let exempt = f.check(&["--head-ref", "dependabot/cargo/x-1.2.3"]);
    assert_eq!(exempt.code, 0, "{}{}", exempt.stdout, exempt.stderr);
    let control = f.check(&["--head-ref", "feature/bump"]);
    assert_eq!(control.code, 1, "{}{}", control.stdout, control.stderr);
    assert!(control.stdout.contains("FAIL nolog"));
}

// Row 4: a line added and a released line edited.
#[test]
fn edited_released_line_fails() {
    let f = Fixture::new("row4");
    code_change(&f);
    f.add_bullet();
    f.edit_released();
    f.commit("fix(uas): tidy");
    let r = f.check(&[]);
    assert_eq!(r.code, 1, "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("FAIL released"), "{}", r.stdout);
}

// Row 6: a conflict marker line in CHANGELOG.md.
#[test]
fn conflict_marker_fails() {
    let f = Fixture::new("row6");
    code_change(&f);
    f.add_bullet();
    f.append("CHANGELOG.md", "<<<<<<< HEAD");
    f.commit("fix(uas): merge");
    let r = f.check(&[]);
    assert_eq!(r.code, 1, "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("FAIL marker"), "{}", r.stdout);
}

// Rows 7 and 9: a ticket ID in a subject fails and names the SHA; a non-key hyphen does not.
#[test]
fn ticket_in_subject_fails_and_names_the_sha() {
    let f = Fixture::new("row7");
    code_change(&f);
    f.add_bullet();
    let sha = f.commit("feat(uas): CER-123 add x");
    let r = f.check(&[]);
    assert_eq!(r.code, 1, "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("FAIL ticket"), "{}", r.stdout);
    assert!(r.stdout.contains(&sha), "SHA missing: {}", r.stdout);
}

#[test]
fn subject_without_a_ticket_passes() {
    let f = Fixture::new("row9");
    code_change(&f);
    f.add_bullet();
    f.commit("docs(spec): SHA-256 note");
    let r = f.check(&[]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
    assert_eq!(r.count("FAIL"), 0);
}

// Rows 15 and 16: the body is advisory. Absent sections warn six times and exit 0.
#[test]
fn body_without_sections_warns_and_exits_zero() {
    let f = Fixture::new("row15");
    f.append("README.md", "x");
    f.commit("docs: x");
    f.write("../body.md", "nothing\n");
    let body = f.top.join("body.md");
    let r = f.check(&["--body-file", body.to_str().unwrap()]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
    assert_eq!(r.count("text-check: warn:"), 6, "{}", r.stdout);
}

#[test]
fn full_template_body_with_part_of_line_has_no_warning() {
    let f = Fixture::new("row16");
    f.append("README.md", "x");
    f.commit("docs: x");
    let body = f
        .read(".github/pull_request_template.md")
        .replace("Part of CER-N", "Part of CER-9");
    f.write("../body.md", &body);
    let r = f.check(&["--body-file", f.top.join("body.md").to_str().unwrap()]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
    assert_eq!(r.count("text-check: warn:"), 0, "{}", r.stdout);
}

// The body line absent (placeholder `Part of CER-N`) warns once; present (row 11) did not.
#[test]
fn body_with_template_placeholder_warns_once_about_the_part_of_line() {
    let f = Fixture::new("partof");
    f.append("README.md", "x");
    f.commit("docs: x");
    f.write("../body.md", &f.read(".github/pull_request_template.md"));
    let r = f.check(&["--body-file", f.top.join("body.md").to_str().unwrap()]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
    assert_eq!(r.count("text-check: warn:"), 1, "{}", r.stdout);
    assert!(r.stdout.contains("Part of"), "{}", r.stdout);
}

// Row 19: a release roll passes. Row 20: the same roll with a released line edited fails.
#[test]
fn release_roll_passes() {
    let f = Fixture::new("row19");
    f.roll("9.9.9");
    f.commit("chore(release): 9.9.9");
    let r = f.check(&[]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
}

#[test]
fn release_roll_with_an_edited_released_line_fails() {
    let f = Fixture::new("row20");
    f.roll("9.9.9");
    f.edit_released();
    f.commit("chore(release): 9.9.9");
    let r = f.check(&[]);
    assert_eq!(r.code, 1, "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("FAIL released"), "{}", r.stdout);
}

// Row 21: an agent file is not exempt even though it is Markdown.
#[test]
fn agent_file_change_needs_a_line() {
    let f = Fixture::new("row21");
    f.append(".claude/agents/uas-planner.md", "x");
    f.commit("docs(agents): tweak planner");
    let r = f.check(&[]);
    assert_eq!(r.code, 1, "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("FAIL nolog"), "{}", r.stdout);
}

// Row 22: a roll-shaped CHANGELOG plus a source change is not a roll.
#[test]
fn roll_plus_a_code_change_fails() {
    let f = Fixture::new("row22");
    f.roll("9.9.9");
    code_change(&f);
    f.commit("chore(release): 9.9.9");
    let r = f.check(&[]);
    assert_eq!(r.code, 1, "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("FAIL nolog"), "{}", r.stdout);
}

// Fail closed: a git failure and a missing CHANGELOG exit 2 with a named reason.
#[test]
fn unknown_base_ref_exits_two() {
    let f = Fixture::new("noref");
    code_change(&f);
    f.add_bullet();
    f.commit("feat(uas): add x");
    let r = f.check_base("no-such-ref", &[]);
    assert_eq!(r.code, 2, "{}{}", r.stdout, r.stderr);
    assert!(r.stderr.contains("text-check: error:"), "{}", r.stderr);
    assert!(!r.stdout.contains("text-check: ok"), "{}", r.stdout);
}

#[test]
fn missing_changelog_exits_two() {
    let f = Fixture::new("nolog-file");
    fs::remove_file(f.repo.join("CHANGELOG.md")).unwrap();
    f.commit("docs: drop the changelog");
    let r = f.check(&[]);
    assert_eq!(r.code, 2, "{}{}", r.stdout, r.stderr);
    assert!(r.stderr.contains("CHANGELOG.md"), "{}", r.stderr);
}

// Finding 5: the base moved on with a merged roll. Comparing with the merge base still passes.
#[test]
fn base_that_moved_on_with_a_roll_still_passes() {
    let f = Fixture::new("moved");
    code_change(&f);
    f.add_bullet();
    f.commit("feat(uas): add x");
    f.git(&["checkout", "-q", "main"]);
    f.roll("9.9.9");
    f.commit("chore(release): 9.9.9");
    f.git(&["checkout", "-q", "feature"]);
    let r = f.check(&[]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
    assert_eq!(r.count("FAIL"), 0, "{}", r.stdout);
}

// Finding 6: a heading violation already in the merge base is not a new failure; a new one is.
const BAD_UNRELEASED: &str = "### Added\n- one\n### Changed\n- two\n### Added\n- three\n";

#[test]
fn heading_violation_present_in_base_and_head_passes() {
    let f = Fixture::new("head-old");
    let bad = f
        .read("CHANGELOG.md")
        .replacen("### Added\n- base entry\n", BAD_UNRELEASED, 1);
    f.git(&["checkout", "-q", "main"]);
    f.write("CHANGELOG.md", &bad);
    f.commit("chore: invalid base");
    f.git(&["checkout", "-q", "-b", "feature2"]);
    code_change(&f);
    f.write(
        "CHANGELOG.md",
        &bad.replacen("- one\n", "- one\n- new\n", 1),
    );
    f.commit("feat(uas): add x");
    let r = f.check(&[]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
}

#[test]
fn new_heading_violation_fails() {
    let f = Fixture::new("head-new");
    code_change(&f);
    let text = f.read("CHANGELOG.md").replacen(
        "- base entry\n",
        "- base entry\n### Changed\n- two\n### Added\n- again\n",
        1,
    );
    f.write("CHANGELOG.md", &text);
    f.commit("feat(uas): add x");
    let r = f.check(&[]);
    assert_eq!(r.code, 1, "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("FAIL headings"), "{}", r.stdout);
}

// REQ-07: release-plz must not write the CHANGELOG. Read from the real file.
#[test]
fn release_plz_does_not_update_the_changelog() {
    let text = read(&real_root().join("release-plz.toml"));
    assert!(
        text.lines().any(|l| l == "changelog_update = false"),
        "release-plz.toml must set changelog_update = false"
    );
}
