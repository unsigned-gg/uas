//! In-memory adapter for the [`LeaseLedger`] port.

use std::collections::BTreeMap;

use cas_domain::{DomainError, Lease, LeaseId, LeaseLedger};

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
        self.leases.insert(lease.id.clone(), lease);
        Ok(())
    }

    fn renew(&mut self, id: &LeaseId, now: u64, extend_by: u64) -> Result<Lease, DomainError> {
        let current = self.find(id)?;
        let renewed = current.renew(now, extend_by)?;
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
    use cas_domain::{Budget, LeaseToken, PrincipalRef, SlotId};

    fn lease(id: &str, expires: u64) -> Lease {
        Lease::new(
            LeaseId::new(id).unwrap(),
            SlotId::new("slot-1").unwrap(),
            PrincipalRef::new("agent-1").unwrap(),
            LeaseToken::new("tok").unwrap(),
            0,
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
        assert_eq!(ledger.renew(&id, 50, 25).unwrap().expires_at, 125);
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
        assert!(ledger.renew(&missing, 1, 1).is_err());
        let id = LeaseId::new("a").unwrap();
        assert!(matches!(
            ledger.renew(&id, 100, 5),
            Err(DomainError::LeaseExpired { .. })
        ));
        assert_eq!(ledger.find(&id).unwrap().expires_at, 100, "unchanged");
    }
}
