//! Claim a waiting job or renew the token of an idempotent poll's existing lease.
use std::fmt;

mod application;
mod ports;

pub use application::RunnerClaims;
pub use ports::{ClaimQueue, LeaseReplay, QueueClaimResult, Specification};

/// A committed storage claim, with the original concrete records moved intact.
pub struct Claimed<J, T> {
    pub job: J,
    pub task: T,
}

/// Keep the existing boxed specification outcome; no extra specification allocation.
pub enum Claim<S> {
    Task(Box<S>),
    Nothing,
    /// An idempotency key was spent by a claimed job whose specification failed.
    Spent,
}

pub enum Resumed<S> {
    Task(Box<S>),
    Spent,
    Unknown,
}

// Composition may supply confidential records without a Debug implementation.
// Diagnostic formatting never traverses a task or a workflow specification.
impl<J, T> fmt::Debug for Claimed<J, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Claimed").finish_non_exhaustive()
    }
}
impl<S> fmt::Debug for Claim<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Task(_) => "Claim::Task([redacted])",
            Self::Nothing => "Claim::Nothing",
            Self::Spent => "Claim::Spent",
        })
    }
}
impl<S> fmt::Debug for Resumed<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Task(_) => "Resumed::Task([redacted])",
            Self::Spent => "Resumed::Spent",
            Self::Unknown => "Resumed::Unknown",
        })
    }
}
