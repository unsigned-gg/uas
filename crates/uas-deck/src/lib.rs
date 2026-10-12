// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Operator dashboard for the Unsigned Agent Substrate (spec 0018, slices 1, 2, and 3).
//!
//! `uas-deck` serves a read-only page: the session table and the settle queue.
//! Routes use Axum and the HTML uses Maud. Settle cards come from the in-process
//! queue or from a Cortex body that a `PendingFetch` supplies. A background `CortexPoller`
//! updates a memory `SnapshotFetch` (spec 0018 D9).

#![forbid(unsafe_code)]

mod card;
mod cortex;
mod diff;
mod error;
mod read_model;
mod server;
mod source;
mod views;

pub use card::{DeckCard, Guard, PendingCards, PlanStep};
pub use cortex::{
    fetch_pending_http, parse_pending, CortexPoller, CortexPollerConfig, CortexSettleSource,
    PendingFetch, SnapshotFetch,
};
pub use diff::{
    compute_diff, render_diff_modal, render_diff_table, DiffLine, DiffSummary, DiffTable, DiffTag,
};
pub use error::DeckError;
pub use read_model::SessionRow;
pub use server::{bind, router, serve};
pub use source::{DeckState, SessionSource, SettleSource};
pub use views::{evidence_badge, page, session_table, settle_cards, settle_disconnected};
