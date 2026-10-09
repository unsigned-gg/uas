//! Apply a finished run's outcome to a task of the project DAG (spec 0027, stage 1). Pure: no I/O.
//!
//! The project DAG is hand-edited and read by other tools, so every struct here is lenient
//! and keeps unknown fields in `extra`. The caller writes back only the fields this module
//! names in [`AppliedChange`].

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// A task of the project DAG as `apply_run` needs it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DagTask {
    pub id: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub attempts: Vec<DagAttempt>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An attempt of a DAG task (dagr v3 contract shape).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DagAttempt {
    pub id: String,
    #[serde(default)]
    pub n: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// What a finished run reports, reduced to what the mapping reads.
#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    pub run_id: String,
    pub started_at: String,
    pub ended_at: String,
    pub pane: String,
    pub agent: Option<String>,
    pub tasks: Vec<RunTaskSummary>,
}

/// One step of the run, with the outcome of its last attempt.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunTaskSummary {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub actor: Option<String>,
    pub evidence: Option<String>,
    pub receipt: Option<String>,
    pub reason: Option<String>,
}

/// The change to write back: the new task state and the one attempt to append.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedChange {
    pub task_state: String,
    pub attempt: DagAttempt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    TaskNotFound(String),
    /// The task already holds an attempt for this run. Carries that attempt's id.
    AlreadyApplied(String),
    RunNotTerminal,
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TaskNotFound(t) => write!(f, "no task {t} in the DAG"),
            Self::AlreadyApplied(a) => write!(f, "run already applied as attempt {a}"),
            Self::RunNotTerminal => write!(f, "run has steps that are not finished"),
        }
    }
}

impl std::error::Error for ApplyError {}

fn terminal(state: &str) -> bool {
    matches!(state, "done" | "failed" | "rejected" | "canceled")
}

/// The marker that ties an attempt to its run. dagr's `cause.ref` must name an attempt or
/// task of the same file, so the run id lives in `cause.reason` instead.
pub fn cause_reason(run_id: &str) -> String {
    format!("saga run {run_id}")
}

/// Build the change for `task_id`. Errors leave the DAG untouched.
pub fn apply_run(
    dag_tasks: &[DagTask],
    task_id: &str,
    summary: &RunSummary,
    force: bool,
) -> Result<AppliedChange, ApplyError> {
    let task = dag_tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| ApplyError::TaskNotFound(task_id.to_string()))?;
    if summary.tasks.is_empty() || !summary.tasks.iter().all(|t| terminal(&t.state)) {
        return Err(ApplyError::RunNotTerminal);
    }
    let reason = cause_reason(&summary.run_id);
    if !force {
        if let Some(a) = task.attempts.iter().find(|a| {
            a.cause
                .as_ref()
                .and_then(|c| c.get("reason"))
                .and_then(Value::as_str)
                == Some(reason.as_str())
        }) {
            return Err(ApplyError::AlreadyApplied(a.id.clone()));
        }
    }

    let total = summary.tasks.len();
    let done = summary.tasks.iter().filter(|t| t.state == "done").count();
    let failed = summary
        .tasks
        .iter()
        .filter(|t| matches!(t.state.as_str(), "failed" | "rejected"))
        .count();
    let canceled = summary
        .tasks
        .iter()
        .filter(|t| t.state == "canceled")
        .count();
    let run_result = if done == total {
        "done"
    } else if failed > 0 {
        "failed"
    } else {
        "canceled"
    };
    // Ruling: a failed or canceled run sets the task to `failed`; a human requeues.
    // dagr has no `canceled` attempt state, so the attempt is `failed` and the detail says why.
    let state = if run_result == "done" {
        "done"
    } else {
        "failed"
    };

    let land_receipt = summary
        .tasks
        .iter()
        .find(|t| t.kind == "land" && t.state == "done")
        .and_then(|t| t.receipt.as_deref())
        .filter(|r| !r.trim().is_empty());
    let evidence = if land_receipt.is_some() {
        "verified"
    } else if summary
        .tasks
        .iter()
        .any(|t| t.evidence.as_deref() == Some("heuristic"))
    {
        "heuristic"
    } else {
        "asserted"
    };
    let receipt = match land_receipt {
        Some(sha) => format!("run {}; merge {sha}", summary.run_id),
        None => format!("run {}", summary.run_id),
    };

    let mut detail = format!("{done}/{total} steps done");
    if failed > 0 {
        detail.push_str(&format!(", {failed} failed"));
    }
    if canceled > 0 {
        detail.push_str(&format!(", {canceled} canceled"));
    }
    if run_result == "canceled" {
        detail = format!("run canceled: {detail}");
    }
    if let Some(t) = summary
        .tasks
        .iter()
        .find(|t| matches!(t.state.as_str(), "failed" | "rejected"))
    {
        detail.push_str(&format!("; first failed: {}", t.id));
        if let Some(r) = t.reason.as_deref().filter(|r| !r.trim().is_empty()) {
            let one_line = r.lines().next().unwrap_or("").trim();
            detail.push_str(&format!(" ({one_line})"));
        }
    }

    let n = task
        .attempts
        .iter()
        .map(|a| a.n)
        .max()
        .unwrap_or(0)
        .max(task.attempts.len() as u32)
        + 1;
    let cause = if task.attempts.is_empty() {
        json!({ "type": "initial", "by": "saga", "reason": reason })
    } else {
        json!({ "type": "followup", "by": "saga", "reason": reason })
    };
    let mut locator = Map::new();
    locator.insert("pane".into(), summary.pane.clone().into());
    if let Some(agent) = &summary.agent {
        locator.insert("agent".into(), agent.clone().into());
    }
    let actor = summary.agent.clone().unwrap_or_else(|| "saga".to_string());

    Ok(AppliedChange {
        task_state: state.to_string(),
        attempt: DagAttempt {
            id: format!("{task_id}·a{n}"),
            n,
            cause: Some(cause),
            actor: Some(actor),
            state: Some(state.to_string()),
            started_at: Some(summary.started_at.clone()),
            ended_at: Some(summary.ended_at.clone()),
            outcome: Some(json!({
                "result": state,
                "evidence": evidence,
                "receipt": receipt,
                "detail": detail,
            })),
            locator: Some(Value::Object(locator)),
            extra: Map::new(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(id: &str, kind: &str, state: &str) -> RunTaskSummary {
        RunTaskSummary {
            id: id.into(),
            kind: kind.into(),
            state: state.into(),
            ..Default::default()
        }
    }

    fn summary(tasks: Vec<RunTaskSummary>) -> RunSummary {
        RunSummary {
            run_id: "20261008-101500-self-improve".into(),
            started_at: "2026-10-08T10:15:00Z".into(),
            ended_at: "2026-10-08T10:40:00Z".into(),
            pane: "w1P:p2".into(),
            agent: Some("claude".into()),
            tasks,
        }
    }

    fn dag() -> Vec<DagTask> {
        serde_json::from_value(json!([
            { "id": "other", "state": "queued", "attempts": [] },
            { "id": "t1", "state": "working", "title": "keep me", "attempts": [] }
        ]))
        .unwrap()
    }

    fn land_done(receipt: Option<&str>) -> RunTaskSummary {
        RunTaskSummary {
            receipt: receipt.map(Into::into),
            evidence: Some("verified".into()),
            ..rt("land", "land", "done")
        }
    }

    fn outcome(c: &AppliedChange) -> &Value {
        c.attempt.outcome.as_ref().unwrap()
    }

    #[test]
    fn done_run_with_land_receipt_is_verified() {
        let s = summary(vec![rt("pick", "plan", "done"), land_done(Some("abc1234"))]);
        let c = apply_run(&dag(), "t1", &s, false).unwrap();
        assert_eq!(c.task_state, "done");
        assert_eq!(c.attempt.id, "t1·a1");
        assert_eq!(outcome(&c)["result"], "done");
        assert_eq!(outcome(&c)["evidence"], "verified");
        assert_eq!(
            outcome(&c)["receipt"],
            "run 20261008-101500-self-improve; merge abc1234"
        );
        assert_eq!(c.attempt.cause.as_ref().unwrap()["type"], "initial");
        assert_eq!(c.attempt.locator.as_ref().unwrap()["pane"], "w1P:p2");
    }

    #[test]
    fn done_run_without_land_receipt_is_not_verified() {
        // A land step that says verified but carries no receipt must not verify the task.
        let s = summary(vec![rt("pick", "plan", "done"), land_done(None)]);
        let c = apply_run(&dag(), "t1", &s, false).unwrap();
        assert_eq!(c.task_state, "done");
        assert_ne!(outcome(&c)["evidence"], "verified");
        let blank = summary(vec![land_done(Some("  "))]);
        let c = apply_run(&dag(), "t1", &blank, false).unwrap();
        assert_ne!(outcome(&c)["evidence"], "verified");
        // No land step at all.
        let c = apply_run(&dag(), "t1", &summary(vec![rt("a", "impl", "done")]), false).unwrap();
        assert_eq!(outcome(&c)["evidence"], "asserted");
        assert_eq!(outcome(&c)["receipt"], "run 20261008-101500-self-improve");
    }

    #[test]
    fn heuristic_evidence_when_any_step_is_heuristic() {
        let mut h = rt("impl", "impl", "done");
        h.evidence = Some("heuristic".into());
        let c = apply_run(
            &dag(),
            "t1",
            &summary(vec![h, rt("n", "note", "done")]),
            false,
        )
        .unwrap();
        assert_eq!(outcome(&c)["evidence"], "heuristic");
    }

    #[test]
    fn failed_run_fails_task_and_names_step() {
        let mut bad = rt("test-check", "test", "failed");
        bad.reason = Some("exit 101\nmore".into());
        let s = summary(vec![
            rt("pick", "plan", "done"),
            bad,
            rt("land", "land", "canceled"),
        ]);
        let c = apply_run(&dag(), "t1", &s, false).unwrap();
        assert_eq!(c.task_state, "failed");
        assert_eq!(c.attempt.state.as_deref(), Some("failed"));
        assert_eq!(outcome(&c)["result"], "failed");
        let d = outcome(&c)["detail"].as_str().unwrap();
        assert!(d.contains("1/3 steps done"), "{d}");
        assert!(d.contains("first failed: test-check (exit 101)"), "{d}");
    }

    #[test]
    fn canceled_run_fails_task_with_failed_attempt() {
        let s = summary(vec![
            rt("pick", "plan", "done"),
            rt("land", "land", "canceled"),
        ]);
        let c = apply_run(&dag(), "t1", &s, false).unwrap();
        assert_eq!(c.task_state, "failed");
        // dagr has no canceled attempt state; result must equal state or dagr rejects it.
        assert_eq!(c.attempt.state.as_deref(), Some("failed"));
        assert_eq!(outcome(&c)["result"], "failed");
        assert!(outcome(&c)["detail"]
            .as_str()
            .unwrap()
            .starts_with("run canceled:"));
    }

    #[test]
    fn non_terminal_run_is_refused() {
        let s = summary(vec![rt("a", "impl", "done"), rt("b", "impl", "working")]);
        assert_eq!(
            apply_run(&dag(), "t1", &s, false),
            Err(ApplyError::RunNotTerminal)
        );
        assert_eq!(
            apply_run(&dag(), "t1", &summary(vec![]), false),
            Err(ApplyError::RunNotTerminal)
        );
    }

    #[test]
    fn unknown_task_is_refused() {
        let s = summary(vec![rt("a", "impl", "done")]);
        assert_eq!(
            apply_run(&dag(), "ghost", &s, false),
            Err(ApplyError::TaskNotFound("ghost".into()))
        );
    }

    #[test]
    fn second_apply_is_refused_and_force_appends() {
        let s = summary(vec![rt("a", "impl", "done")]);
        let mut tasks = dag();
        let c = apply_run(&tasks, "t1", &s, false).unwrap();
        tasks[1].attempts.push(c.attempt);
        assert_eq!(
            apply_run(&tasks, "t1", &s, false),
            Err(ApplyError::AlreadyApplied("t1·a1".into()))
        );
        let again = apply_run(&tasks, "t1", &s, true).unwrap();
        assert_eq!(again.attempt.id, "t1·a2");
        assert_eq!(again.attempt.n, 2);
        assert_eq!(again.attempt.cause.as_ref().unwrap()["type"], "followup");
    }

    #[test]
    fn attempt_number_follows_existing_attempts() {
        let tasks: Vec<DagTask> = serde_json::from_value(json!([
            { "id": "t1", "state": "failed", "attempts": [
                { "id": "t1·a1", "n": 1 }, { "id": "t1·a4", "n": 4 }
            ] }
        ]))
        .unwrap();
        let s = summary(vec![rt("a", "impl", "done")]);
        assert_eq!(
            apply_run(&tasks, "t1", &s, false).unwrap().attempt.id,
            "t1·a5"
        );
    }

    #[test]
    fn unknown_fields_survive_a_round_trip() {
        let raw = json!({
            "id": "t1", "state": "queued", "title": "x", "deps": ["a"], "weird": {"k": 1},
            "attempts": [{ "id": "t1·a1", "n": 1, "progress": {"done": 1}, "chain_key": "z" }]
        });
        let t: DagTask = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(&t).unwrap(), raw);
    }
}
