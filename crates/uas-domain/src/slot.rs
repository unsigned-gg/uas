use std::fmt;

use serde::{Deserialize, Serialize};

use crate::DomainError;

/// Longest allowed worktree reference, in bytes.
const MAX_WORKTREE_LEN: usize = 4096;

/// A validated slot id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SlotId(String);

impl SlotId {
    /// Validate `text` and wrap it.
    pub fn new(text: impl Into<String>) -> Result<Self, DomainError> {
        let text = text.into();
        crate::ident::check("slot id", &text, 64, "")?;
        Ok(Self(text))
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SlotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for SlotId {
    type Error = DomainError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SlotId> for String {
    fn from(id: SlotId) -> Self {
        id.0
    }
}

/// A validated reference to a git worktree.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WorktreeRef(String);

impl WorktreeRef {
    /// Validate `text` and wrap it.
    pub fn new(text: impl Into<String>) -> Result<Self, DomainError> {
        let text = text.into();
        let reason = if text.is_empty() {
            Some("must not be empty")
        } else if text.len() > MAX_WORKTREE_LEN {
            Some("is too long")
        } else if text.chars().any(char::is_control) {
            Some("must not contain control characters")
        } else if text.split('/').any(|part| part == "..") {
            Some("must not contain a '..' segment")
        } else {
            None
        };
        match reason {
            Some(reason) => Err(DomainError::Invalid {
                kind: "worktree ref",
                value: text,
                reason,
            }),
            None => Ok(Self(text)),
        }
    }

    /// The reference as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WorktreeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for WorktreeRef {
    type Error = DomainError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<WorktreeRef> for String {
    fn from(r: WorktreeRef) -> Self {
        r.0
    }
}

/// Lifecycle state of a [`Slot`].
///
/// Legal moves: `Cold -> Warm -> Leased -> Suspended -> Leased -> Warm`.
/// Any state except `Draining` may move to `Draining`. `Draining` is final.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotState {
    /// No worktree, no harness.
    Cold,
    /// Worktree ready, no lease.
    Warm,
    /// Held by a lease.
    Leased,
    /// Lease holder paused; the slot stays reserved.
    Suspended,
    /// Being shut down. Final.
    Draining,
}

/// One harness slot in the pool.
///
/// Deserializing accepts only a state and worktree pair that the transitions
/// can reach: `Cold` has no worktree, `Warm`, `Leased` and `Suspended` have
/// one, and `Draining` may have either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SlotRepr")]
pub struct Slot {
    id: SlotId,
    state: SlotState,
    worktree: Option<WorktreeRef>,
}

/// The wire shape of a [`Slot`], checked before it becomes one.
#[derive(Deserialize)]
struct SlotRepr {
    id: SlotId,
    state: SlotState,
    worktree: Option<WorktreeRef>,
}

impl TryFrom<SlotRepr> for Slot {
    type Error = DomainError;

    fn try_from(r: SlotRepr) -> Result<Self, Self::Error> {
        use SlotState::{Cold, Draining, Leased, Suspended, Warm};
        let reachable = match r.state {
            Cold => r.worktree.is_none(),
            Warm | Leased | Suspended => r.worktree.is_some(),
            Draining => true,
        };
        if !reachable {
            let worktree = if r.worktree.is_some() { "set" } else { "none" };
            return Err(DomainError::Invalid {
                kind: "slot",
                value: format!("state={:?} worktree={worktree}", r.state),
                reason: "no transition reaches this state and worktree pair",
            });
        }
        Ok(Self {
            id: r.id,
            state: r.state,
            worktree: r.worktree,
        })
    }
}

impl Slot {
    /// Create a [`SlotState::Cold`] slot.
    pub fn new(id: SlotId) -> Self {
        Self {
            id,
            state: SlotState::Cold,
            worktree: None,
        }
    }

    /// The slot id.
    pub fn id(&self) -> &SlotId {
        &self.id
    }

    /// The current state.
    pub fn state(&self) -> SlotState {
        self.state
    }

    /// The worktree, set once the slot has been warmed.
    pub fn worktree(&self) -> Option<&WorktreeRef> {
        self.worktree.as_ref()
    }

    /// Move `Cold` to `Warm`. The slot needs a worktree.
    pub fn warm(self, worktree: WorktreeRef) -> Result<Self, DomainError> {
        let mut next = self.transition("warm", &[SlotState::Cold], SlotState::Warm)?;
        next.worktree = Some(worktree);
        Ok(next)
    }

    /// Move `Warm` to `Leased`.
    pub fn lease(self) -> Result<Self, DomainError> {
        self.transition("lease", &[SlotState::Warm], SlotState::Leased)
    }

    /// Move `Leased` to `Suspended`.
    pub fn suspend(self) -> Result<Self, DomainError> {
        self.transition("suspend", &[SlotState::Leased], SlotState::Suspended)
    }

    /// Move `Suspended` to `Leased`.
    pub fn resume(self) -> Result<Self, DomainError> {
        self.transition("resume", &[SlotState::Suspended], SlotState::Leased)
    }

    /// Move `Leased` to `Warm`.
    pub fn release(self) -> Result<Self, DomainError> {
        self.transition("release", &[SlotState::Leased], SlotState::Warm)
    }

    /// Move any state except `Draining` to `Draining`.
    ///
    /// The worktree reference stays, so the pool can tear the worktree down
    /// (spec 0007, R4). A slot drained from `Cold` has none.
    pub fn drain(self) -> Result<Self, DomainError> {
        use SlotState::{Cold, Leased, Suspended, Warm};
        self.transition(
            "drain",
            &[Cold, Warm, Leased, Suspended],
            SlotState::Draining,
        )
    }

    fn transition(
        mut self,
        op: &'static str,
        allowed_from: &[SlotState],
        to: SlotState,
    ) -> Result<Self, DomainError> {
        if !allowed_from.contains(&self.state) {
            return Err(DomainError::IllegalTransition {
                from: self.state,
                op,
            });
        }
        self.state = to;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wt() -> WorktreeRef {
        WorktreeRef::new("wt/slot-1").unwrap()
    }

    fn slot_in(state: SlotState) -> Slot {
        let s = Slot::new(SlotId::new("slot-1").unwrap());
        match state {
            SlotState::Cold => s,
            SlotState::Warm => s.warm(wt()).unwrap(),
            SlotState::Leased => s.warm(wt()).unwrap().lease().unwrap(),
            SlotState::Suspended => s.warm(wt()).unwrap().lease().unwrap().suspend().unwrap(),
            SlotState::Draining => s.drain().unwrap(),
        }
    }

    type Op = (&'static str, fn(Slot) -> Result<Slot, DomainError>);

    fn ops() -> Vec<Op> {
        vec![
            ("warm", |s| s.warm(wt())),
            ("lease", Slot::lease),
            ("suspend", Slot::suspend),
            ("resume", Slot::resume),
            ("release", Slot::release),
            ("drain", Slot::drain),
        ]
    }

    #[test]
    fn every_transition_is_legal_or_errors_exhaustively() {
        use SlotState::{Cold, Draining, Leased, Suspended, Warm};
        let legal = [
            (Cold, "warm", Warm),
            (Warm, "lease", Leased),
            (Leased, "suspend", Suspended),
            (Suspended, "resume", Leased),
            (Leased, "release", Warm),
            (Cold, "drain", Draining),
            (Warm, "drain", Draining),
            (Leased, "drain", Draining),
            (Suspended, "drain", Draining),
        ];
        let mut seen = 0;
        for from in [Cold, Warm, Leased, Suspended, Draining] {
            for (name, op) in ops() {
                let result = op(slot_in(from));
                match legal.iter().find(|(f, n, _)| *f == from && *n == name) {
                    Some((_, _, to)) => {
                        assert_eq!(result.unwrap().state(), *to, "{from:?} {name}");
                    }
                    None => {
                        assert_eq!(
                            result,
                            Err(DomainError::IllegalTransition { from, op: name }),
                            "{from:?} {name} should fail"
                        );
                    }
                }
                seen += 1;
            }
        }
        assert_eq!(seen, 30);
    }

    #[test]
    fn warm_records_the_worktree() {
        assert_eq!(slot_in(SlotState::Cold).worktree(), None);
        let warm = slot_in(SlotState::Warm);
        assert_eq!(warm.worktree(), Some(&wt()));
        assert_eq!(slot_in(SlotState::Suspended).worktree(), Some(&wt()));
    }

    #[test]
    fn full_cycle_ends_warm() {
        let s = slot_in(SlotState::Cold)
            .warm(wt())
            .unwrap()
            .lease()
            .unwrap()
            .suspend()
            .unwrap()
            .resume()
            .unwrap()
            .release()
            .unwrap();
        assert_eq!(s.state(), SlotState::Warm);
    }

    #[test]
    fn slot_ids_are_validated() {
        for ok in ["a", "slot-1", "a1-b2"] {
            assert_eq!(SlotId::new(ok).unwrap().as_str(), ok);
        }
        assert!(SlotId::new("a".repeat(64)).is_ok());
        for bad in ["", "1abc", "-abc", "Abc", "a_b", "a b", "a.b", "\u{e9}"] {
            assert!(
                matches!(SlotId::new(bad), Err(DomainError::Invalid { .. })),
                "{bad:?}"
            );
        }
        assert!(SlotId::new("a".repeat(65)).is_err());
    }

    #[test]
    fn worktree_refs_are_validated() {
        for ok in ["wt/slot-1", "/abs/path/wt", "a"] {
            assert!(WorktreeRef::new(ok).is_ok(), "{ok:?}");
        }
        for bad in ["", "a/../b", "..", "a\nb", "a\0b", "\ta"] {
            assert!(
                matches!(WorktreeRef::new(bad), Err(DomainError::Invalid { .. })),
                "{bad:?}"
            );
        }
        assert!(WorktreeRef::new("a".repeat(4097)).is_err());
    }

    #[test]
    fn serde_validates_newtypes() {
        assert!(serde_json::from_str::<SlotId>("\"good-id\"").is_ok());
        assert!(serde_json::from_str::<SlotId>("\"Bad Id\"").is_err());
        assert!(serde_json::from_str::<WorktreeRef>("\"a/../b\"").is_err());
    }

    #[test]
    fn serde_rejects_states_no_transition_can_reach() {
        let wt_json = serde_json::to_value(slot_in(SlotState::Warm)).unwrap()["worktree"].clone();
        let mut cold_with_worktree = serde_json::to_value(slot_in(SlotState::Cold)).unwrap();
        cold_with_worktree["worktree"] = wt_json;
        let mut bad = vec![cold_with_worktree];
        for state in [SlotState::Warm, SlotState::Leased, SlotState::Suspended] {
            let mut v = serde_json::to_value(slot_in(state)).unwrap();
            v["worktree"] = serde_json::Value::Null;
            bad.push(v);
        }
        for v in bad {
            assert!(
                serde_json::from_value::<Slot>(v.clone()).is_err(),
                "accepted {v}"
            );
        }
    }

    #[test]
    fn serde_round_trips_every_reachable_slot() {
        use SlotState::{Cold, Draining, Leased, Suspended, Warm};
        let mut reachable: Vec<Slot> = [Cold, Warm, Leased, Suspended, Draining]
            .into_iter()
            .map(slot_in)
            .collect();
        // Draining is reachable with a worktree too.
        reachable.push(slot_in(Warm).drain().unwrap());
        for s in reachable {
            let json = serde_json::to_value(&s).unwrap();
            assert_eq!(serde_json::from_value::<Slot>(json).unwrap(), s);
        }
    }

    #[test]
    fn drain_keeps_the_worktree() {
        use SlotState::{Cold, Leased, Suspended, Warm};
        for state in [Warm, Leased, Suspended] {
            assert_eq!(slot_in(state).drain().unwrap().worktree(), Some(&wt()));
        }
        assert_eq!(slot_in(Cold).drain().unwrap().worktree(), None);
    }
}
