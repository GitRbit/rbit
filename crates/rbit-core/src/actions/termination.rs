//! How a job's task ended: the termination record an agent reads instead of a log.
//!
//! The runner reports what it saw (`completed`, `step_failed`, `timeout`, `cancelled`,
//! `setup_failed`, with the step and its exit code); the server records what only it knows
//! (`runner_lost`, `superseded`, `server_shutdown`). A task with no record reads by its
//! status: `completed`, `cancelled`, or `unknown` for a failure nobody explained (an older
//! runner). A record whose log is not complete is never a pass.

use std::fmt;

/// Why a task ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminationReason {
    Completed,
    StepFailed,
    Timeout,
    Cancelled,
    Superseded,
    RunnerLost,
    SetupFailed,
    ServerShutdown,
    Unknown,
}

impl TerminationReason {
    pub const ALL: [Self; 9] = [
        Self::Completed,
        Self::StepFailed,
        Self::Timeout,
        Self::Cancelled,
        Self::Superseded,
        Self::RunnerLost,
        Self::SetupFailed,
        Self::ServerShutdown,
        Self::Unknown,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::StepFailed => "step_failed",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::Superseded => "superseded",
            Self::RunnerLost => "runner_lost",
            Self::SetupFailed => "setup_failed",
            Self::ServerShutdown => "server_shutdown",
            Self::Unknown => "unknown",
        }
    }

    /// The reason a word names; `Unknown` for anything else (a newer runner's word).
    pub fn parse(word: &str) -> Self {
        Self::ALL.into_iter().find(|reason| reason.as_str() == word).unwrap_or(Self::Unknown)
    }
}

impl fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A task's termination record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Termination {
    pub reason: TerminationReason,
    /// The index of the step that ended the job, in the task's steps.
    pub step: Option<u32>,
    /// That step's exit code, when it was a process that exited.
    pub exit_code: Option<i32>,
}

impl Termination {
    pub const fn because(reason: TerminationReason) -> Self {
        Self { reason, step: None, exit_code: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reason_reads_back_and_strangers_are_unknown() {
        for reason in TerminationReason::ALL {
            assert_eq!(TerminationReason::parse(reason.as_str()), reason);
        }
        assert_eq!(TerminationReason::parse("exploded"), TerminationReason::Unknown);
        assert_eq!(Termination::because(TerminationReason::RunnerLost).step, None);
    }
}
