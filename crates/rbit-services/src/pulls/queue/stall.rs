//! When a queued candidate's required checks can no longer pass, so its entry leaves the
//! line instead of holding up every entry behind it.

use std::time::Duration;

use rbit_core::{Timestamp, actions::RunEvent};
use rbit_storage::sqlite::actions_runs::{QueueEntry, RunFilter};

use crate::{ServiceError, pulls::PullService};

/// How long a candidate may wait for its required checks once it is built: GitHub's
/// default for its merge queue.
pub const CHECK_TIMEOUT: Duration = Duration::from_hours(1);

/// The event of a context named as rbit's Actions name their statuses,
/// `<workflow> / <job> (<event>)`; `None` for any other name (an outside CI system's).
fn actions_event(context: &str) -> Option<RunEvent> {
    let inner = context.strip_suffix(')')?;
    let open = inner.rfind(" (")?;
    if !inner[..open].contains(" / ") {
        return None;
    }
    RunEvent::parse(&inner[open + 2..])
}

/// What the candidate's own Actions runs can still do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runs {
    /// Its push has not been decided yet, or some of its runs have not finished.
    Going,
    /// Its push started no run (no workflow, or none for `merge-queue/…` branches).
    NoneStarted,
    /// Every run it started has finished.
    Finished,
}

/// Why the candidate cannot pass, from the required contexts that have not passed
/// (`waiting`), those of them with no status at all (`missing`), what its runs can still
/// do and how long it has waited; `None` while it may still pass.
fn verdict(waiting: &[&str], missing: &[&str], runs: Runs, waited: Duration) -> Option<String> {
    if let Some((context, event)) =
        missing.iter().find_map(|context| actions_event(context).filter(|event| *event != RunEvent::Push).map(|event| (context, event)))
    {
        return Some(format!(
            "`{context}` is reported by {event} runs, and the candidate is tested by a push, so it never will be: require the push run's check instead"
        ));
    }
    if let Some(context) = missing.iter().find(|context| actions_event(context) == Some(RunEvent::Push)) {
        match runs {
            Runs::NoneStarted => {
                return Some(format!(
                    "no workflow ran on the candidate, so `{context}` never will: workflows limited to some branches do not run on `merge-queue/…`"
                ));
            }
            Runs::Finished => return Some(format!("the candidate's runs have finished and none reported `{context}`")),
            Runs::Going => {}
        }
    }
    (waited >= CHECK_TIMEOUT).then(|| {
        let context = waiting.first().copied().unwrap_or_default();
        format!("`{context}` did not pass on the candidate within {} minutes", CHECK_TIMEOUT.as_secs() / 60)
    })
}

impl PullService {
    /// Why `entry`'s candidate cannot pass while the required contexts `waiting` have not
    /// (`missing` of them reported nothing yet); `None` to keep waiting.
    pub(super) async fn stalled(
        &self,
        entry: &QueueEntry,
        candidate: &str,
        waiting: &[&str],
        missing: &[&str],
    ) -> Result<Option<String>, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let runs = match store.decision(entry.repo, candidate).await? {
            None => Runs::Going,
            Some(decision) if decision.started == 0 => Runs::NoneStarted,
            Some(_) => {
                let filter = RunFilter { sha: Some(candidate), limit: 100, ..RunFilter::default() };
                let runs = store.list_runs(entry.repo, &filter).await?;
                if !runs.is_empty() && runs.iter().all(|run| run.status.is_done()) { Runs::Finished } else { Runs::Going }
            }
        };
        let waited = Duration::from_secs(u64::try_from(Timestamp::now().0 - entry.updated_at.0).unwrap_or_default());
        Ok(verdict(waiting, missing, runs, waited))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_mins(1);

    #[test]
    fn contexts_of_rbit_actions_name_their_event() {
        assert_eq!(actions_event("CI / test (pull_request)"), Some(RunEvent::PullRequest));
        assert_eq!(actions_event("CI / build (linux) (push)"), Some(RunEvent::Push), "a matrix leg's name has parentheses too");
        assert_eq!(actions_event("ci/test"), None);
        assert_eq!(actions_event("Jenkins (nightly)"), None, "no workflow and job");
        assert_eq!(actions_event("CI / test (merge_group)"), None, "an event rbit does not run");
    }

    #[test]
    fn a_check_only_another_event_reports_fails_at_once() {
        let missing = ["CI / test (pull_request)"];
        let reason = verdict(&missing, &missing, Runs::Going, Duration::ZERO).unwrap();
        assert!(reason.contains("pull_request runs") && reason.contains("push"), "{reason}");
        // Once reported (pending or not), it is the run's own business.
        assert_eq!(verdict(&missing, &[], Runs::Going, Duration::ZERO), None);
    }

    #[test]
    fn a_push_check_fails_once_no_run_can_report_it() {
        let missing = ["CI / test (push)"];
        assert_eq!(verdict(&missing, &missing, Runs::Going, 59 * MINUTE), None, "runs still going may report it");
        let none = verdict(&missing, &missing, Runs::NoneStarted, Duration::ZERO).unwrap();
        assert!(none.contains("no workflow ran"), "{none}");
        let finished = verdict(&missing, &missing, Runs::Finished, Duration::ZERO).unwrap();
        assert!(finished.contains("none reported `CI / test (push)`"), "{finished}");
    }

    #[test]
    fn any_check_times_out_after_an_hour() {
        // An outside system's context, or one still pending, may report late: it gets the hour.
        for (waiting, missing) in [(["ci/test"], &["ci/test"][..]), (["CI / test (push)"], &[][..])] {
            assert_eq!(verdict(&waiting, missing, Runs::Finished, 59 * MINUTE), None);
            let reason = verdict(&waiting, missing, Runs::Finished, 60 * MINUTE).unwrap();
            assert!(reason.contains("within 60 minutes"), "{reason}");
        }
    }
}
