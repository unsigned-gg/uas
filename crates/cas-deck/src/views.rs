//! Maud templates. Agent text is escaped. Only the static style is raw.

use cas_domain::EvidenceTier;
use cas_engine::{SessionState, SettleCard};
use maud::{html, Markup, PreEscaped, DOCTYPE};

use crate::SessionRow;

/// Evidence and layout styles. Static text only, no URLs.
const STYLE: &str = "body { font-family: sans-serif; margin: 2rem; color: #1a1a1a; } \
table { border-collapse: collapse; } \
td, th { border: 1px solid #ccc; padding: 0.3rem 0.6rem; text-align: left; } \
.badge { border-radius: 0.3rem; color: #fff; padding: 0.1rem 0.4rem; } \
.badge-verified { background: #2e7d32; } \
.badge-reported { background: #1565c0; } \
.badge-heuristic { background: #f9a825; color: #1a1a1a; } \
.badge-asserted { background: #757575; } \
.card { border: 1px solid #999; margin: 0.5rem 0; padding: 0.5rem; } \
.notice { border: 2px solid #b71c1c; color: #b71c1c; padding: 0.5rem; }";

/// Full page: the session table, then the settle queue.
pub fn page(rows: &[SessionRow], queue: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { "CAS deck" }
                style { (PreEscaped(STYLE)) }
            }
            body {
                h1 { "Sessions" }
                (session_table(rows))
                h2 { "Settle queue" }
                (queue)
            }
        }
    }
}

/// Session table. One row per session, with an evidence badge when one exists.
pub fn session_table(rows: &[SessionRow]) -> Markup {
    html! {
        @if rows.is_empty() {
            p { "No sessions." }
        } @else {
            table {
                thead {
                    tr {
                        th { "Session" }
                        th { "Slot" }
                        th { "State" }
                        th { "Activity" }
                        th { "Evidence" }
                        th { "Updated" }
                    }
                }
                tbody {
                    @for row in rows {
                        tr {
                            td { (row.session_id.as_str()) }
                            td { (row.slot_id.as_str()) }
                            td { (state_label(row.state)) }
                            td { (row.current_activity.as_deref().unwrap_or("-")) }
                            td {
                                @if let Some(tier) = row.last_evidence {
                                    (evidence_badge(tier))
                                } @else {
                                    "-"
                                }
                            }
                            td { (row.updated_at) }
                        }
                    }
                }
            }
        }
    }
}

/// Settle cards for the pending queue. Card text is escaped, and no card has an action yet.
pub fn settle_cards(cards: &[SettleCard]) -> Markup {
    html! {
        @if cards.is_empty() {
            p { "Nothing waits for a decision." }
        }
        @for card in cards {
            article class="card" {
                h3 { (card.title) }
                p { "Session " (card.session_id) ", step " (card.step_id) }
                p { (card.rationale) }
                @if let Some(diff) = &card.diff_summary {
                    p { "Diff: " (diff) }
                }
                p { "Created at " (card.created_at) }
            }
        }
    }
}

/// Notice shown when the settle queue cannot be reached. It carries no error text.
pub fn settle_disconnected() -> Markup {
    html! {
        p class="notice" { "Settle queue disconnected. No cards can be shown." }
    }
}

/// Evidence badge with class `badge-<tier>`.
pub fn evidence_badge(tier: EvidenceTier) -> Markup {
    html! {
        span class=(format!("badge badge-{}", tier.as_str())) { (tier.as_str()) }
    }
}

fn state_label(state: SessionState) -> &'static str {
    match state {
        SessionState::Unstarted => "unstarted",
        SessionState::Running => "running",
        SessionState::Compensating => "compensating",
        SessionState::Succeeded => "succeeded",
        SessionState::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cas_domain::SlotId;
    use cas_engine::SessionId;

    fn row(evidence: Option<EvidenceTier>) -> SessionRow {
        SessionRow {
            session_id: SessionId::new("sess-view").unwrap(),
            slot_id: SlotId::new("slot-view").unwrap(),
            state: SessionState::Running,
            current_activity: None,
            last_evidence: evidence,
            updated_at: 1,
        }
    }

    #[test]
    fn evidence_badge_classes() {
        let cases = [
            (EvidenceTier::Verified, "verified"),
            (EvidenceTier::Reported, "reported"),
            (EvidenceTier::Heuristic, "heuristic"),
            (EvidenceTier::Asserted, "asserted"),
        ];
        for (tier, name) in cases {
            let markup = evidence_badge(tier).into_string();
            let expected = format!("class=\"badge badge-{name}\"");
            assert!(markup.contains(&expected), "{markup}");
        }
    }

    #[test]
    fn settle_title_is_escaped() {
        let card = SettleCard::new(
            "card-x",
            "sess",
            "step",
            "<script>alert(1)</script>",
            "why",
            1,
        );
        let markup = settle_cards(&[card]).into_string();
        assert!(markup.contains("&lt;script&gt;"), "{markup}");
        assert!(!markup.contains("<script>"), "{markup}");
    }

    #[test]
    fn page_has_no_external_url() {
        let card = SettleCard::new("card-y", "sess", "step", "Push fix", "why", 1);
        let markup =
            page(&[row(Some(EvidenceTier::Verified))], settle_cards(&[card])).into_string();
        assert!(!markup.contains("http:"), "{markup}");
        assert!(!markup.contains("https:"), "{markup}");
        assert!(!markup.contains("//"), "{markup}");
    }
}
