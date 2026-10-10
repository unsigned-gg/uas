//! Domain model for the Cerebral Agent Substrate.
//!
//! This crate is the bottom layer. Every other `cas-*` crate may depend on it.
//! It depends on no other `cas-*` crate. It does no I/O: no files, no network,
//! no processes, no clock, no async runtime. Time is passed in as epoch
//! seconds. Adapters live in other crates and implement the [`LeaseLedger`]
//! port.
//!
//! # Example
//!
//! ```
//! use cas_domain::{Slot, SlotId, SlotState, WorktreeRef};
//!
//! let slot = Slot::new(SlotId::new("slot-1").unwrap());
//! assert_eq!(slot.state(), SlotState::Cold);
//! let slot = slot.warm(WorktreeRef::new("wt/slot-1").unwrap()).unwrap();
//! let slot = slot.lease().unwrap();
//! assert_eq!(slot.state(), SlotState::Leased);
//! // A leased slot cannot be warmed again.
//! assert!(slot.warm(WorktreeRef::new("wt/other").unwrap()).is_err());
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod attestation;
mod envelope;
mod error;
mod ident;
mod landing;
mod lease;
mod policy;
mod slot;

pub use attestation::{Attestation, EvidenceTier};
pub use envelope::Envelope;
pub use error::DomainError;
pub use landing::{
    AuditStatus, CostSource, HumanRaise, LandingFacts, LandingReceipt, LandingReceiptError,
};
pub use lease::{check_slot_free, Budget, Lease, LeaseId, LeaseLedger, LeaseToken, PrincipalRef};
pub use policy::{PolicyTier, SomaClassification};
pub use slot::{Slot, SlotId, SlotState, WorktreeRef};
