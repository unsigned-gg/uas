//! Maud templates. Agent text is escaped. Only the static style is raw.

use maud::{html, Markup, PreEscaped, DOCTYPE};
use uas_domain::EvidenceTier;
use uas_engine::SessionState;

use crate::{DeckCard, Guard, PendingCards, SessionRow};

/// Evidence, layout and card styles. Static text only, no URLs.
const STYLE: &str = "body { font-family: sans-serif; margin: 2rem; color: #1a1a1a; } \
table { border-collapse: collapse; } \
td, th { border: 1px solid #ccc; padding: 0.3rem 0.6rem; text-align: left; } \
.badge { border-radius: 0.3rem; color: #fff; padding: 0.1rem 0.4rem; } \
.badge-verified { background: #2e7d32; } \
.badge-reported { background: #1565c0; } \
.badge-heuristic { background: #f9a825; color: #1a1a1a; } \
.badge-asserted { background: #757575; } \
.card { border: 1px solid #999; margin: 0.5rem 0; padding: 0.5rem; } \
.card-attention { border: 2px solid #f9a825; } \
.guard-allow { color: #2e7d32; } \
.guard-deny { color: #b71c1c; font-weight: bold; } \
.notice { border: 2px solid #b71c1c; color: #b71c1c; padding: 0.5rem; }";

/// Full page: the session table, then the settle queue.
pub fn page(rows: &[SessionRow], queue: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { "UAS deck" }
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
/// A count notice shows when the source reports a total other than the cards shown.
pub fn settle_cards(queue: &PendingCards) -> Markup {
    html! {
        @if queue.total != queue.cards.len() {
            p class="notice" {
                "Cortex reports " (queue.total) " pending. The deck shows " (queue.cards.len()) "."
            }
        }
        @if queue.total == 0 && queue.cards.is_empty() {
            p { "Nothing waits for a decision." }
        }
        @for card in &queue.cards {
            (settle_card(card))
        }
    }
}

fn settle_card(card: &DeckCard) -> Markup {
    let classes = card_classes(card);
    html! {
        article class=(classes) id=(format!("settle/{}", card.id)) {
            h3 { (card.title) }
            p { "Source " (card.source) }
            @if let Some(context) = &card.context {
                p { (context) }
            }
            @if let Some(plan) = card.plan {
                p { "step " (plan.step_index) " of " (plan.step_count) }
            }
            @if let Some(guard) = &card.guard {
                p {
                    span class=(guard_class(guard)) {
                        "Guard " (if guard.allow { "allow" } else { "deny" })
                    }
                    " " (guard.rule_id) ": " (guard.reason)
                }
            }
            @if let Some(detail) = &card.detail {
                p { (detail) }
            }
            @if let Some(run_hash) = &card.run_hash {
                p { "Run " (run_hash) }
            }
            @if let Some(diff) = &card.diff_stat {
                p { "Diff: " (diff) }
            }
            @if let Some(custody) = &card.custody {
                p { "Custody " (custody) }
            }
            @if let Some(age) = card.age_secs {
                p { "Age " (age) " s" }
            }
            p { "Created at " (card.created_at) }
        }
    }
}

/// Card classes: `card`, the guard class, and `card-attention` when the card waits long.
fn card_classes(card: &DeckCard) -> String {
    let mut classes = vec!["card"];
    if let Some(guard) = &card.guard {
        classes.push(guard_class(guard));
    }
    if card.needs_attention {
        classes.push("card-attention");
    }
    classes.join(" ")
}

fn guard_class(guard: &Guard) -> &'static str {
    if guard.allow {
        "guard-allow"
    } else {
        "guard-deny"
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
    use uas_domain::SlotId;
    use uas_engine::{SessionId, SettleCard};

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

    fn fixture() -> PendingCards {
        crate::parse_pending(include_str!("../tests/fixtures/settle-pending.json")).unwrap()
    }

    fn queue_of(cards: Vec<DeckCard>) -> PendingCards {
        let total = cards.len();
        PendingCards { cards, total }
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
        let markup = settle_cards(&queue_of(vec![DeckCard::from(card)])).into_string();
        assert!(markup.contains("&lt;script&gt;"), "{markup}");
        assert!(!markup.contains("<script>"), "{markup}");
    }

    #[test]
    fn cortex_text_is_escaped() {
        let markup = settle_cards(&fixture()).into_string();
        assert!(
            markup.contains("&lt;script&gt;x&lt;/script&gt;"),
            "{markup}"
        );
        assert!(!markup.contains("<script>"), "{markup}");
    }

    #[test]
    fn cortex_card_renders_contract_fields() {
        let markup = settle_cards(&fixture()).into_string();
        let expected = [
            r#"id="settle/e1""#,
            "guard-deny",
            "external-send-review",
            "External send needs operator review",
            "card-attention",
            "step 2 of 5",
            "b3f1c2a9",
            "2 files changed, 14 insertions(+), 3 deletions(-)",
            "fleet:builder-1",
            "Source email",
            "Vendor asked for a revised invoice",
            "Age 93600 s",
        ];
        for needle in expected {
            assert!(markup.contains(needle), "missing {needle}: {markup}");
        }
    }

    #[test]
    fn every_text_field_is_escaped() {
        let body = r#"{"by_source": {"<x>": [{"job_id": "a\"b<x>", "source": "<x>", "changeset": {"run_hash": "<x>", "description": "<x>"}, "guard_decision": {"allow": false, "rule_id": "<x>", "reason": "<x>"}, "settle_url": "http://cortex.invalid/v1/settle/x", "created_at": "<x>", "age_secs": 1, "needs_attention": false, "detail": "<x>", "custody": "<x>"}]}, "plans": [], "total": 1}"#;
        let queue = crate::parse_pending(body).unwrap();
        let markup = settle_cards(&queue).into_string();
        let expected = [
            r#"id="settle/a&quot;b&lt;x&gt;""#,
            "<h3>&lt;x&gt;</h3>",
            "Source &lt;x&gt;",
            "Run &lt;x&gt;",
            "&lt;x&gt;: &lt;x&gt;",
            "<p>&lt;x&gt;</p>",
            "Custody &lt;x&gt;",
            "Created at &lt;x&gt;",
        ];
        for needle in expected {
            assert!(markup.contains(needle), "missing {needle}: {markup}");
        }
        assert!(!markup.contains("<x>"), "{markup}");
    }

    #[test]
    fn unknown_source_renders_as_sent() {
        let body = r#"{"by_source": {"fax": [{"job_id": "f1", "source": "fax", "changeset": {"run_hash": "r1", "description": "Send the form"}, "guard_decision": {"allow": true, "rule_id": "r", "reason": "ok"}, "settle_url": "http://cortex.invalid/v1/settle/f1", "created_at": "2026-10-10T00:00:00Z", "age_secs": 5, "needs_attention": false}]}, "plans": [], "total": 1}"#;
        let queue = crate::parse_pending(body).unwrap();
        let markup = settle_cards(&queue).into_string();
        assert!(markup.contains("Source fax"), "{markup}");
    }

    #[test]
    fn guard_and_attention_are_per_card() {
        let markup = settle_cards(&fixture()).into_string();
        let deny = r#"<article class="card guard-deny card-attention" id="settle/e1">"#;
        let allow = r#"<article class="card guard-allow" id="settle/g1">"#;
        assert!(markup.contains(deny), "{markup}");
        assert!(markup.contains(allow), "{markup}");
    }

    #[test]
    fn empty_cards_with_total_shows_notice_not_empty_text() {
        let queue = PendingCards {
            cards: Vec::new(),
            total: 2,
        };
        let markup = settle_cards(&queue).into_string();
        assert!(
            markup.contains("Cortex reports 2 pending. The deck shows 0."),
            "{markup}"
        );
        assert!(!markup.contains("Nothing waits"), "{markup}");
        let empty = settle_cards(&queue_of(Vec::new())).into_string();
        assert!(empty.contains("Nothing waits for a decision."), "{empty}");
    }

    #[test]
    fn count_notice_from_source_total() {
        let body = include_str!("../tests/fixtures/settle-pending.json")
            .replace("\"total\": 4", "\"total\": 5");
        let queue = crate::parse_pending(&body).unwrap();
        assert_eq!(queue.total, 5);
        let markup = settle_cards(&queue).into_string();
        assert!(
            markup.contains("Cortex reports 5 pending. The deck shows 4."),
            "{markup}"
        );
    }

    #[test]
    fn settle_url_never_rendered() {
        let markup = settle_cards(&fixture()).into_string();
        assert!(!markup.contains("cortex.invalid"), "{markup}");
        assert!(!markup.contains("/v1/settle/"), "{markup}");
    }

    #[test]
    fn count_notice_when_total_differs() {
        let card = DeckCard::from(SettleCard::new("card-n", "sess", "step", "Title", "why", 1));
        let short = PendingCards {
            cards: vec![card.clone()],
            total: 3,
        };
        let markup = settle_cards(&short).into_string();
        assert!(
            markup.contains("Cortex reports 3 pending. The deck shows 1."),
            "{markup}"
        );
        let even = queue_of(vec![card]);
        let markup = settle_cards(&even).into_string();
        assert!(!markup.contains("Cortex reports"), "{markup}");
    }

    #[test]
    fn page_has_no_external_url() {
        let markup = page(
            &[row(Some(EvidenceTier::Verified))],
            settle_cards(&fixture()),
        )
        .into_string();
        assert!(!markup.contains("http:"), "{markup}");
        assert!(!markup.contains("https:"), "{markup}");
        assert!(!markup.contains("//"), "{markup}");
    }
}
