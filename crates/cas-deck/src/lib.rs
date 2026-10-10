//! Operator dashboard for the Cerebral Agent Substrate (spec 0018, slice 1).
//!
//! `cas-deck` serves a read-only page: the session table and the settle queue.
//! Routes use Axum and the HTML uses Maud. This slice has no actions, no event
//! stream and no Cortex bridge yet.

#![forbid(unsafe_code)]

mod error;
mod read_model;
mod server;
mod source;
mod views;

pub use error::DeckError;
pub use read_model::SessionRow;
pub use server::{bind, router, serve};
pub use source::{DeckState, SessionSource, SettleSource};
pub use views::{evidence_badge, page, session_table, settle_cards, settle_disconnected};
