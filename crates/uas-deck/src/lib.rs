// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Operator dashboard for the Unsigned Agent Substrate (spec 0018, slices 1 and 2).
//!
//! `uas-deck` serves a read-only page: the session table and the settle queue.
//! Routes use Axum and the HTML uses Maud. Settle cards come from the in-process
//! queue or from a Cortex body that a `PendingFetch` supplies. This slice has no
//! actions, no event stream and no HTTP fetch yet.

#![forbid(unsafe_code)]

mod card;
mod cortex;
mod error;
mod read_model;
mod server;
mod source;
mod views;

pub use card::{DeckCard, Guard, PendingCards, PlanStep};
pub use cortex::{parse_pending, CortexSettleSource, PendingFetch};
pub use error::DeckError;
pub use read_model::SessionRow;
pub use server::{bind, router, serve};
pub use source::{DeckState, SessionSource, SettleSource};
pub use views::{evidence_badge, page, session_table, settle_cards, settle_disconnected};
