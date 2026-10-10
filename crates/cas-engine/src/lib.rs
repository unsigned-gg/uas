//! Durable workflow engine for the Cerebral Agent Substrate.
//!
//! `cas-engine` provides Autumn-Harvest style event-sourced workflows,
//! activities, and saga compensators. It manages the execution lifecycle
//! of agent sessions on top of [`cas_domain::Slot`] and [`cas_domain::Lease`].
//!
//! # Architecture
//!
//! A workflow session progresses through deterministic lifecycle states:
//! `Unstarted` -> `Running` -> `Compensating` (on error) -> `Succeeded` / `Failed`.
//! Every execution turn and activity emits an immutable [`WorkflowEvent`] into
//! an [`EventStore`]. Crashed or suspended sessions can be reconstructed
//! via [`WorkflowSession::replay_from_events`] without re-executing actions
//! or re-spending model tokens.
//!
//! # Example
//!
//! ```
//! use cas_domain::{Attestation, EvidenceTier, LeaseId, PrincipalRef, SlotId};
//! use cas_engine::{Activity, ActivityKind, ActivityResult, SessionId, SessionState, WorkflowSession};
//!
//! let session_id = SessionId::new("sess-1").unwrap();
//! let slot_id = SlotId::new("slot-1").unwrap();
//! let lease_id = LeaseId::new("lease-1").unwrap();
//!
//! let mut session = WorkflowSession::new(session_id.clone());
//! assert_eq!(session.state(), SessionState::Unstarted);
//!
//! session.start(slot_id, lease_id, 1000).unwrap();
//! assert_eq!(session.state(), SessionState::Running);
//!
//! let activity = Activity::new("act-check", "make check", ActivityKind::Verify);
//! session.start_activity(activity.clone(), 1005).unwrap();
//!
//! let attestation = Attestation {
//!     signer: PrincipalRef::new("cas/builder-1").unwrap(),
//!     evidence: EvidenceTier::Verified,
//!     receipt: "cargo check: 0 errors".into(),
//!     at: 1010,
//! };
//!
//! let result = ActivityResult {
//!     activity_id: activity.id,
//!     success: true,
//!     attestation,
//!     summary: Some("passed".into()),
//! };
//! session.complete_activity(result, 1010).unwrap();
//! session.finish_success(1020).unwrap();
//!
//! assert_eq!(session.state(), SessionState::Succeeded);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod activity;
mod compensator;
mod cortex;
mod cpm;
mod error;
mod event;
mod landing;
mod reverie;
mod saga;
mod session;
mod soma;
mod wave;
mod workflow;

pub use activity::{Activity, ActivityKind, ActivityResult};
pub use compensator::{
    BranchPruneCompensator, CompensationReceipt, Compensator, CompositeCompensator,
    LeaseReturnCompensator, NoopCompensator, RecordingCompensator, WorktreeCleanupCompensator,
};
pub use cortex::{CortexMeshEvent, MeshIntent, SettleCard, SettleQueue, SettleStatus};
pub use cpm::{CpmSchedule, CpmStepSchedule};
pub use error::EngineError;
pub use event::{EventStore, InMemoryEventStore, WorkflowEvent};
pub use landing::{
    LandingActivity, LandingConfig, LandingError, LandingInput, LandingOutcome, ReconcileOutcome,
};
pub use reverie::{InMemoryReverieClient, ReverieClient, ReverieContextQuery, ReverieMemoryRecord};
pub use saga::{FlowMetadata, SagaFlowCoordinator, SagaFlowDef, SagaStep};
pub use session::{SessionId, SessionState};
pub use soma::SomaAdmissionGate;
pub use wave::{
    classify_step, validate_verdict, AgentRole, ExecutionWave, ModelTier, WaveCoordinator,
    WavePlan, WaveStep, WorkspaceMode, DEFAULT_MAX_WAVE_CONCURRENCY, MAX_VERDICT_LINES,
};
pub use workflow::WorkflowSession;
