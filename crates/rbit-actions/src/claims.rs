//! Claim a waiting job or renew the token of an idempotent poll's existing lease.

mod application;
mod ports;

pub use application::RunnerClaims;
pub use ports::{ClaimQueue, LeaseReplay, Specification};

/// A committed storage claim, with the original concrete records moved intact.
#[derive(Debug)]
pub struct Claimed<J, T> {
    pub job: J,
    pub task: T,
}

/// Keep the existing boxed specification outcome; no extra specification allocation.
#[derive(Debug)]
pub enum Claim<S> {
    Task(Box<S>),
    Nothing,
    /// An idempotency key was spent by a claimed job whose specification failed.
    Spent,
}

#[derive(Debug)]
pub enum Resumed<S> {
    Task(Box<S>),
    Spent,
    Unknown,
}
