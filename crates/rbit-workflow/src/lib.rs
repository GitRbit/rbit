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
//! * [`builtins`] — `uses:` resolved to the built-in actions `rbit-runner` carries, with
//!   their `actions/*` aliases and checked `with:` inputs.
//! * [`cron`] — `schedule` expressions and their next fire time.
//! * [`expr`] — GitHub's expression language: one evaluator, shared by the server and
//!   `rbit-runner`, each supplying the contexts it has.
//! * [`plan()`] — the jobs of a run as [`spec::JobTemplate`]s: matrices expanded and the
//!   fields the server decides at run creation evaluated ([`expr`]) with the run's
//!   [`RunContext`]; and the run's name and concurrency group.
//! * [`spec`] — the stored [`spec::JobTemplate`] of a planned job and the [`spec::JobSpec`]
//!   a runner receives for one task.
//! * [`protocol`] — the request and response types of runner protocol v1, its version,
//!   headers, body caps and error codes.
//! * [`secret`] — the zeroizing, redacting [`secret::Secret`] those messages carry.
//!
//! The Actions foundation (workstream F of `docs/design/actions.md`) owns this crate.

pub mod builtins;
pub mod cron;
pub mod error;
pub mod expr;
pub mod limits;
pub mod model;
pub mod parse;
pub mod pattern;
pub mod plan;
pub mod protocol;
pub mod secret;
pub mod spec;
pub mod trigger;
pub mod yaml;

pub use error::{ErrorKind, Location, WorkflowError};
pub use limits::{Deadline, Limits};
pub use model::Workflow;
pub use parse::parse;
pub use plan::{GroupContext, RunContext, concurrency_group, plan, run_name};
pub use trigger::{ChangedPaths, Event, LazyPaths, matches};
