// SPDX-License-Identifier: Apache-2.0 OR MIT
//! The Cortex settle source. It parses the body of `GET /v1/settle/pending`
//! (spec 0018 A1, D9, D10). The fetch sits behind `PendingFetch`, so tests read
//! fixture files and open no socket to Cortex.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::card::{DeckCard, Guard, PendingCards, PlanStep};
use crate::source::SettleSource;
use crate::DeckError;

/// Supplies the body of `GET /v1/settle/pending`.
pub trait PendingFetch: Send + Sync {
    /// Return the response body as text, or the error that stopped the fetch.
    fn fetch_pending(&self) -> Result<String, DeckError>;
}

/// A settle source that reads the Cortex pending route through a `PendingFetch`.
pub struct CortexSettleSource<F> {
    fetch: F,
}

impl<F> CortexSettleSource<F> {
    /// Build the source from a fetch.
    pub fn new(fetch: F) -> Self {
        Self { fetch }
    }
}

impl<F: PendingFetch> SettleSource for CortexSettleSource<F> {
    fn pending_cards(&self) -> Result<PendingCards, DeckError> {
        let body = self.fetch.fetch_pending()?;
        parse_pending(&body)
    }
}

/// Parse the body of `GET /v1/settle/pending`. Cards come in key order of
/// `by_source`, then one card for each plan's `active_step`. A body that does not
/// match the contract returns `SettleContract`.
pub fn parse_pending(body: &str) -> Result<PendingCards, DeckError> {
    let response: PendingResponse =
        serde_json::from_str(body).map_err(|err| DeckError::SettleContract(err.to_string()))?;
    let mut cards = Vec::new();
    for jobs in response.by_source.into_values() {
        cards.extend(jobs.into_iter().map(card_from_job));
    }
    for plan in response.plans {
        cards.push(card_from_job(plan.active_step));
    }
    Ok(PendingCards {
        cards,
        total: response.total,
    })
}

fn card_from_job(job: JobView) -> DeckCard {
    DeckCard {
        id: job.job_id,
        title: job.changeset.description,
        source: job.source,
        context: None,
        detail: job.detail,
        run_hash: Some(job.changeset.run_hash),
        diff_stat: job.changeset.diff_stat,
        guard: Some(Guard {
            allow: job.guard_decision.allow,
            rule_id: job.guard_decision.rule_id,
            reason: job.guard_decision.reason,
        }),
        needs_attention: job.needs_attention,
        age_secs: Some(job.age_secs),
        created_at: job.created_at,
        plan: job.plan.map(|plan| PlanStep {
            step_index: plan.step_index,
            step_count: plan.step_count,
        }),
        custody: job.custody,
        settle_url: Some(job.settle_url),
    }
}

#[derive(Deserialize)]
struct PendingResponse {
    by_source: BTreeMap<String, Vec<JobView>>,
    plans: Vec<PlanView>,
    total: usize,
}

#[derive(Deserialize)]
struct PlanView {
    active_step: JobView,
}

/// The fields of a Cortex `SettleJobView` that the card reads. Serde ignores the rest.
#[derive(Deserialize)]
struct JobView {
    job_id: String,
    source: String,
    changeset: ChangesetView,
    guard_decision: GuardView,
    settle_url: String,
    created_at: String,
    age_secs: i64,
    needs_attention: bool,
    #[serde(default)]
    plan: Option<PlanRef>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    custody: Option<String>,
}

#[derive(Deserialize)]
struct ChangesetView {
    run_hash: String,
    description: String,
    #[serde(default)]
    diff_stat: Option<String>,
}

#[derive(Deserialize)]
struct GuardView {
    allow: bool,
    rule_id: String,
    reason: String,
}

#[derive(Deserialize)]
struct PlanRef {
    step_index: u32,
    step_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/settle-pending.json");
    const EMPTY: &str = include_str!("../tests/fixtures/settle-pending-empty.json");

    fn find<'a>(parsed: &'a PendingCards, id: &str) -> &'a DeckCard {
        parsed
            .cards
            .iter()
            .find(|card| card.id == id)
            .unwrap_or_else(|| panic!("no card {id}"))
    }

    #[test]
    fn parse_orders_by_source_then_plans() {
        let parsed = parse_pending(FIXTURE).unwrap();
        let ids: Vec<&str> = parsed.cards.iter().map(|card| card.id.as_str()).collect();
        assert_eq!(ids, ["e1", "g1", "s1", "p1-s2"]);
        assert_eq!(parsed.total, 4);
        assert_eq!(find(&parsed, "g1").source, "git_hub");
        assert_eq!(
            find(&parsed, "p1-s2").plan,
            Some(PlanStep {
                step_index: 2,
                step_count: 5
            })
        );
    }

    #[test]
    fn parse_reads_absent_and_null() {
        let parsed = parse_pending(FIXTURE).unwrap();
        let email = find(&parsed, "e1");
        assert_eq!(email.title, "Reply to the vendor invoice");
        assert_eq!(email.run_hash.as_deref(), Some("b3f1c2a9"));
        assert_eq!(
            email.diff_stat.as_deref(),
            Some("2 files changed, 14 insertions(+), 3 deletions(-)")
        );
        assert_eq!(
            email.guard,
            Some(Guard {
                allow: false,
                rule_id: "external-send-review".to_string(),
                reason: "External send needs operator review".to_string(),
            })
        );
        assert!(email.needs_attention);
        assert_eq!(email.age_secs, Some(93600));
        assert_eq!(email.custody.as_deref(), Some("fleet:builder-1"));
        assert_eq!(
            email.settle_url.as_deref(),
            Some("http://cortex.invalid/v1/settle/e1")
        );
        let git = find(&parsed, "g1");
        assert_eq!(git.diff_stat, None);
        assert_eq!(git.detail, None);
        assert_eq!(git.custody, None);
        let session = find(&parsed, "s1");
        assert_eq!(session.title, "<script>x</script>");
        assert_eq!(session.diff_stat, None);
        assert_eq!(session.plan, None);
        assert_eq!(session.custody, None);
    }

    #[test]
    fn parse_empty_queue_has_no_cards() {
        let parsed = parse_pending(EMPTY).unwrap();
        assert!(parsed.cards.is_empty());
        assert_eq!(parsed.total, 0);
    }

    #[test]
    fn parse_malformed_is_settle_contract() {
        let bodies = [
            "{not json",
            r#"{"by_source": {}, "plans": []}"#,
            r#"{"by_source": {"email": [{"job_id": "e1"}]}, "plans": [], "total": 1}"#,
        ];
        for body in bodies {
            match parse_pending(body) {
                Err(DeckError::SettleContract(_)) => {}
                other => panic!("expected SettleContract, got {:?}", other.map(|_| ())),
            }
        }
    }

    struct Refused;

    impl PendingFetch for Refused {
        fn fetch_pending(&self) -> Result<String, DeckError> {
            Err(DeckError::CortexUnavailable("connection refused".into()))
        }
    }

    #[test]
    fn fetch_error_passes_through() {
        match CortexSettleSource::new(Refused).pending_cards() {
            Err(DeckError::CortexUnavailable(_)) => {}
            other => panic!("expected CortexUnavailable, got {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn parse_total_is_the_source_total() {
        let body = FIXTURE.replace("\"total\": 4", "\"total\": 5");
        let parsed = parse_pending(&body).unwrap();
        assert_eq!(parsed.total, 5);
        assert_eq!(parsed.cards.len(), 4);
    }

    #[test]
    fn parse_keeps_unknown_source_as_sent() {
        let body = r#"{"by_source": {"fax": [{"job_id": "f1", "source": "fax", "changeset": {"run_hash": "r1", "description": "Send the form"}, "guard_decision": {"allow": true, "rule_id": "r", "reason": "ok"}, "settle_url": "http://cortex.invalid/v1/settle/f1", "created_at": "2026-10-10T00:00:00Z", "age_secs": 5, "needs_attention": false}]}, "plans": [], "total": 1}"#;
        let parsed = parse_pending(body).unwrap();
        assert_eq!(parsed.cards[0].source, "fax");
    }
}
