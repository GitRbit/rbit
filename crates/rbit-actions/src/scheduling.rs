//! Run scheduling decisions (§11.6, §11.8 and §12.6 of the Actions design): what an event
//! may start, how a run enters its concurrency group, which held run starts next, how a
//! run starts again, and whether a runner still holds a task's lease. The atomic SQL that
//! creates runs, settles jobs, claims tasks and moves leases stays in storage and remains
//! the race authority; these decisions run under the composition's group lock.
//!
//! * An event starts at most `max_runs_per_event` runs and `max_jobs_per_event` jobs
//!   ([`Allowance`]); a repository may have at most `max_waiting_jobs_per_repository` jobs
//!   waiting, and a `runs-on` label is 1 to [`MAX_JOB_LABEL_CHARS`] characters. Past them a
//!   run becomes a workflow error with the reason ([`check_jobs`], [`check_labels`],
//!   [`check_waiting`]).
//! * A run awaiting approval neither enters nor holds a group. Entering one with
//!   `cancel-in-progress` cancels every other unfinished run of it; without, runs that were
//!   only held are cancelled (the newer replaces them) and the run is held while another is
//!   active ([`enter_group`]). A finished run releases the oldest held run once no other is
//!   active ([`next_to_release`]).
//! * Running a run again, or approving one, enters its group as a new run would; a rerun
//!   whose author would wait today waits for approval first and enters nothing.
//! * A runner holds a task's lease while the task is its own, still running, and its job's
//!   current attempt ([`leases`]); a stopped task's runner is told to cancel, and a closed
//!   log takes no more lines.
//! * After each settle the server decides the jobs left to it: their `if:`, a job planned
//!   from its needs, and fail-fast ([`jobs`]).
//! * People cancel, run again, approve and review within the rules of [`requests`].

#[cfg(test)]
mod decision_tests;
pub mod jobs;
pub mod leases;
pub mod requests;
#[cfg(test)]
mod tests;

/// Longest runner label a job may ask for, in characters.
pub const MAX_JOB_LABEL_CHARS: usize = 255;

/// What one event may still start (§11.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowance {
    pub runs: u32,
    pub jobs: u32,
}

impl Allowance {
    /// Takes one run, or `false` once the event started as many runs as it may.
    pub fn take_run(&mut self) -> bool {
        if self.runs == 0 {
            return false;
        }
        self.runs -= 1;
        true
    }
}

/// Takes a planned run's `jobs` from `allowance`; `Err` (the run's workflow error) when the
/// event already started its share. `max` is `max_jobs_per_event`.
pub fn check_jobs(path: &str, jobs: usize, allowance: &mut Allowance, max: u32) -> Result<(), String> {
    let jobs = u32::try_from(jobs).unwrap_or(u32::MAX);
    if jobs > allowance.jobs {
        return Err(format!("{path}: this event already started its share of jobs; one event may start at most {max}"));
    }
    allowance.jobs -= jobs;
    Ok(())
}

/// The first `runs-on` label that is empty or longer than [`MAX_JOB_LABEL_CHARS`], as the
/// run's workflow error.
pub fn check_labels<'a>(path: &str, labels: impl IntoIterator<Item = &'a str>) -> Result<(), String> {
    match labels.into_iter().find(|label| label.is_empty() || label.chars().count() > MAX_JOB_LABEL_CHARS) {
        Some(label) => {
            Err(format!("{path}: `runs-on` label `{}` must be 1 to {MAX_JOB_LABEL_CHARS} characters", truncate_chars(label, 64)))
        }
        None => Ok(()),
    }
}

/// Whether `starting` more waiting jobs fit beside the repository's `waiting` ones under
/// `max` (`max_waiting_jobs_per_repository`).
pub fn check_waiting(path: &str, waiting: u64, starting: u64, max: u64) -> Result<(), String> {
    if waiting + starting > max {
        return Err(format!("{path}: the repository already has {waiting} jobs waiting for a runner; at most {max} may wait"));
    }
    Ok(())
}

/// Where an unfinished run of a concurrency group stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Member {
    /// Waiting for a writer's approval: it holds no group.
    AwaitingApproval,
    /// Held while another run of the group is active.
    Held,
    /// Waiting for runners or running.
    Active,
}

/// How a run that becomes runnable starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Runnable,
    HeldForConcurrency,
}

/// What entering a group does: the runs to cancel, in order, and how the run starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<Id> {
    pub cancel: Vec<Id>,
    pub start: Start,
}

/// Enters a group whose unfinished runs are `others` (the run itself, `own`, is skipped).
pub fn enter_group<Id: Copy + PartialEq>(others: &[(Id, Member)], own: Option<Id>, cancel_in_progress: bool) -> Entry<Id> {
    let mut cancel = Vec::new();
    let mut busy = false;
    for &(id, member) in others {
        if Some(id) == own || (member == Member::AwaitingApproval && !cancel_in_progress) {
            continue;
        }
        if cancel_in_progress || member == Member::Held {
            cancel.push(id);
        } else {
            busy = true;
        }
    }
    Entry { cancel, start: if busy { Start::HeldForConcurrency } else { Start::Runnable } }
}

/// The held run to release once a run of the group finished: the oldest held one, and only
/// when no run of the group is active. `unfinished` is oldest first.
pub fn next_to_release<Id: Copy>(unfinished: &[(Id, Member)]) -> Option<Id> {
    if unfinished.iter().any(|&(_, member)| member == Member::Active) {
        return None;
    }
    unfinished.iter().find(|&&(_, member)| member == Member::Held).map(|&(id, _)| id)
}

/// `text` cut to at most `max` characters.
fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}
