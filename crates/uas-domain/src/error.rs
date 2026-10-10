use crate::SlotState;

/// Errors the domain can return.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    /// A validated text value was rejected.
    #[error("invalid {kind} {value:?}: {reason}")]
    Invalid {
        /// Which kind of value was rejected, for example `slot id`.
        kind: &'static str,
        /// The rejected text. Never set for secrets.
        value: String,
        /// Why the text was rejected.
        reason: &'static str,
    },
    /// The requested slot state change is not allowed.
    #[error("illegal slot transition: cannot {op} a slot that is {from:?}")]
    IllegalTransition {
        /// State before the change.
        from: SlotState,
        /// The operation the caller asked for.
        op: &'static str,
    },
    /// A lease has expired and cannot be renewed.
    #[error("lease {lease} expired at {expires_at} (now {now})")]
    LeaseExpired {
        /// The lease id.
        lease: String,
        /// When the lease ended, in epoch seconds.
        expires_at: u64,
        /// The time of the attempt, in epoch seconds.
        now: u64,
    },
    /// A time calculation went past the range of `u64`.
    #[error("time value overflow")]
    TimeOverflow,
    /// A lease with this id already exists.
    #[error("lease {0} already exists")]
    LeaseExists(String),
    /// No lease has this id.
    #[error("lease {0} not found")]
    LeaseNotFound(String),
    /// Another lease holds the slot (spec 0007, R1).
    #[error("slot {slot} is held by lease {lease} until {until}")]
    SlotLeased {
        /// The slot that was asked for.
        slot: String,
        /// The lease that holds it.
        lease: String,
        /// When that lease expires, in epoch seconds.
        until: u64,
    },
    /// The presented token does not match the lease (spec 0007, R2). Holds the
    /// lease id only, never a token.
    #[error("token does not match lease {0}")]
    TokenMismatch(String),
    /// A task violates the policy tier constraint.
    #[error("policy violation: {action} not allowed under {tier}: {reason}")]
    PolicyViolation {
        /// Action attempted.
        action: String,
        /// Active policy tier.
        tier: String,
        /// Why it was rejected.
        reason: &'static str,
    },
}
