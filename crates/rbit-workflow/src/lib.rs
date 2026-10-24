//! # rbit-workflow
//!
//! The pure half of rbit Actions: everything that can be decided from a workflow file and
//! an event without touching Git, the database or the network. No I/O, no async, and no
//! dependency on other rbit crates, so every rule here is tested with plain tables.
//!
//! * [`yaml`] — the YAML document tree, parsed within budgets.
//! * [`parse()`] — a workflow file into the typed [`model`], within [`Limits`] and a
//!   [`Deadline`]; problems come back as a [`WorkflowError`] with a line and column.
//! * [`pattern`] — GitHub's filter-pattern dialect for `branches`, `tags` and `paths`.
//! * [`trigger`] — whether a push, pull request, dispatch or schedule tick starts a
//!   workflow, and dispatch input checks.
//! * [`cron`] — `schedule` expressions and their next fire time.
//!
//! The Actions foundation (workstream F of `docs/design/actions.md`) owns this crate.

pub mod cron;
pub mod error;
pub mod limits;
pub mod model;
pub mod parse;
pub mod pattern;
pub mod trigger;
pub mod yaml;

pub use error::{ErrorKind, Location, WorkflowError};
pub use limits::{Deadline, Limits};
pub use model::Workflow;
pub use parse::parse;
pub use trigger::{ChangedPaths, Event, LazyPaths, matches};
