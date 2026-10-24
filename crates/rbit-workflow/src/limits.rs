//! Bounds on the work one workflow file may cause (§6.5 of the Actions design), and the
//! cooperative deadline that stops parsing and planning when evaluation runs out of time.

use std::time::{Duration, Instant};

use crate::error::{ErrorKind, WorkflowError};

/// What one workflow may cost. The defaults are the design's limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Largest file, in bytes (`[actions] max_workflow_bytes`).
    pub max_bytes: usize,
    /// Deepest YAML nesting.
    pub max_depth: usize,
    /// Most YAML nodes, counting those replayed by aliases.
    pub max_nodes: usize,
    /// Most anchors and most aliases.
    pub max_aliases: usize,
    /// Most jobs in one workflow.
    pub max_jobs: usize,
    /// Most matrix combinations of one job.
    pub max_combinations: usize,
    /// Most jobs one run may have after matrix expansion.
    pub max_run_jobs: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024,
            max_depth: 64,
            max_nodes: 100_000,
            max_aliases: 1_000,
            max_jobs: 256,
            max_combinations: 256,
            max_run_jobs: 1_024,
        }
    }
}

/// A point in time after which parsing and planning give up with [`ErrorKind::Timeout`].
/// Checked between units of work (jobs, matrix combinations), so a slow evaluation stops
/// itself instead of only being abandoned by its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline(Option<Instant>);

impl Deadline {
    /// No deadline (tests, and callers that bound the work otherwise).
    pub const NONE: Self = Self(None);

    pub fn after(budget: Duration) -> Self {
        Self(Instant::now().checked_add(budget))
    }

    pub fn at(instant: Instant) -> Self {
        Self(Some(instant))
    }

    pub fn is_expired(&self) -> bool {
        self.0.is_some_and(|deadline| Instant::now() >= deadline)
    }

    pub fn check(&self) -> Result<(), WorkflowError> {
        if self.is_expired() { Err(WorkflowError::new(ErrorKind::Timeout, "evaluating the workflow took too long")) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadlines() {
        assert!(Deadline::NONE.check().is_ok());
        assert!(Deadline::after(Duration::from_hours(1)).check().is_ok());
        let past = Deadline::at(Instant::now().checked_sub(Duration::from_secs(1)).unwrap());
        assert_eq!(past.check().unwrap_err().kind, ErrorKind::Timeout);
    }
}
