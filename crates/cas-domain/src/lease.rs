use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{DomainError, SlotId};

/// Longest allowed lease token, in bytes.
const MAX_TOKEN_LEN: usize = 256;

/// A validated lease id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LeaseId(String);

impl LeaseId {
    /// Validate `text` and wrap it.
    pub fn new(text: impl Into<String>) -> Result<Self, DomainError> {
        let text = text.into();
        crate::ident::check("lease id", &text, 64, "")?;
        Ok(Self(text))
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LeaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for LeaseId {
    type Error = DomainError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<LeaseId> for String {
    fn from(id: LeaseId) -> Self {
        id.0
    }
}

/// An opaque bearer token that proves a lease. `Debug` never prints it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LeaseToken(String);

impl LeaseToken {
    /// Validate `text` and wrap it.
    pub fn new(text: impl Into<String>) -> Result<Self, DomainError> {
        let text = text.into();
        let reason = if text.is_empty() {
            Some("must not be empty")
        } else if text.len() > MAX_TOKEN_LEN {
            Some("is too long")
        } else if text.chars().any(|c| c.is_whitespace() || c.is_control()) {
            Some("must not contain whitespace or control characters")
        } else {
            None
        };
        match reason {
            // The rejected text is a secret: do not put it in the error.
            Some(reason) => Err(DomainError::Invalid {
                kind: "lease token",
                value: String::new(),
                reason,
            }),
            None => Ok(Self(text)),
        }
    }

    /// The token text. Handle with care: do not log it.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether `other` is the same token. For two tokens of the same length,
    /// the work done does not depend on where they differ. A length mismatch
    /// returns false at once.
    pub fn matches(&self, other: &LeaseToken) -> bool {
        let (a, b) = (self.0.as_bytes(), other.0.as_bytes());
        if a.len() != b.len() {
            return false;
        }
        let diff = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
        std::hint::black_box(diff) == 0
    }
}

impl fmt::Debug for LeaseToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LeaseToken(<redacted>)")
    }
}

impl TryFrom<String> for LeaseToken {
    type Error = DomainError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<LeaseToken> for String {
    fn from(t: LeaseToken) -> Self {
        t.0
    }
}

/// A validated reference to a principal, such as an agent or a person.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PrincipalRef(String);

impl PrincipalRef {
    /// Validate `text` and wrap it.
    pub fn new(text: impl Into<String>) -> Result<Self, DomainError> {
        let text = text.into();
        crate::ident::check("principal ref", &text, 128, ":/._@")?;
        Ok(Self(text))
    }

    /// The reference as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrincipalRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for PrincipalRef {
    type Error = DomainError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PrincipalRef> for String {
    fn from(p: PrincipalRef) -> Self {
        p.0
    }
}

/// Spending limits for one lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "BudgetRepr")]
pub struct Budget {
    /// Most tokens the holder may use.
    pub max_tokens: u64,
    /// Most wall-clock seconds the holder may use.
    pub max_wall_secs: u64,
}

impl Budget {
    /// Build a budget. Both limits must be above zero.
    pub fn new(max_tokens: u64, max_wall_secs: u64) -> Result<Self, DomainError> {
        if max_tokens == 0 || max_wall_secs == 0 {
            return Err(DomainError::Invalid {
                kind: "budget",
                value: format!("tokens={max_tokens} wall_secs={max_wall_secs}"),
                reason: "both limits must be above zero",
            });
        }
        Ok(Self {
            max_tokens,
            max_wall_secs,
        })
    }
}

/// The right of one holder to use one slot until a deadline.
///
/// Time values are epoch seconds passed in by the caller. The domain never
/// reads a clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "LeaseRepr")]
pub struct Lease {
    /// Lease id.
    pub id: LeaseId,
    /// The leased slot.
    pub slot: SlotId,
    /// Who holds the lease.
    pub holder: PrincipalRef,
    /// Proof of the lease.
    pub token: LeaseToken,
    /// When the lease began.
    pub issued_at: u64,
    /// When the lease ends. The lease is expired from this second on.
    pub expires_at: u64,
    /// Spending limits.
    pub budget: Budget,
}

/// The wire shape of a [`Budget`], checked by [`Budget::new`].
#[derive(Deserialize)]
struct BudgetRepr {
    max_tokens: u64,
    max_wall_secs: u64,
}

impl TryFrom<BudgetRepr> for Budget {
    type Error = DomainError;

    fn try_from(r: BudgetRepr) -> Result<Self, Self::Error> {
        Self::new(r.max_tokens, r.max_wall_secs)
    }
}

/// The wire shape of a [`Lease`], checked by [`Lease::new`].
#[derive(Deserialize)]
struct LeaseRepr {
    id: LeaseId,
    slot: SlotId,
    holder: PrincipalRef,
    token: LeaseToken,
    issued_at: u64,
    expires_at: u64,
    budget: Budget,
}

impl TryFrom<LeaseRepr> for Lease {
    type Error = DomainError;

    fn try_from(r: LeaseRepr) -> Result<Self, Self::Error> {
        Self::new(
            r.id,
            r.slot,
            r.holder,
            r.token,
            r.issued_at,
            r.expires_at,
            r.budget,
        )
    }
}

impl Lease {
    /// Build a lease. `expires_at` must be after `issued_at`.
    pub fn new(
        id: LeaseId,
        slot: SlotId,
        holder: PrincipalRef,
        token: LeaseToken,
        issued_at: u64,
        expires_at: u64,
        budget: Budget,
    ) -> Result<Self, DomainError> {
        if expires_at <= issued_at {
            return Err(DomainError::Invalid {
                kind: "lease",
                value: format!("issued_at={issued_at} expires_at={expires_at}"),
                reason: "must expire after it is issued",
            });
        }
        Ok(Self {
            id,
            slot,
            holder,
            token,
            issued_at,
            expires_at,
            budget,
        })
    }

    /// True when `now` is at or after `expires_at`.
    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }

    /// Whether this lease holds `slot` at time `at`: the same slot, and not
    /// expired at `at` (spec 0007, R1).
    pub fn holds_slot_at(&self, slot: &SlotId, at: u64) -> bool {
        self.slot == *slot && !self.is_expired(at)
    }

    /// Return a copy whose deadline is `extend_by` seconds later.
    ///
    /// Needs the lease token (spec 0007, R2). Checks run in this order:
    /// - [`DomainError::TokenMismatch`] if `token` is not the lease token;
    /// - [`DomainError::LeaseExpired`] if the lease is already expired;
    /// - [`DomainError::Invalid`] if `extend_by` is zero (spec 0007, R3);
    /// - [`DomainError::TimeOverflow`] if the new deadline does not fit.
    pub fn renew(&self, token: &LeaseToken, now: u64, extend_by: u64) -> Result<Self, DomainError> {
        if !self.token.matches(token) {
            return Err(DomainError::TokenMismatch(self.id.to_string()));
        }
        if self.is_expired(now) {
            return Err(DomainError::LeaseExpired {
                lease: self.id.to_string(),
                expires_at: self.expires_at,
                now,
            });
        }
        if extend_by == 0 {
            return Err(DomainError::Invalid {
                kind: "lease renewal",
                value: "extend_by=0".to_string(),
                reason: "must extend by at least one second",
            });
        }
        let expires_at = self
            .expires_at
            .checked_add(extend_by)
            .ok_or(DomainError::TimeOverflow)?;
        Ok(Self {
            expires_at,
            ..self.clone()
        })
    }
}

/// Refuse `new` when a lease in `held` holds the same slot at `new.issued_at`
/// (spec 0007, R1). The ledger has no clock, so the new lease's `issued_at`
/// is the time of the check. Every [`LeaseLedger`] adapter calls this in
/// `issue`.
pub fn check_slot_free<'a>(
    held: impl IntoIterator<Item = &'a Lease>,
    new: &Lease,
) -> Result<(), DomainError> {
    match held
        .into_iter()
        .find(|l| l.holds_slot_at(&new.slot, new.issued_at))
    {
        Some(l) => Err(DomainError::SlotLeased {
            slot: new.slot.to_string(),
            lease: l.id.to_string(),
            until: l.expires_at,
        }),
        None => Ok(()),
    }
}

/// Port for lease storage. Adapters in other crates implement it.
pub trait LeaseLedger {
    /// Record a new lease. Fails with [`DomainError::LeaseExists`] if the id is
    /// taken, then with [`DomainError::SlotLeased`] if another lease holds the
    /// slot (spec 0007, R1). Adapters check the slot with [`check_slot_free`].
    fn issue(&mut self, lease: Lease) -> Result<(), DomainError>;

    /// Extend a lease with its token. Fails if it is missing, or for any
    /// reason [`Lease::renew`] fails.
    fn renew(
        &mut self,
        id: &LeaseId,
        token: &LeaseToken,
        now: u64,
        extend_by: u64,
    ) -> Result<Lease, DomainError>;

    /// Remove a lease and return it. Fails with [`DomainError::LeaseNotFound`] if absent.
    fn revoke(&mut self, id: &LeaseId) -> Result<Lease, DomainError>;

    /// Fetch a lease by id.
    fn find(&self, id: &LeaseId) -> Result<Lease, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok() -> LeaseToken {
        LeaseToken::new("s3cr3t-token-value").unwrap()
    }

    fn lease_on(id: &str, slot: &str, issued: u64, expires: u64) -> Lease {
        Lease::new(
            LeaseId::new(id).unwrap(),
            SlotId::new(slot).unwrap(),
            PrincipalRef::new("agent-1").unwrap(),
            tok(),
            issued,
            expires,
            Budget::new(1000, 60).unwrap(),
        )
        .unwrap()
    }

    fn lease(issued: u64, expires: u64) -> Lease {
        Lease::new(
            LeaseId::new("lease-1").unwrap(),
            SlotId::new("slot-1").unwrap(),
            PrincipalRef::new("agent-1").unwrap(),
            LeaseToken::new("s3cr3t-token-value").unwrap(),
            issued,
            expires,
            Budget::new(1000, 60).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn expiry_boundary() {
        let l = lease(100, 200);
        assert!(!l.is_expired(99));
        assert!(!l.is_expired(199));
        assert!(l.is_expired(200));
        assert!(l.is_expired(201));
    }

    #[test]
    fn renew_extends_the_deadline() {
        let l = lease(100, 200);
        let r = l.renew(&tok(), 150, 50).unwrap();
        assert_eq!(r.expires_at, 250);
        assert_eq!(r.issued_at, 100);
        assert_eq!(r.id, l.id);
        assert_eq!(l.expires_at, 200, "original unchanged");
    }

    #[test]
    fn renew_refuses_an_expired_lease() {
        let l = lease(100, 200);
        assert_eq!(
            l.renew(&tok(), 200, 50),
            Err(DomainError::LeaseExpired {
                lease: "lease-1".into(),
                expires_at: 200,
                now: 200
            })
        );
        assert!(l.renew(&tok(), 300, 50).is_err());
    }

    #[test]
    fn renew_overflow_is_an_error() {
        let l = lease(100, u64::MAX - 1);
        assert_eq!(l.renew(&tok(), 150, 10), Err(DomainError::TimeOverflow));
    }

    #[test]
    fn lease_must_end_after_it_begins() {
        let make = |i, e| {
            Lease::new(
                LeaseId::new("l").unwrap(),
                SlotId::new("s").unwrap(),
                PrincipalRef::new("p").unwrap(),
                LeaseToken::new("tok").unwrap(),
                i,
                e,
                Budget::new(1, 1).unwrap(),
            )
        };
        assert!(make(10, 10).is_err());
        assert!(make(10, 5).is_err());
        assert!(make(10, 11).is_ok());
    }

    #[test]
    fn token_debug_does_not_print_the_value() {
        let t = LeaseToken::new("s3cr3t-token-value").unwrap();
        assert!(!format!("{t:?}").contains("s3cr3t"));
        let l = lease(1, 2);
        assert!(!format!("{l:?}").contains("s3cr3t"));
        assert!(!format!("{l:#?}").contains("s3cr3t"));
        assert_eq!(t.expose(), "s3cr3t-token-value");
    }

    #[test]
    fn newtypes_are_validated() {
        assert!(LeaseId::new("lease-1").is_ok());
        assert!(LeaseId::new("Lease 1").is_err());
        assert!(LeaseId::new("").is_err());
        assert!(PrincipalRef::new("spiffe://cas/ns/agent_1").is_ok());
        assert!(PrincipalRef::new("a b").is_err());
        assert!(PrincipalRef::new("").is_err());
        assert!(LeaseToken::new("").is_err());
        assert!(LeaseToken::new("has space").is_err());
        assert!(LeaseToken::new("line\nbreak").is_err());
        assert!(LeaseToken::new("t".repeat(257)).is_err());
        assert!(Budget::new(0, 1).is_err());
        assert!(Budget::new(1, 0).is_err());
    }

    #[test]
    fn serde_validates_newtypes() {
        assert!(serde_json::from_str::<LeaseToken>("\"bad token\"").is_err());
        assert!(serde_json::from_str::<PrincipalRef>("\"\"").is_err());
        let l = lease(1, 2);
        let json = serde_json::to_string(&l).unwrap();
        assert_eq!(serde_json::from_str::<Lease>(&json).unwrap(), l);
    }

    #[test]
    fn serde_rejects_what_the_constructors_reject() {
        let good = serde_json::to_value(lease(100, 200)).unwrap();
        let mut ends_early = good.clone();
        ends_early["expires_at"] = serde_json::json!(100);
        assert!(serde_json::from_value::<Lease>(ends_early).is_err());
        let mut zero_budget = good.clone();
        zero_budget["budget"]["max_tokens"] = serde_json::json!(0);
        assert!(serde_json::from_value::<Lease>(zero_budget).is_err());
        assert!(serde_json::from_str::<Budget>(r#"{"max_tokens":0,"max_wall_secs":60}"#).is_err());
        assert!(
            serde_json::from_str::<Budget>(r#"{"max_tokens":1000,"max_wall_secs":0}"#).is_err()
        );
        assert_eq!(
            serde_json::from_value::<Lease>(good).unwrap(),
            lease(100, 200)
        );
    }

    #[test]
    fn renew_requires_the_lease_token() {
        let l = lease(100, 200);
        let wrong = LeaseToken::new("wrong-token").unwrap();
        assert_eq!(
            l.renew(&wrong, 150, 10),
            Err(DomainError::TokenMismatch("lease-1".into()))
        );
        assert_eq!(l.renew(&tok(), 150, 10).unwrap().expires_at, 210);
    }

    #[test]
    fn renew_checks_the_token_before_expiry() {
        let l = lease(100, 200);
        let wrong = LeaseToken::new("wrong-token").unwrap();
        assert!(matches!(
            l.renew(&wrong, 300, 10),
            Err(DomainError::TokenMismatch(_))
        ));
    }

    #[test]
    fn token_mismatch_does_not_print_the_token() {
        let l = lease(100, 200);
        let guess = LeaseToken::new("guess-1234").unwrap();
        let err = l.renew(&guess, 150, 10).unwrap_err();
        let text = format!("{err} {err:?}");
        assert!(!text.contains("s3cr3t-token-value"), "{text}");
        assert!(!text.contains("guess-1234"), "{text}");
    }

    #[test]
    fn renew_rejects_a_zero_extension() {
        let l = lease(100, 200);
        assert!(matches!(
            l.renew(&tok(), 150, 0),
            Err(DomainError::Invalid { .. })
        ));
    }

    #[test]
    fn token_matches_only_the_same_text() {
        let abc = LeaseToken::new("abc").unwrap();
        assert!(abc.matches(&LeaseToken::new("abc").unwrap()));
        assert!(!abc.matches(&LeaseToken::new("abd").unwrap()));
        assert!(!abc.matches(&LeaseToken::new("abcd").unwrap()));
    }

    #[test]
    fn holds_slot_at_until_expiry() {
        let l = lease(100, 200);
        let slot = SlotId::new("slot-1").unwrap();
        // The ledger has no clock: a lease holds its slot at any time before it expires.
        assert!(l.holds_slot_at(&slot, 50));
        assert!(l.holds_slot_at(&slot, 199));
        assert!(!l.holds_slot_at(&slot, 200));
        assert!(!l.holds_slot_at(&SlotId::new("slot-2").unwrap(), 150));
    }

    #[test]
    fn check_slot_free_refuses_a_second_live_lease() {
        let held = [lease_on("lease-1", "slot-1", 100, 200)];
        assert_eq!(
            check_slot_free(&held, &lease_on("lease-2", "slot-1", 150, 250)),
            Err(DomainError::SlotLeased {
                slot: "slot-1".into(),
                lease: "lease-1".into(),
                until: 200,
            })
        );
    }

    #[test]
    fn check_slot_free_allows_after_expiry_and_on_other_slots() {
        let held = [lease_on("lease-1", "slot-1", 100, 200)];
        assert!(check_slot_free(&held, &lease_on("lease-2", "slot-1", 200, 300)).is_ok());
        assert!(check_slot_free(&held, &lease_on("lease-2", "slot-2", 150, 250)).is_ok());
    }
}
