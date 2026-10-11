// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `cargo xtask spec-check`: validate specification files under `specs/`.
//!
//! Spec 0052 owns the rule set. The code and that spec agree; a mismatch is a defect in whichever is stale.
//!
//! R01 file names
//! R02 frontmatter
//! R03 depends_on stems
//! R04 H1 equals the file stem
//! R05 H2 sections in order
//! R06 fenced block in Test or Verification
//! R07 a requirement with shall
//! R08 ambiguous modals
//! R09 EARS keyword at the start of a bullet
//! R10 line count
//! R11 README lists every spec
//! R12 relative links
//!
//! Changing a rule: amend the spec 0052 tables, add a unit test that fails without the change.
//!
//! A missing or empty `specs/` directory passes with a notice.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::ExitCode;

const STATUSES: &[&str] = &["planned", "in-progress", "complete", "archived"];
const PRIORITIES: &[&str] = &["low", "medium", "high", "critical"];
const KEYS: &[&str] = &[
    "status",
    "created",
    "tags",
    "priority",
    "depends_on",
    "scope",
];
const MODERN_SECTIONS: &[&str] = &["Requirements", "Design", "Tasks", "Verification", "Notes"];
const LEGACY_SECTIONS: &[&str] = &["Overview", "Design", "Plan", "Test", "Notes"];
const WARN_LINES: usize = 300;
const ERROR_LINES: usize = 400;

#[derive(Debug, Default)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub notice: Option<String>,
}

pub fn run(root: &Path) -> ExitCode {
    let report = check(root);
    if let Some(notice) = &report.notice {
        println!("spec-check: {notice}");
    }
    for w in &report.warnings {
        println!("warning: {w}");
    }
    for e in &report.errors {
        eprintln!("error: {e}");
    }
    if report.errors.is_empty() {
        println!("spec-check: ok ({} warning(s))", report.warnings.len());
        ExitCode::SUCCESS
    } else {
        eprintln!("spec-check: {} error(s)", report.errors.len());
        ExitCode::FAILURE
    }
}

/// Check `<root>/specs`. Paths in messages are relative to `root`.
pub fn check(root: &Path) -> Report {
    let mut report = Report::default();
    let dir = root.join("specs");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".md"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    if names.is_empty() {
        report.notice =
            Some("no specs found in specs/ (directory missing or empty); nothing to check".into());
        return report;
    }

    // R01: names and unique numbers.
    let mut stems: BTreeSet<String> = BTreeSet::new();
    let mut numbers: BTreeMap<String, String> = BTreeMap::new();
    let mut spec_names: Vec<String> = Vec::new();
    for name in names.iter().filter(|n| n.as_str() != "README.md") {
        let rel = format!("specs/{name}");
        let Some(number) = spec_number(name) else {
            report
                .errors
                .push(format!("{rel}:1: file name must be NNNN-kebab-slug.md"));
            continue;
        };
        if let Some(first) = numbers.get(&number) {
            report.errors.push(format!(
                "{rel}:1: number {number} is already used by {first}"
            ));
            continue;
        }
        numbers.insert(number, name.clone());
        stems.insert(name.trim_end_matches(".md").to_string());
        spec_names.push(name.clone());
    }

    let mut statuses: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for name in &spec_names {
        let rel = format!("specs/{name}");
        match fs::read_to_string(dir.join(name)) {
            Ok(text) => {
                if let Some(status) = check_spec(
                    &rel,
                    &text,
                    name.trim_end_matches(".md"),
                    &stems,
                    &mut report,
                ) {
                    statuses.insert(name.clone(), status);
                }
            }
            Err(err) => report
                .errors
                .push(format!("{rel}:1: cannot read file: {err}")),
        }
        if let Ok(text) = fs::read_to_string(dir.join(name)) {
            check_links(&rel, &dir, &text, &mut report);
        }
    }

    // Rules for README.md.
    let readme = dir.join("README.md");
    match fs::read_to_string(&readme) {
        Ok(text) => {
            check_links("specs/README.md", &dir, &text, &mut report);
            check_readme(&text, &spec_names, &statuses, &mut report);
        }
        Err(_) => {
            if !spec_names.is_empty() {
                report
                    .errors
                    .push("specs/README.md:1: file is missing".into());
            }
        }
    }

    // R12 over plans (spec 0052 D7): relative links in engineering/plans/*.md.
    let plans = root.join("engineering/plans");
    let mut plan_names: Vec<String> = fs::read_dir(&plans)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".md"))
                .collect()
        })
        .unwrap_or_default();
    plan_names.sort();
    for name in &plan_names {
        if let Ok(text) = fs::read_to_string(plans.join(name)) {
            check_links(
                &format!("engineering/plans/{name}"),
                &plans,
                &text,
                &mut report,
            );
            if name != "README.md" {
                check_plan_shape(&format!("engineering/plans/{name}"), &text, &mut report);
            }
        }
    }
    report
}

/// R14 (spec 0052 D12): each `### P<n>` package in a plan holds a Persona,
/// Files, Done-when and Red-first label. A package whose heading carries the
/// word DONE, or whose section carries the word BLOCKED, is exempt. Warning only.
fn check_plan_shape(rel: &str, text: &str, report: &mut Report) {
    const LABELS: [(&str, &str); 4] = [
        ("**persona", "Persona"),
        ("**files", "Files"),
        ("**done-when", "Done-when"),
        ("**red-first", "Red-first"),
    ];
    let raw: Vec<&str> = text.lines().collect();
    let lines = classify(&raw, 1);
    let is_package = |l: &Line| {
        !l.inside
            && l.text
                .strip_prefix("### P")
                .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
    };
    let mut i = 0;
    while i < lines.len() {
        if !is_package(&lines[i]) {
            i += 1;
            continue;
        }
        let head = &lines[i];
        let mut end = i + 1;
        while end < lines.len()
            && (lines[end].inside
                || !(lines[end].text.starts_with("## ") || lines[end].text.starts_with("### ")))
        {
            end += 1;
        }
        let section = &lines[i..end];
        i = end;
        let body = section
            .iter()
            .map(|l| l.text)
            .collect::<Vec<_>>()
            .join("\n");
        if contains_word(head.text, "DONE") || contains_word(&body, "BLOCKED") {
            continue;
        }
        let lower = body.to_lowercase();
        if let Some((_, name)) = LABELS.iter().find(|(key, _)| !lower.contains(key)) {
            let id = head.text["### ".len()..]
                .split(|c: char| !c.is_ascii_alphanumeric())
                .next()
                .unwrap_or("P");
            report
                .warnings
                .push(format!("{rel}:{}: package {id} lacks {name}", head.no));
        }
    }
}

/// Return the 4-digit number if `name` is `NNNN-kebab-slug.md`.
fn spec_number(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".md")?;
    let (num, slug) = stem.split_once('-')?;
    let digits = num.len() == 4 && num.chars().all(|c| c.is_ascii_digit());
    let slug_ok = !slug.is_empty()
        && slug.split('-').all(|seg| {
            !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        });
    (digits && slug_ok).then(|| num.to_string())
}

/// One source line with its 1-based number and fence state.
struct Line<'a> {
    no: usize,
    text: &'a str,
    /// True for the fence marker lines themselves.
    marker: bool,
    /// True for marker lines and everything between them.
    inside: bool,
}

fn classify<'a>(lines: &[&'a str], first_no: usize) -> Vec<Line<'a>> {
    let mut open = false;
    let mut out = Vec::with_capacity(lines.len());
    for (i, text) in lines.iter().enumerate() {
        let t = text.trim_start();
        let marker = t.starts_with("```") || t.starts_with("~~~");
        let inside = open || marker;
        if marker {
            open = !open;
        }
        out.push(Line {
            no: first_no + i,
            text,
            marker,
            inside,
        });
    }
    out
}

#[derive(Debug)]
enum Value {
    Scalar(String),
    List(Vec<String>),
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
            return inner.to_string();
        }
    }
    s.to_string()
}

/// Parse the frontmatter lines (between the `---` markers).
/// `first_no` is the line number of the first entry.
fn parse_frontmatter(
    rel: &str,
    lines: &[&str],
    first_no: usize,
    report: &mut Report,
) -> Vec<(String, Value, usize)> {
    let mut entries: Vec<(String, Value, usize)> = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        let no = first_no + i;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if raw.starts_with(char::is_whitespace) && trimmed.starts_with('-') {
            let item = unquote(trimmed.trim_start_matches('-'));
            match entries.last_mut() {
                Some((_, Value::List(items), _)) => items.push(item),
                _ => report
                    .errors
                    .push(format!("{rel}:{no}: list item without a list key")),
            }
            continue;
        }
        let Some((key, val)) = trimmed.split_once(':') else {
            report
                .errors
                .push(format!("{rel}:{no}: cannot parse frontmatter line"));
            continue;
        };
        let key = key.trim().to_string();
        let val = val.trim();
        let value = if val.is_empty() {
            Value::List(Vec::new())
        } else if let Some(inner) = val.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            Value::List(
                inner
                    .split(',')
                    .map(unquote)
                    .filter(|s| !s.is_empty())
                    .collect(),
            )
        } else {
            Value::Scalar(unquote(val))
        };
        if entries.iter().any(|(k, _, _)| *k == key) {
            report
                .errors
                .push(format!("{rel}:{no}: duplicate key `{key}`"));
            continue;
        }
        entries.push((key, value, no));
    }
    entries
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| s[r].chars().all(|c| c.is_ascii_digit());
    if !(digits(0..4) && digits(5..7) && digits(8..10)) {
        return false;
    }
    let month: u32 = s[5..7].parse().unwrap_or(0);
    let day: u32 = s[8..10].parse().unwrap_or(0);
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

/// Returns the frontmatter `status` value and its line, when the frontmatter parses.
fn check_spec(
    rel: &str,
    text: &str,
    stem: &str,
    stems: &BTreeSet<String>,
    report: &mut Report,
) -> Option<(String, usize)> {
    let lines: Vec<&str> = text.lines().collect();

    // Length limit.
    if lines.len() > ERROR_LINES {
        report.errors.push(format!(
            "{rel}:{}: {} lines is over the {ERROR_LINES}-line limit; split into sub-spec files",
            lines.len(),
            lines.len()
        ));
    } else if lines.len() > WARN_LINES {
        report.warnings.push(format!(
            "{rel}:{}: {} lines is over {WARN_LINES}; consider splitting",
            lines.len(),
            lines.len()
        ));
    }

    // Frontmatter.
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        report.errors.push(format!(
            "{rel}:1: missing frontmatter (first line must be ---)"
        ));
        return None;
    }
    let Some(end) = lines
        .iter()
        .skip(1)
        .position(|l| l.trim_end() == "---")
        .map(|p| p + 1)
    else {
        report
            .errors
            .push(format!("{rel}:1: frontmatter is not closed by a --- line"));
        return None;
    };
    let entries = parse_frontmatter(rel, &lines[1..end], 2, report);
    check_entries(rel, &entries, stems, report);

    // Body headings.
    let body = classify(&lines[end + 1..], end + 2);
    let h1: Vec<&Line> = body
        .iter()
        .filter(|l| !l.inside && l.text.starts_with("# "))
        .collect();
    match h1.as_slice() {
        [] => report
            .errors
            .push(format!("{rel}:{}: missing H1 `# {stem}`", end + 2)),
        [first, rest @ ..] => {
            if first.text.trim_end() != format!("# {stem}") {
                report.errors.push(format!(
                    "{rel}:{}: H1 must be `# {stem}`, found `{}`",
                    first.no,
                    first.text.trim_end()
                ));
            }
            for extra in rest {
                report
                    .errors
                    .push(format!("{rel}:{}: more than one H1", extra.no));
            }
        }
    }

    let h2: Vec<(usize, &str)> = body
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.inside && l.text.starts_with("## "))
        .map(|(i, l)| (i, l.text[3..].trim()))
        .collect();

    let is_modern = h2.iter().any(|(_, h)| *h == "Requirements");
    let (expected_sections, verif_section) = if is_modern {
        (MODERN_SECTIONS, "Verification")
    } else {
        (LEGACY_SECTIONS, "Test")
    };

    let mut last_pos: Option<usize> = None;
    for section in expected_sections {
        match h2.iter().position(|(_, h)| h == section) {
            None => report
                .errors
                .push(format!("{rel}:1: missing section `## {section}`")),
            Some(pos) => {
                if last_pos.is_some_and(|p| pos < p) {
                    report.errors.push(format!(
                        "{rel}:{}: section `## {section}` is out of order",
                        body[h2[pos].0].no
                    ));
                }
                last_pos = Some(last_pos.map_or(pos, |p| p.max(pos)));
            }
        }
    }

    if let Some(pos) = h2.iter().position(|(_, h)| *h == verif_section) {
        let from = h2[pos].0 + 1;
        let to = h2.get(pos + 1).map_or(body.len(), |(i, _)| *i);
        if !body[from..to].iter().any(|l| l.marker) {
            report.errors.push(format!(
                "{rel}:{}: `## {verif_section}` needs a fenced code block with a runnable command",
                body[h2[pos].0].no
            ));
        }
    }

    if is_modern {
        if let Some(pos) = h2.iter().position(|(_, h)| *h == "Requirements") {
            let from = h2[pos].0 + 1;
            let to = h2.get(pos + 1).map_or(body.len(), |(i, _)| *i);
            check_ears_requirements(rel, &body[from..to], body[h2[pos].0].no, false, report);
        }
    } else if let Some(pos) = body
        .iter()
        .position(|l| !l.inside && l.text.trim_end() == "### Requirements (EARS)")
    {
        // Legacy layout: the block runs to the next H2 or H3 outside a fence.
        let from = pos + 1;
        let to = body[from..]
            .iter()
            .position(|l| !l.inside && (l.text.starts_with("## ") || l.text.starts_with("### ")))
            .map_or(body.len(), |p| from + p);
        check_ears_requirements(rel, &body[from..to], body[pos].no, true, report);
    }
    entries.iter().find_map(|(k, v, no)| match (k.as_str(), v) {
        ("status", Value::Scalar(v)) => Some((v.clone(), *no)),
        _ => None,
    })
}

fn contains_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|w| w.eq_ignore_ascii_case(word))
}

fn is_ears_prefix(line: &str) -> bool {
    let mut s = line.trim();
    if s.starts_with('*') || s.starts_with('-') {
        s = s[1..].trim();
    }
    if let Some(rest) = s.strip_prefix("**") {
        if let Some(idx) = rest.find("**") {
            s = rest[idx + 2..].trim();
            if let Some(rest2) = s.strip_prefix(':') {
                s = rest2.trim();
            }
            // House form: `**[REQ-NNNN-NN]** (Pattern): ...`.
            if let Some(rest2) = s.strip_prefix('(') {
                if let Some(close) = rest2.find(')') {
                    s = rest2[close + 1..].trim();
                    if let Some(rest3) = s.strip_prefix(':') {
                        s = rest3.trim();
                    }
                }
            }
        }
    }
    let lower = s.to_ascii_lowercase();
    lower.starts_with("the ")
        || lower.starts_with("when ")
        || lower.starts_with("while ")
        || lower.starts_with("if ")
        || lower.starts_with("where ")
}

/// Strip inline code spans enclosed in backticks (`).
fn strip_inline_code(line: &str) -> String {
    let mut stripped = String::with_capacity(line.len());
    let mut in_code = false;
    for c in line.chars() {
        if c == '`' {
            in_code = !in_code;
        } else if !in_code {
            stripped.push(c);
        }
    }
    stripped
}

fn check_ears_requirements(
    rel: &str,
    lines: &[Line],
    heading_no: usize,
    legacy: bool,
    report: &mut Report,
) {
    let mut shall_count = 0;
    for l in lines {
        if l.inside {
            continue;
        }
        let clean = strip_inline_code(l.text);
        if contains_word(&clean, "shall") {
            shall_count += 1;
            for bad in ["should", "could", "might", "may"] {
                if contains_word(&clean, bad) {
                    report.warnings.push(format!(
                        "{rel}:{}: requirement contains ambiguous modal '{bad}'; use 'shall' for mandatory requirements",
                        l.no
                    ));
                }
            }
            let trimmed = clean.trim();
            if (trimmed.starts_with('*') || trimmed.starts_with('-')) && !is_ears_prefix(trimmed) {
                report.warnings.push(format!(
                    "{rel}:{}: requirement clause does not begin with a standard EARS pattern keyword (The, When, While, If, Where)",
                    l.no
                ));
            }
        }
    }
    if shall_count == 0 {
        let msg = format!(
            "{rel}:{heading_no}: `## Requirements` needs at least one EARS requirement clause containing 'shall'"
        );
        // Legacy blocks only warn (spec 0052 D5).
        if legacy {
            report.warnings.push(msg);
        } else {
            report.errors.push(msg);
        }
    }
}

fn check_entries(
    rel: &str,
    entries: &[(String, Value, usize)],
    stems: &BTreeSet<String>,
    report: &mut Report,
) {
    for (key, value, no) in entries {
        match (key.as_str(), value) {
            ("status", Value::Scalar(v)) => {
                if !STATUSES.contains(&v.as_str()) {
                    report.errors.push(format!(
                        "{rel}:{no}: status `{v}` must be one of {}",
                        STATUSES.join(", ")
                    ));
                }
            }
            ("priority", Value::Scalar(v)) => {
                if !PRIORITIES.contains(&v.as_str()) {
                    report.errors.push(format!(
                        "{rel}:{no}: priority `{v}` must be one of {}",
                        PRIORITIES.join(", ")
                    ));
                }
            }
            ("created", Value::Scalar(v)) => {
                if !valid_date(v) {
                    report
                        .errors
                        .push(format!("{rel}:{no}: created `{v}` must be YYYY-MM-DD"));
                }
            }
            ("tags", Value::List(_)) => {}
            ("scope", Value::List(_)) => {}
            ("depends_on", Value::List(items)) => {
                for dep in items {
                    if !stems.contains(dep) {
                        report
                            .errors
                            .push(format!("{rel}:{no}: depends_on `{dep}` is not a spec"));
                    }
                }
            }
            (k, _) if KEYS.contains(&k) => {
                let want = if matches!(k, "tags" | "depends_on" | "scope") {
                    "a list"
                } else {
                    "a single value"
                };
                report
                    .errors
                    .push(format!("{rel}:{no}: `{k}` must be {want}"));
            }
            (k, _) => report
                .errors
                .push(format!("{rel}:{no}: unknown frontmatter key `{k}`")),
        }
    }
    for required in ["status", "created"] {
        if !entries.iter().any(|(k, _, _)| k == required) {
            report
                .errors
                .push(format!("{rel}:1: missing required key `{required}`"));
        }
    }
}

/// Link targets in one line, ignoring inline code spans.
fn line_links(line: &str) -> Vec<String> {
    let stripped = strip_inline_code(line);
    let mut out = Vec::new();
    let mut rest = stripped.as_str();
    while let Some(i) = rest.find("](") {
        rest = &rest[i + 2..];
        if let Some(j) = rest.find(')') {
            // Drop an optional link title.
            if let Some(target) = rest[..j].split_whitespace().next() {
                out.push(target.to_string());
            }
            rest = &rest[j..];
        }
    }
    out
}

/// A relative path target, or None for URLs and pure anchors.
fn relative_target(target: &str) -> Option<&str> {
    let skip = ["http://", "https://", "mailto:", "#"];
    if skip.iter().any(|p| target.starts_with(p)) {
        return None;
    }
    let path = target.split('#').next().unwrap_or("");
    (!path.is_empty()).then_some(path)
}

/// R12: relative links must resolve.
fn check_links(rel: &str, dir: &Path, text: &str, report: &mut Report) {
    let lines: Vec<&str> = text.lines().collect();
    for line in classify(&lines, 1).iter().filter(|l| !l.inside) {
        for target in line_links(line.text) {
            if let Some(path) = relative_target(&target) {
                if !dir.join(path).exists() {
                    report
                        .errors
                        .push(format!("{rel}:{}: broken link `{target}`", line.no));
                }
            }
        }
    }
}

/// R11: README lists every spec file once and no missing file.
/// R13: the Status cell of each row equals the frontmatter `status` (spec 0052 D12).
fn check_readme(
    text: &str,
    spec_names: &[String],
    statuses: &BTreeMap<String, (String, usize)>,
    report: &mut Report,
) {
    let rel = "specs/README.md";
    let lines: Vec<&str> = text.lines().collect();
    let mut listed: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut cells: BTreeMap<String, String> = BTreeMap::new();
    let mut status_col: Option<usize> = None;
    let mut header_seen = false;
    for line in classify(&lines, 1).iter().filter(|l| !l.inside) {
        let row: Vec<&str> = line
            .text
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if line.text.trim_start().starts_with('|') && !header_seen {
            header_seen = true;
            status_col = row.iter().position(|c| *c == "Status");
            if status_col.is_none() && !spec_names.is_empty() {
                report
                    .errors
                    .push(format!("{rel}:{}: no Status column", line.no));
            }
        }
        for target in line_links(line.text) {
            let Some(path) = relative_target(&target) else {
                continue;
            };
            let name = path.strip_prefix("./").unwrap_or(path);
            if spec_number(name).is_some() {
                listed.entry(name.to_string()).or_default().push(line.no);
                if let Some(cell) = status_col.and_then(|c| row.get(c)) {
                    cells
                        .entry(name.to_string())
                        .or_insert_with(|| (*cell).to_string());
                }
            }
        }
    }
    for name in spec_names {
        match listed.get(name).map_or(0, Vec::len) {
            0 => report.errors.push(format!("{rel}:1: {name} is not listed")),
            1 => {}
            n => report.errors.push(format!(
                "{rel}:{}: {name} is listed {n} times",
                listed[name][1]
            )),
        }
    }
    for (name, (status, no)) in statuses {
        if let Some(cell) = cells.get(name) {
            if cell != status {
                report.errors.push(format!(
                    "specs/{name}:{no}: status `{status}` differs from the specs/README.md row `{cell}`"
                ));
            }
        }
    }
    for (name, nos) in &listed {
        if !spec_names.contains(name) {
            report.errors.push(format!(
                "{rel}:{}: lists {name} but the file does not exist",
                nos[0]
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct Tmp(std::path::PathBuf);

    impl Tmp {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let p = std::env::temp_dir().join(format!("xtask-spec-{}-{n}", std::process::id()));
            fs::create_dir_all(p.join("specs")).unwrap();
            Self(p)
        }
        fn write(&self, rel: &str, body: &str) {
            fs::write(self.0.join(rel), body).unwrap();
        }
        fn errors(&self) -> Vec<String> {
            check(&self.0).errors
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn spec(stem: &str) -> String {
        format!(
            "---\nstatus: planned\ncreated: 2026-10-08\ntags: [a, b]\npriority: high\n---\n\n# {stem}\n\n\
             ## Overview\n\nText.\n\n## Design\n\nText.\n\n## Plan\n\n- [ ] one\n\n\
             ## Test\n\n```bash\nmake check\n```\n\n## Notes\n\nText.\n"
        )
    }

    fn modern_spec(stem: &str) -> String {
        format!(
            "---\nstatus: planned\ncreated: 2026-10-09\ntags: [a, b]\npriority: high\nscope: [core]\n---\n\n# {stem}\n\n\
             ## Requirements\n\n* **Ubiquitous:** The system shall operate continuously.\n\n## Design\n\nText.\n\n## Tasks\n\n- [ ] one\n\n\
             ## Verification\n\n```bash\nmake check\n```\n\n## Notes\n\nText.\n"
        )
    }

    fn readme(names: &[&str]) -> String {
        let mut s =
            String::from("# Specs\n\n| Spec | Title | Status | Priority |\n|---|---|---|---|\n");
        for n in names {
            s.push_str(&format!("| [{n}]({n}) | t | planned | high |\n"));
        }
        s
    }

    /// A tree with specs 0001 and 0002 and a matching README.
    fn good() -> Tmp {
        let t = Tmp::new();
        t.write("specs/0001-first.md", &spec("0001-first"));
        t.write("specs/0002-second.md", &spec("0002-second"));
        t.write(
            "specs/README.md",
            &readme(&["0001-first.md", "0002-second.md"]),
        );
        t
    }

    fn has(errors: &[String], needle: &str) -> bool {
        errors.iter().any(|e| e.contains(needle))
    }

    #[test]
    fn good_tree_passes() {
        let t = good();
        let r = check(&t.0);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn modern_ears_spec_passes() {
        let t = Tmp::new();
        t.write("specs/0001-first.md", &spec("0001-first"));
        t.write("specs/0002-second.md", &modern_spec("0002-second"));
        t.write(
            "specs/README.md",
            &readme(&["0001-first.md", "0002-second.md"]),
        );
        let r = check(&t.0);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn modern_ears_spec_missing_shall_fails() {
        let t = Tmp::new();
        let bad = modern_spec("0001-bad").replace(
            "The system shall operate continuously.",
            "The system operates continuously.",
        );
        t.write("specs/0001-bad.md", &bad);
        t.write("specs/README.md", &readme(&["0001-bad.md"]));
        let r = check(&t.0);
        assert!(has(
            &r.errors,
            "needs at least one EARS requirement clause containing 'shall'"
        ));
    }

    #[test]
    fn modern_ears_spec_ambiguous_modal_warns() {
        let t = Tmp::new();
        let with_warning = modern_spec("0001-warn").replace(
            "The system shall operate continuously.",
            "* The system shall operate continuously and should handle faults.",
        );
        t.write("specs/0001-warn.md", &with_warning);
        t.write("specs/README.md", &readme(&["0001-warn.md"]));
        let r = check(&t.0);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r
            .warnings
            .iter()
            .any(|w| w.contains("contains ambiguous modal 'should'")));
    }

    #[test]
    fn modern_ears_spec_backticked_modals_do_not_warn() {
        let t = Tmp::new();
        let with_code = modern_spec("0001-code").replace(
            "The system shall operate continuously.",
            "The system shall flag modals like `should`, `could`, and `may`.",
        );
        t.write("specs/0001-code.md", &with_code);
        t.write("specs/README.md", &readme(&["0001-code.md"]));
        let r = check(&t.0);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    /// A legacy spec whose Overview holds a `### Requirements (EARS)` block.
    fn legacy_ears_spec(stem: &str, block: &str) -> String {
        spec(stem).replace(
            "## Overview\n\nText.\n",
            &format!("## Overview\n\nText.\n\n### Requirements (EARS)\n\n{block}\n\n### Other\n\n- Systems will run.\n"),
        )
    }

    fn warnings_for(name: &str, body: &str) -> (Vec<String>, Vec<String>) {
        let t = Tmp::new();
        t.write(&format!("specs/{name}.md"), body);
        t.write("specs/README.md", &readme(&[&format!("{name}.md")]));
        let r = check(&t.0);
        (r.errors, r.warnings)
    }

    #[test]
    fn house_form_bullet_draws_no_keyword_warning() {
        let bad = modern_spec("0001-hf").replace(
            "* **Ubiquitous:** The system shall operate continuously.",
            "- **[REQ-0001-01]** (Ubiquitous): The system shall run.",
        );
        let (e, w) = warnings_for("0001-hf", &bad);
        assert!(e.is_empty(), "{e:?}");
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn house_form_bullet_with_bad_keyword_still_warns() {
        let bad = modern_spec("0001-hf").replace(
            "* **Ubiquitous:** The system shall operate continuously.",
            "- **[REQ-0001-01]** (Ubiquitous): Systems shall run.",
        );
        let (e, w) = warnings_for("0001-hf", &bad);
        assert!(e.is_empty(), "{e:?}");
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("does not begin with a standard EARS pattern keyword"));
    }

    #[test]
    fn legacy_ears_block_bad_keyword_warns_not_errors() {
        let body = legacy_ears_spec("0001-lg", "- Systems shall run.");
        let (e, w) = warnings_for("0001-lg", &body);
        assert!(e.is_empty(), "{e:?}");
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("does not begin with a standard EARS pattern keyword"));
    }

    #[test]
    fn legacy_ears_block_ambiguous_modal_warns() {
        let body = legacy_ears_spec("0001-lg", "- The system shall run and may stop.");
        let (e, w) = warnings_for("0001-lg", &body);
        assert!(e.is_empty(), "{e:?}");
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("ambiguous modal 'may'"));
    }

    #[test]
    fn legacy_ears_block_without_shall_warns_not_errors() {
        let body = legacy_ears_spec("0001-lg", "- The system will run.");
        let (e, w) = warnings_for("0001-lg", &body);
        assert!(e.is_empty(), "{e:?}");
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("needs at least one EARS requirement clause containing 'shall'"));
    }

    #[test]
    fn legacy_ears_house_form_and_backticked_modal_do_not_warn() {
        let body = legacy_ears_spec(
            "0001-lg",
            "- **[REQ-0001-01]** (Ubiquitous): The system shall flag `may` and `should`.",
        );
        let (e, w) = warnings_for("0001-lg", &body);
        assert!(e.is_empty(), "{e:?}");
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn legacy_spec_without_ears_block_draws_nothing() {
        let (e, w) = warnings_for("0001-lg", &spec("0001-lg"));
        assert!(e.is_empty(), "{e:?}");
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn missing_or_empty_specs_dir_passes_with_notice() {
        let t = Tmp::new();
        let r = check(&t.0);
        assert!(r.errors.is_empty());
        assert!(r.notice.unwrap().contains("no specs"));
        fs::remove_dir_all(t.0.join("specs")).unwrap();
        let r = check(&t.0);
        assert!(r.errors.is_empty());
        assert!(r.notice.is_some());
    }

    #[test]
    fn bad_file_name_fails() {
        let t = good();
        t.write("specs/12-short.md", &spec("12-short"));
        t.write("specs/0003-Upper.md", &spec("0003-Upper"));
        let e = t.errors();
        assert!(has(&e, "specs/12-short.md:1: file name"));
        assert!(has(&e, "specs/0003-Upper.md:1: file name"));
    }

    #[test]
    fn duplicate_number_fails() {
        let t = good();
        t.write("specs/0001-other.md", &spec("0001-other"));
        assert!(has(&t.errors(), "number 0001 is already used"));
    }

    #[test]
    fn frontmatter_rules() {
        let t = good();
        let base = spec("0001-first");
        t.write(
            "specs/0001-first.md",
            &base.replace("status: planned", "status: wip"),
        );
        assert!(has(&t.errors(), "status `wip`"));
        t.write(
            "specs/0001-first.md",
            &base.replace("2026-10-08", "2026-13-08"),
        );
        assert!(has(&t.errors(), "created `2026-13-08`"));
        t.write(
            "specs/0001-first.md",
            &base.replace("priority: high", "priority: urgent"),
        );
        assert!(has(&t.errors(), "priority `urgent`"));
        t.write(
            "specs/0001-first.md",
            &base.replace("tags: [a, b]", "owner: me"),
        );
        assert!(has(&t.errors(), "unknown frontmatter key `owner`"));
        t.write(
            "specs/0001-first.md",
            &base.replace("status: planned\n", ""),
        );
        assert!(has(&t.errors(), "missing required key `status`"));
        t.write(
            "specs/0001-first.md",
            &base.replace("created: 2026-10-08\n", ""),
        );
        assert!(has(&t.errors(), "missing required key `created`"));
        t.write(
            "specs/0001-first.md",
            &base.replace("---\nstatus", "status"),
        );
        assert!(has(&t.errors(), "missing frontmatter"));
    }

    #[test]
    fn inline_list_form_is_accepted() {
        let t = good();
        let base = spec("0001-first");
        t.write(
            "specs/0001-first.md",
            &base.replace(
                "tags: [a, b]\npriority: high",
                "tags: [ci, runners]\ndepends_on: [0002-second]",
            ),
        );
        assert!(t.errors().is_empty(), "{:?}", t.errors());
        t.write(
            "specs/0001-first.md",
            &base.replace("tags: [a, b]", "depends_on: [0002-second, 0009-ghost]"),
        );
        assert!(has(&t.errors(), "depends_on `0009-ghost`"));
    }

    #[test]
    fn block_list_form_is_accepted() {
        let t = good();
        let base = spec("0001-first");
        t.write(
            "specs/0001-first.md",
            &base.replace(
                "tags: [a, b]\npriority: high",
                "tags:\n  - ci\n  - runners\ndepends_on:\n  - 0002-second",
            ),
        );
        assert!(t.errors().is_empty(), "{:?}", t.errors());
    }

    #[test]
    fn empty_lists_are_accepted() {
        let t = good();
        let base = spec("0001-first");
        t.write(
            "specs/0001-first.md",
            &base.replace("tags: [a, b]", "tags: []\ndepends_on: []"),
        );
        assert!(t.errors().is_empty(), "{:?}", t.errors());
        t.write(
            "specs/0001-first.md",
            &base.replace("tags: [a, b]", "tags:\ndepends_on:"),
        );
        assert!(t.errors().is_empty(), "{:?}", t.errors());
    }

    #[test]
    fn depends_on_must_exist() {
        let t = good();
        let base = spec("0001-first");
        t.write(
            "specs/0001-first.md",
            &base.replace(
                "priority: high",
                "depends_on:\n  - 0002-second\n  - 0009-ghost",
            ),
        );
        let e = t.errors();
        assert!(has(&e, "depends_on `0009-ghost`"));
        assert!(!has(&e, "depends_on `0002-second`"));
    }

    #[test]
    fn h1_must_match_stem() {
        let t = good();
        t.write(
            "specs/0001-first.md",
            &spec("0001-first").replace("# 0001-first", "# Wrong"),
        );
        assert!(has(&t.errors(), "H1 must be `# 0001-first`"));
        t.write(
            "specs/0001-first.md",
            &spec("0001-first").replace("## Plan", "# 0001-first\n\n## Plan"),
        );
        assert!(has(&t.errors(), "more than one H1"));
    }

    #[test]
    fn keyword_warning_on_non_ears_bullet() {
        let t = Tmp::new();
        let bad = modern_spec("0001-kw").replace(
            "* **Ubiquitous:** The system shall operate continuously.",
            "- Systems shall run.",
        );
        t.write("specs/0001-kw.md", &bad);
        t.write("specs/README.md", &readme(&["0001-kw.md"]));
        let r = check(&t.0);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(
            r.warnings,
            vec![
                "specs/0001-kw.md:13: requirement clause does not begin with a standard EARS pattern keyword (The, When, While, If, Where)"
                    .to_string()
            ]
        );
    }

    #[test]
    fn duplicate_key_fails() {
        let t = good();
        t.write(
            "specs/0001-first.md",
            &spec("0001-first").replace("status: planned\n", "status: planned\nstatus: planned\n"),
        );
        assert_eq!(
            t.errors(),
            vec!["specs/0001-first.md:3: duplicate key `status`".to_string()]
        );
    }

    #[test]
    fn unclosed_frontmatter_fails() {
        let t = good();
        t.write(
            "specs/0001-first.md",
            &spec("0001-first").replace("priority: high\n---\n", "priority: high\n"),
        );
        assert_eq!(
            t.errors(),
            vec!["specs/0001-first.md:1: frontmatter is not closed by a --- line".to_string()]
        );
    }

    #[test]
    fn list_and_scalar_type_errors() {
        let t = good();
        let base = spec("0001-first");
        t.write(
            "specs/0001-first.md",
            &base.replace("tags: [a, b]", "tags: a"),
        );
        assert_eq!(
            t.errors(),
            vec!["specs/0001-first.md:4: `tags` must be a list".to_string()]
        );
        t.write(
            "specs/0001-first.md",
            &base.replace("priority: high", "priority: [high]"),
        );
        assert_eq!(
            t.errors(),
            vec!["specs/0001-first.md:5: `priority` must be a single value".to_string()]
        );
    }

    #[test]
    fn missing_h1_fails() {
        let t = good();
        t.write(
            "specs/0001-first.md",
            &spec("0001-first").replace("# 0001-first\n\n", ""),
        );
        assert_eq!(
            t.errors(),
            vec!["specs/0001-first.md:7: missing H1 `# 0001-first`".to_string()]
        );
    }

    #[test]
    fn sections_required_and_ordered() {
        let t = good();
        t.write(
            "specs/0001-first.md",
            &spec("0001-first").replace("## Plan\n\n- [ ] one\n\n", ""),
        );
        assert!(has(&t.errors(), "missing section `## Plan`"));
        t.write(
            "specs/0001-first.md",
            &spec("0001-first")
                .replace("## Design", "## Tmp")
                .replace("## Notes", "## Design"),
        );
        assert!(has(&t.errors(), "is out of order"));
    }

    #[test]
    fn test_section_needs_fence() {
        let t = good();
        t.write(
            "specs/0001-first.md",
            &spec("0001-first").replace("```bash\nmake check\n```", "run make check"),
        );
        assert!(has(&t.errors(), "`## Test` needs a fenced code block"));
    }

    #[test]
    fn fenced_headings_are_ignored() {
        let t = good();
        t.write(
            "specs/0001-first.md",
            &spec("0001-first").replace("make check", "# not a heading\n## Overview"),
        );
        assert!(t.errors().is_empty(), "{:?}", t.errors());
    }

    #[test]
    fn length_limits() {
        let t = good();
        let pad = |n: usize| spec("0001-first") + &"filler\n".repeat(n);
        let base = spec("0001-first").lines().count();
        t.write("specs/0001-first.md", &pad(300 - base));
        let r = check(&t.0);
        assert!(
            r.errors.is_empty() && r.warnings.is_empty(),
            "300 lines is fine"
        );
        t.write("specs/0001-first.md", &pad(301 - base));
        let r = check(&t.0);
        assert!(r.errors.is_empty());
        assert_eq!(r.warnings.len(), 1, "301 lines warns");
        t.write("specs/0001-first.md", &pad(400 - base));
        assert!(check(&t.0).errors.is_empty(), "400 lines is a warning only");
        t.write("specs/0001-first.md", &pad(401 - base));
        assert!(has(&t.errors(), "over the 400-line limit"));
    }

    #[test]
    fn readme_must_list_each_spec_once() {
        let t = good();
        t.write("specs/README.md", &readme(&["0001-first.md"]));
        assert!(has(&t.errors(), "0002-second.md is not listed"));
        t.write(
            "specs/README.md",
            &readme(&["0001-first.md", "0002-second.md", "0001-first.md"]),
        );
        assert!(has(&t.errors(), "0001-first.md is listed 2 times"));
        t.write(
            "specs/README.md",
            &readme(&["0001-first.md", "0002-second.md", "0003-ghost.md"]),
        );
        let e = t.errors();
        assert!(has(&e, "lists 0003-ghost.md but the file does not exist"));
        assert!(has(&e, "broken link `0003-ghost.md`"));
        fs::remove_file(t.0.join("specs/README.md")).unwrap();
        assert!(has(&t.errors(), "specs/README.md:1: file is missing"));
    }

    #[test]
    fn links_are_checked() {
        let t = good();
        let with_link = |link: &str| {
            spec("0001-first").replace("Text.\n\n## Design", &format!("{link}\n\n## Design"))
        };
        t.write("specs/0001-first.md", &with_link("[x](../missing.md)"));
        assert!(has(
            &t.errors(),
            "specs/0001-first.md:12: broken link `../missing.md`"
        ));
        t.write(
            "specs/0001-first.md",
            &with_link("[x](0002-second.md#design)"),
        );
        assert!(t.errors().is_empty(), "{:?}", t.errors());
        t.write(
            "specs/0001-first.md",
            &with_link("[a](https://x.test/y) [b](mailto:a@b.test) [c](#top) `[d](nope.md)`"),
        );
        assert!(t.errors().is_empty(), "{:?}", t.errors());
        t.write("specs/0001-first.md", &with_link("```\n[x](nope.md)\n```"));
        assert!(t.errors().is_empty(), "{:?}", t.errors());
    }

    #[test]
    fn plan_links_are_checked() {
        let t = good();
        // A missing plans directory is skipped.
        assert!(t.errors().is_empty(), "{:?}", t.errors());
        fs::create_dir_all(t.0.join("engineering/plans")).unwrap();
        t.write("engineering/plans/0001-p.md", "# p\n\n[x](nope.md)\n");
        assert!(has(
            &t.errors(),
            "engineering/plans/0001-p.md:3: broken link `nope.md`"
        ));
        t.write(
            "engineering/plans/0001-p.md",
            "# p\n\n[x](../../specs/0001-first.md#design) [u](https://x.test) `[c](nope.md)`\n",
        );
        assert!(t.errors().is_empty(), "{:?}", t.errors());
    }

    fn plan_pkg(heading: &str, labels: &[&str]) -> String {
        let mut s = format!("# p\n\n{heading}\n");
        for l in labels {
            s.push_str(&format!("- **{l}:** x\n"));
        }
        s
    }

    fn plan_warnings(body: &str) -> Vec<String> {
        let t = good();
        fs::create_dir_all(t.0.join("engineering/plans")).unwrap();
        t.write("engineering/plans/0001-p.md", body);
        check(&t.0).warnings
    }

    #[test]
    fn plan_shape_complete_package_is_clean() {
        let w = plan_warnings(&plan_pkg(
            "### P1: a",
            &["Persona", "Files, exist", "Done-when", "Red-first"],
        ));
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn plan_shape_missing_field_warns_once_naming_file_line_and_field() {
        let w = plan_warnings(&plan_pkg("### P12: a", &["Persona", "Files", "Red-first"]));
        assert_eq!(
            w,
            ["engineering/plans/0001-p.md:3: package P12 lacks Done-when"]
        );
    }

    #[test]
    fn plan_shape_done_and_blocked_packages_are_exempt() {
        let w = plan_warnings(&plan_pkg("### P1: a (DONE 2026-10-11, #1)", &["Persona"]));
        assert!(w.is_empty(), "{w:?}");
        let w = plan_warnings(&plan_pkg("### P2: a (BLOCKED)", &["Persona"]));
        assert!(w.is_empty(), "{w:?}");
        let w = plan_warnings(&format!(
            "{}- Blocked by the spec line 12: BLOCKED.\n",
            plan_pkg("### P3: a", &["Persona"])
        ));
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn plan_shape_ignores_fenced_headings_readme_and_ends_at_next_heading() {
        let t = good();
        fs::create_dir_all(t.0.join("engineering/plans")).unwrap();
        t.write("engineering/plans/README.md", "### P1: a\n");
        t.write(
            "engineering/plans/0001-p.md",
            "# p\n\n```\n### P1: a\n```\n\n### P2: a\n- **Persona:** x\n- **Files:** x\n- **Done-when:** x\n- **Red-first:** x\n\n## Later\n\n### P3: b\n- **Persona:** x\n- **Files:** x\n- **Done-when:** x\n",
        );
        assert_eq!(
            check(&t.0).warnings,
            ["engineering/plans/0001-p.md:15: package P3 lacks Red-first"]
        );
    }

    #[test]
    fn status_parity_is_checked() {
        let t = good();
        t.write(
            "specs/README.md",
            &readme(&["0001-first.md", "0002-second.md"]).replacen(
                "0001-first.md) | t | planned",
                "0001-first.md) | t | in-progress",
                1,
            ),
        );
        let e = t.errors();
        assert!(
            has(
                &e,
                "specs/0001-first.md:2: status `planned` differs from the specs/README.md row `in-progress`"
            ),
            "{e:?}"
        );
        assert_eq!(e.len(), 1, "{e:?}");
    }

    #[test]
    fn status_parity_requires_a_status_column() {
        let t = good();
        t.write(
            "specs/README.md",
            "# Specs\n\n| Spec | Title |\n|---|---|\n| [0001-first.md](0001-first.md) | t |\n| [0002-second.md](0002-second.md) | t |\n",
        );
        assert!(has(&t.errors(), "specs/README.md:3: no Status column"));
    }

    #[test]
    fn helpers() {
        assert_eq!(spec_number("0001-a-b2.md").as_deref(), Some("0001"));
        assert!(spec_number("0001-.md").is_none());
        assert!(spec_number("0001-a--b.md").is_none());
        assert!(valid_date("2026-10-08"));
        assert!(!valid_date("2026-1-08"));
        assert!(!valid_date("2026-00-08"));
        assert_eq!(line_links("[a](x.md \"t\") [b](y.md)"), ["x.md", "y.md"]);
    }

    #[test]
    fn repository_specs_pass_check() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let report = check(root);
        assert!(
            report.errors.is_empty(),
            "repository specs must be valid: {:?}",
            report.errors
        );
    }
}
