//! Small application-host primitives, independent of forge features and protocols.
//!
//! [`BlockingBudget`] bounds submitted blocking work, including jobs still queued in
//! Tokio. An admitted worker owns its capacity until it exits, even when the request
//! awaiting its result is cancelled. Compose distinct budgets for workloads with
//! different resource costs; this crate never creates a global pool or hidden queue.
//!
//! The host owns budget configuration and shutdown. Feature code receives only its
//! budget handle. Dependencies exclude domain entities, HTTP and persistence.

mod blocking;
mod blocking_reservation;

pub use blocking::{AdmissionError, BlockingBudget};
pub use blocking_reservation::BlockingReservation;
