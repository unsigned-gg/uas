//! Stage 1 task pick rule (spec 0027). Pure: no I/O.

/// A task as the pick rule sees it. Position in the slice is the rank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickCandidate {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub owner: Option<String>,
    pub state: String,
    pub deps: Vec<String>,
}

/// Choose the next task: `queued`, unowned or owned by `owner`, every dep `done`.
/// The lowest index wins. A dep that names no task in the slice is not done.
pub fn pick_next<'a>(tasks: &'a [PickCandidate], owner: &str) -> Option<&'a PickCandidate> {
    tasks.iter().find(|t| {
        t.state == "queued"
            && t.owner.as_deref().is_none_or(|o| o == owner)
            && t.deps.iter().all(|d| {
                tasks
                    .iter()
                    .any(|other| other.id == *d && other.state == "done")
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, state: &str, owner: Option<&str>, deps: &[&str]) -> PickCandidate {
        PickCandidate {
            id: id.into(),
            title: format!("title {id}"),
            kind: "impl".into(),
            owner: owner.map(Into::into),
            state: state.into(),
            deps: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn picks_first_eligible() {
        let tasks = [
            t("a", "done", None, &[]),
            t("b", "queued", None, &["a"]),
            t("c", "queued", Some("saga"), &["a"]),
        ];
        assert_eq!(pick_next(&tasks, "saga").unwrap().id, "b");
    }

    #[test]
    fn skips_task_with_dep_not_done() {
        let tasks = [
            t("a", "working", None, &[]),
            t("b", "queued", None, &["a"]),
            t("c", "queued", None, &[]),
        ];
        assert_eq!(pick_next(&tasks, "saga").unwrap().id, "c");
    }

    #[test]
    fn skips_task_owned_by_someone_else() {
        let tasks = [
            t("a", "queued", Some("w1P:p1"), &[]),
            t("b", "queued", Some("saga"), &[]),
        ];
        assert_eq!(pick_next(&tasks, "saga").unwrap().id, "b");
    }

    #[test]
    fn skips_non_queued_states() {
        let tasks = [t("a", "working", None, &[]), t("b", "done", None, &[])];
        assert!(pick_next(&tasks, "saga").is_none());
    }

    #[test]
    fn none_when_nothing_qualifies() {
        let tasks = [t("a", "queued", Some("other"), &[])];
        assert!(pick_next(&tasks, "saga").is_none());
        assert!(pick_next(&[], "saga").is_none());
    }

    #[test]
    fn unknown_dep_is_not_done() {
        let tasks = [t("a", "queued", None, &["ghost"])];
        assert!(pick_next(&tasks, "saga").is_none());
    }
}
