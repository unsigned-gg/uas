//! In-memory adapter for the [`LeaseLedger`] port.

use std::collections::BTreeMap;

use cas_domain::{check_slot_free, DomainError, Lease, LeaseId, LeaseLedger, LeaseToken};

/// Keeps leases in a sorted map. Data is lost when the process exits.
#[derive(Debug, Default)]
pub struct InMemoryLedger {
    leases: BTreeMap<LeaseId, Lease>,
}

impl LeaseLedger for InMemoryLedger {
    fn issue(&mut self, lease: Lease) -> Result<(), DomainError> {
        if self.leases.contains_key(&lease.id) {
            return Err(DomainError::LeaseExists(lease.id.to_string()));
        }
        check_slot_free(self.leases.values(), &lease)?;
        self.leases.insert(lease.id.clone(), lease);
        Ok(())
    }

    fn renew(
        &mut self,
        id: &LeaseId,
        token: &LeaseToken,
        now: u64,
        extend_by: u64,
    ) -> Result<Lease, DomainError> {
        let current = self.find(id)?;
        let renewed = current.renew(token, now, extend_by)?;
        self.leases.insert(id.clone(), renewed.clone());
        Ok(renewed)
    }

    fn revoke(&mut self, id: &LeaseId) -> Result<Lease, DomainError> {
        self.leases
            .remove(id)
            .ok_or_else(|| DomainError::LeaseNotFound(id.to_string()))
    }

    fn find(&self, id: &LeaseId) -> Result<Lease, DomainError> {
        self.leases
            .get(id)
            .cloned()
            .ok_or_else(|| DomainError::LeaseNotFound(id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cas_domain::{Budget, PrincipalRef, SlotId};

    fn tok() -> LeaseToken {
        LeaseToken::new("tok").unwrap()
    }

    fn lease(id: &str, expires: u64) -> Lease {
        lease_at(id, "slot-1", 0, expires)
    }

    fn lease_at(id: &str, slot: &str, issued: u64, expires: u64) -> Lease {
        Lease::new(
            LeaseId::new(id).unwrap(),
            SlotId::new(slot).unwrap(),
            PrincipalRef::new("agent-1").unwrap(),
            tok(),
            issued,
            expires,
            Budget::new(10, 10).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn issue_find_renew_revoke() {
        let mut ledger = InMemoryLedger::default();
        ledger.issue(lease("a", 100)).unwrap();
        let id = LeaseId::new("a").unwrap();
        assert_eq!(ledger.find(&id).unwrap().expires_at, 100);
        assert_eq!(ledger.renew(&id, &tok(), 50, 25).unwrap().expires_at, 125);
        assert_eq!(ledger.find(&id).unwrap().expires_at, 125);
        ledger.revoke(&id).unwrap();
        assert_eq!(
            ledger.find(&id),
            Err(DomainError::LeaseNotFound("a".into()))
        );
    }

    #[test]
    fn duplicate_missing_and_expired_error() {
        let mut ledger = InMemoryLedger::default();
        ledger.issue(lease("a", 100)).unwrap();
        assert_eq!(
            ledger.issue(lease("a", 100)),
            Err(DomainError::LeaseExists("a".into()))
        );
        let missing = LeaseId::new("z").unwrap();
        assert!(ledger.revoke(&missing).is_err());
        assert!(ledger.renew(&missing, &tok(), 1, 1).is_err());
        let id = LeaseId::new("a").unwrap();
        assert!(matches!(
            ledger.renew(&id, &tok(), 100, 5),
            Err(DomainError::LeaseExpired { .. })
        ));
        assert_eq!(ledger.find(&id).unwrap().expires_at, 100, "unchanged");
    }

    #[test]
    fn second_live_lease_on_a_slot_is_refused() {
        let mut ledger = InMemoryLedger::default();
        ledger.issue(lease("a", 100)).unwrap();
        assert!(matches!(
            ledger.issue(lease_at("b", "slot-1", 50, 150)),
            Err(DomainError::SlotLeased { .. })
        ));
        assert!(
            ledger.find(&LeaseId::new("b").unwrap()).is_err(),
            "not stored"
        );
        ledger.issue(lease_at("c", "slot-2", 50, 150)).unwrap();
        ledger.issue(lease_at("d", "slot-1", 100, 200)).unwrap();
    }

    #[test]
    fn a_revoked_lease_frees_its_slot() {
        let mut ledger = InMemoryLedger::default();
        ledger.issue(lease("a", 100)).unwrap();
        ledger.revoke(&LeaseId::new("a").unwrap()).unwrap();
        ledger.issue(lease_at("b", "slot-1", 50, 150)).unwrap();
    }
}
