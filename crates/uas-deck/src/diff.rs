// SPDX-License-Identifier: Apache-2.0 OR MIT
//! In-process diff engine and HTML presentation for candidate changesets (spec 0018 D4).
//!
//! Uses `similar::TextDiff` to compute unified line diffs, classify insertions,
//! deletions, and unchanged lines, and render side-by-side or line-numbered HTML tables
//! via Maud.

use maud::{html, Markup};
use serde::{Deserialize, Serialize};

/// Tag classifying one line of diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffTag {
    /// Line added in the modified candidate.
    Insert,
    /// Line removed from the base candidate.
    Delete,
    /// Line unchanged between base and candidate.
    Equal,
}

impl DiffTag {
    /// Return the diff marker character (`+`, `-`, or space).
    pub fn marker(&self) -> &'static str {
        match self {
            Self::Insert => "+",
            Self::Delete => "-",
            Self::Equal => " ",
        }
    }

    /// Return the CSS class name for styling the row.
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Insert => "diff-row-insert",
            Self::Delete => "diff-row-delete",
            Self::Equal => "diff-row-equal",
        }
    }
}

/// One formatted line of diff output with line numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    /// Classification of the diff line.
    pub tag: DiffTag,
    /// 1-indexed line number in base text, if present.
    pub old_lineno: Option<usize>,
    /// 1-indexed line number in modified text, if present.
    pub new_lineno: Option<usize>,
    /// Text content of the line (without trailing newline).
    pub content: String,
}

/// Aggregate counts of changes in a diff.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    /// Total inserted lines.
    pub insertions: usize,
    /// Total deleted lines.
    pub deletions: usize,
    /// Total unchanged lines.
    pub unchanged: usize,
}

/// Structured diff ready for template rendering or programmatic inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffTable {
    /// Sequenced lines of the diff.
    pub lines: Vec<DiffLine>,
    /// Aggregate insertion, deletion, and unchanged statistics.
    pub summary: DiffSummary,
}

/// Compute a line diff between base text and modified candidate text using `similar::TextDiff`.
pub fn compute_diff(base: &str, candidate: &str) -> DiffTable {
    let text_diff = similar::TextDiff::from_lines(base, candidate);
    let mut lines = Vec::new();
    let mut insertions = 0;
    let mut deletions = 0;
    let mut unchanged = 0;

    for change in text_diff.iter_all_changes() {
        let tag = match change.tag() {
            similar::ChangeTag::Insert => {
                insertions += 1;
                DiffTag::Insert
            }
            similar::ChangeTag::Delete => {
                deletions += 1;
                DiffTag::Delete
            }
            similar::ChangeTag::Equal => {
                unchanged += 1;
                DiffTag::Equal
            }
        };

        lines.push(DiffLine {
            tag,
            old_lineno: change.old_index().map(|i| i + 1),
            new_lineno: change.new_index().map(|i| i + 1),
            content: change.value().trim_end_matches(['\r', '\n']).to_string(),
        });
    }

    DiffTable {
        lines,
        summary: DiffSummary {
            insertions,
            deletions,
            unchanged,
        },
    }
}

/// Render a structured diff table to Maud HTML.
pub fn render_diff_table(diff: &DiffTable) -> Markup {
    html! {
        div class="diff-container" {
            div class="diff-stat" {
                span class="stat-insert" { "+" (diff.summary.insertions) }
                " "
                span class="stat-delete" { "-" (diff.summary.deletions) }
                " lines (" (diff.summary.unchanged) " unchanged)"
            }
            table class="diff-table" {
                thead {
                    tr {
                        th class="line-num" { "Base" }
                        th class="line-num" { "New" }
                        th class="marker" { " " }
                        th class="content-header" { "Content" }
                    }
                }
                tbody {
                    @for line in &diff.lines {
                        tr class=(line.tag.css_class()) {
                            td class="line-num" {
                                @if let Some(old) = line.old_lineno {
                                    (old)
                                }
                            }
                            td class="line-num" {
                                @if let Some(new) = line.new_lineno {
                                    (new)
                                }
                            }
                            td class="marker" { (line.tag.marker()) }
                            td class="content" {
                                code { (line.content) }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a modal diff fragment for a specific card ID.
pub fn render_diff_modal(card_id: &str, diff: &DiffTable) -> Markup {
    html! {
        div id=(format!("diff-modal-{}", card_id)) class="diff-modal" {
            div class="diff-modal-header" {
                h3 { "Changeset Diff: " (card_id) }
            }
            div class="diff-modal-body" {
                (render_diff_table(diff))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_insertions_deletions_and_unchanged_lines() {
        let base = "line1\nline2\nline3\n";
        let candidate = "line1\nline2 modified\nline3\nline4\n";

        let diff = compute_diff(base, candidate);

        assert_eq!(diff.summary.deletions, 1);
        assert_eq!(diff.summary.insertions, 2);
        assert_eq!(diff.summary.unchanged, 2);

        // line1: unchanged (equal)
        assert_eq!(diff.lines[0].tag, DiffTag::Equal);
        assert_eq!(diff.lines[0].content, "line1");
        assert_eq!(diff.lines[0].old_lineno, Some(1));
        assert_eq!(diff.lines[0].new_lineno, Some(1));

        // line2: deleted
        assert_eq!(diff.lines[1].tag, DiffTag::Delete);
        assert_eq!(diff.lines[1].content, "line2");
        assert_eq!(diff.lines[1].old_lineno, Some(2));
        assert_eq!(diff.lines[1].new_lineno, None);

        // line2 modified: inserted
        assert_eq!(diff.lines[2].tag, DiffTag::Insert);
        assert_eq!(diff.lines[2].content, "line2 modified");
        assert_eq!(diff.lines[2].old_lineno, None);
        assert_eq!(diff.lines[2].new_lineno, Some(2));

        // line3: unchanged (equal)
        assert_eq!(diff.lines[3].tag, DiffTag::Equal);
        assert_eq!(diff.lines[3].content, "line3");
        assert_eq!(diff.lines[3].old_lineno, Some(3));
        assert_eq!(diff.lines[3].new_lineno, Some(3));

        // line4: inserted
        assert_eq!(diff.lines[4].tag, DiffTag::Insert);
        assert_eq!(diff.lines[4].content, "line4");
        assert_eq!(diff.lines[4].old_lineno, None);
        assert_eq!(diff.lines[4].new_lineno, Some(4));
    }

    #[test]
    fn renders_diff_html_with_table_and_markers() {
        let base = "alpha\n";
        let candidate = "beta\n";
        let diff = compute_diff(base, candidate);
        let rendered = render_diff_table(&diff).into_string();

        assert!(rendered.contains("class=\"diff-table\""));
        assert!(rendered.contains("diff-row-delete"));
        assert!(rendered.contains("diff-row-insert"));
        assert!(rendered.contains("alpha"));
        assert!(rendered.contains("beta"));
        assert!(rendered.contains("+1"));
        assert!(rendered.contains("-1"));
    }
}
