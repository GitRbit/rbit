//! # rbit-workflow
//!
//! The pure half of rbit Actions: everything that can be decided from a workflow file and
//! an event without touching Git, the database or the network. No I/O, no async, and no
//! dependency on other rbit crates, so every rule here is tested with plain tables.
//!
//! * [`pattern`] — GitHub's filter-pattern dialect for `branches`, `tags` and `paths`.
//!
//! The Actions foundation (workstream F of `docs/design/actions.md`) owns this crate.

pub mod pattern;
