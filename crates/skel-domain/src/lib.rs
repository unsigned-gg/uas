//! Domain model for skel.
//!
//! This crate holds pure logic. It does no I/O: no files, no network, no
//! processes, no async runtime. Adapters live in other crates and implement
//! the [`ItemRepository`] port.
//!
//! # Example
//!
//! ```
//! use skel_domain::{Item, ItemId, ItemState};
//!
//! let id = ItemId::new("first-item").unwrap();
//! let item = Item::new(id);
//! assert_eq!(item.state(), ItemState::Draft);
//! let item = item.activate().unwrap();
//! assert_eq!(item.state(), ItemState::Active);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod item;
mod repository;

pub use error::DomainError;
pub use item::{Item, ItemId, ItemState};
pub use repository::ItemRepository;
