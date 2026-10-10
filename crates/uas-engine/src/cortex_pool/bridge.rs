//! The lease bridge (spec 0026, Lease bridge and D4, D5, D9).

use uas_domain::{
    check_slot_free, Budget, DomainError, Lease, LeaseId, LeaseToken, PrincipalRef, SlotId,
};

/// The smallest `lease_ms` the adapter accepts (D9).
pub const MIN_LEASE_MS: u64 = 300;

/// `ttl = max(1, ceil(lease_ms / 1000))`.
pub fn ttl_secs(lease_ms: u64) -> u64 {
    lease_ms.div_ceil(1000).max(1)
}

/// Issue a UAS lease of `ttl + 1` seconds at `now`, after R1 against `held`.
#[allow(clippy::too_many_arguments)]
pub fn issue(
    held: &[&Lease],
    id: LeaseId,
    slot: SlotId,
    holder: PrincipalRef,
    token: LeaseToken,
    budget: Budget,
    now: u64,
    lease_ms: u64,
) -> Result<Lease, DomainError> {
    let expires_at = now
        .checked_add(ttl_secs(lease_ms) + 1)
        .ok_or(DomainError::TimeOverflow)?;
    let lease = Lease::new(id, slot, holder, token, now, expires_at, budget)?;
    check_slot_free(held.iter().copied(), &lease)?;
    Ok(lease)
}

/// Renew up to `now + ttl + 1`, and only when `expires_at` is below it. Never by a fixed step
/// (D4): `renew` adds to the old expiry, so the extension is the difference to the target.
pub fn renew(lease: &Lease, now: u64, lease_ms: u64) -> Result<Lease, DomainError> {
    let target = now
        .checked_add(ttl_secs(lease_ms) + 1)
        .ok_or(DomainError::TimeOverflow)?;
    if lease.expires_at >= target {
        return Ok(lease.clone());
    }
    lease.renew(&lease.token, now, target - lease.expires_at)
}

/// The `Nack` delay: `min(1000 * 2^(a - 1), 60000)` ms, `a = max(1, attempt)` (D5). Saturating,
/// so `u32::MAX` does not overflow.
pub fn nack_delay_ms(attempt: u32) -> u64 {
    let shift = attempt.max(1) - 1;
    1000u64
        .saturating_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX))
        .min(60_000)
}
