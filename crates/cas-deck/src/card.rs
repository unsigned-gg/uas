//! The deck card. Every settle source maps to this one shape (spec 0018 D7).

use cas_engine::SettleCard;

/// One card on the settle queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckCard {
    /// Card identifier. For a Cortex card this is the `job_id`.
    pub id: String,
    /// Card title. For a Cortex card this is the changeset description.
    pub title: String,
    /// Source as the settle source reported it, shown unchanged.
    pub source: String,
    /// One context line, when the source gives one.
    pub context: Option<String>,
    /// Longer rationale or detail text.
    pub detail: Option<String>,
    /// Hash of the changeset the card points at.
    pub run_hash: Option<String>,
    /// One-line summary of the changeset diff.
    pub diff_stat: Option<String>,
    /// Guard decision recorded for the card.
    pub guard: Option<Guard>,
    /// True when the card has waited past the attention threshold.
    pub needs_attention: bool,
    /// Age of the card in seconds.
    pub age_secs: Option<i64>,
    /// Creation time, kept as the text the source sent.
    pub created_at: String,
    /// Position of the card inside a plan.
    pub plan: Option<PlanStep>,
    /// Custody label, such as `fleet:builder-1`.
    pub custody: Option<String>,
    /// Settle link. The deck keeps it and never renders it.
    pub settle_url: Option<String>,
}

/// The guard decision recorded for a card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guard {
    /// True when the guard allowed the action.
    pub allow: bool,
    /// Rule that produced the decision.
    pub rule_id: String,
    /// Reason the rule gave.
    pub reason: String,
}

/// A card's position in a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanStep {
    /// Index of this step.
    pub step_index: u32,
    /// Number of steps in the plan.
    pub step_count: u32,
}

/// The pending queue: its cards, and the total the source reported.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingCards {
    /// Cards that the deck shows.
    pub cards: Vec<DeckCard>,
    /// Total pending jobs the source reported. It can differ from the card count.
    pub total: usize,
}

impl From<SettleCard> for DeckCard {
    fn from(card: SettleCard) -> Self {
        Self {
            id: card.id,
            title: card.title,
            source: "local".to_string(),
            context: Some(format!(
                "Session {}, step {}",
                card.session_id, card.step_id
            )),
            detail: Some(card.rationale),
            run_hash: None,
            diff_stat: card.diff_summary,
            guard: None,
            needs_attention: false,
            age_secs: None,
            created_at: card.created_at.to_string(),
            plan: None,
            custody: None,
            settle_url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cas_engine::SettleCard;

    #[test]
    fn settle_card_maps_to_local() {
        let mut card = SettleCard::new(
            "card-7",
            "sess-9",
            "step-3",
            "Push fix",
            "Checks passed",
            200,
        );
        card.diff_summary = Some("1 file changed".to_string());
        let mapped = DeckCard::from(card);
        assert_eq!(mapped.id, "card-7");
        assert_eq!(mapped.title, "Push fix");
        assert_eq!(mapped.source, "local");
        assert_eq!(
            mapped.context.as_deref(),
            Some("Session sess-9, step step-3")
        );
        assert_eq!(mapped.detail.as_deref(), Some("Checks passed"));
        assert_eq!(mapped.diff_stat.as_deref(), Some("1 file changed"));
        assert_eq!(mapped.run_hash, None);
        assert_eq!(mapped.guard, None);
        assert!(!mapped.needs_attention);
        assert_eq!(mapped.age_secs, None);
        assert_eq!(mapped.created_at, "200");
        assert_eq!(mapped.plan, None);
        assert_eq!(mapped.custody, None);
        assert_eq!(mapped.settle_url, None);
    }
}
