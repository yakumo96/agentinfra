//! `ws_apply` semantics, independent of how a workspace is stored.
//!
//! All operations of one call are validated against a staged view first
//! (expectations, text matches, path safety); if any fails nothing is written.
//! The result is an ordered [`Plan`] that the workspace writer executes, after
//! which the call's changes are committed as one commit.

mod path;
mod plan;
mod view;

pub use path::{NormPath, normalize};
pub use plan::{Action, Op, Plan, plan};
pub use view::{FileView, Kind, Node};
