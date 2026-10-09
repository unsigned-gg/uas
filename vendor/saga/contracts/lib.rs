//! saga-domain — pure DDD core for the saga workflow suite.
//! No I/O, no suite dependencies.

pub mod apply;
pub mod decision;
pub mod evidence;
pub mod flow;
pub mod ids;
pub mod ipc;
pub mod journal;
pub mod pick;
pub mod raid;
pub mod run_catalog;
pub mod tournament;
pub mod world_model;

pub use apply::{
    apply_run, cause_reason, AppliedChange, ApplyError, DagAttempt, DagTask, RunSummary,
    RunTaskSummary,
};
pub use decision::Decision;
pub use evidence::EvidenceTier;
pub use flow::{AgentSelection, Budget, FlowGraph, GateKind, Limits, PathKind, StepSpec};
pub use ids::{RunId, StepId};
pub use ipc::{IpcDeliveryTier, IpcPayloadDescriptor};
pub use journal::{Journal, JournalError, RunEvent};
pub use pick::{pick_next, PickCandidate};
pub use raid::{
    GateDecision, LandingStatus, PendingGateSummary, RaidConfig, RaidRetireReport, RaidRunSpec,
    RaidRunStatus, RaidStatusReport,
};
pub use run_catalog::RunCatalogEntry;
pub use tournament::{TournamentCandidateSummary, TournamentSummary};
pub use world_model::{
    CategoryStats, EffortLevel, ModelEffort, RunOutcomeRecord, TaskOutcomeRecord, WorldModelSummary,
};
