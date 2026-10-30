//! Runtime primitives for safely retiring a plugin generation.
//!
//! [`Admission`] serializes new operation admission with closing the generation. An
//! [`OperationLease`] remains alive for the entire operation, including a streamed
//! response's body. Closing prevents new work; draining waits for admitted work to end.
//!
//! [`PluginScope`] additionally cancels tracked asynchronous tasks. Cancellation is
//! cooperative: a task that ignores its token keeps disposal incomplete, and a timeout
//! is reported rather than claiming the generation was unloaded. Blocking work and
//! arbitrary native code cannot be forcibly stopped by these primitives.

mod admission;
mod scope;

pub use admission::{Admission, AdmissionError, DrainError, OperationLease};
pub use scope::PluginScope;

#[cfg(test)]
mod tests;
