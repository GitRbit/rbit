//! What the run page's summary says about how a run started and how far it is, beyond its
//! heading: the pull request or schedule behind it, a manual run's inputs, who started a
//! later attempt, the ticking total duration and how many artifacts it uploaded.
use std::collections::BTreeMap;

use rbit_core::{Account, AccountId, actions::Run};
use rbit_services::actions::trigger_details::TriggerDetails;

use super::name_of;

/// The run's trigger and progress details.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TriggerView {
    /// The pull request a run is for: its number and page.
    pub pull: Option<(i64, String)>,
    /// A scheduled run's cron expression.
    pub schedule: Option<String>,
    /// A manual run's inputs, by name.
    pub inputs: Vec<(String, String)>,
    /// Inputs not listed.
    pub more_inputs: u64,
    /// Who started this attempt, when it is a later one by someone else than the first.
    pub rerun_by: Option<String>,
    /// While the run runs: when it started, in Unix seconds (the total duration ticks).
    pub since: Option<i64>,
    pub artifacts: usize,
}

impl TriggerView {
    /// `home` is the repository's page (`/{owner}/{repo}`).
    pub fn new(run: &Run, people: &BTreeMap<AccountId, Account>, details: TriggerDetails, home: &str, artifacts: usize) -> Self {
        let rerun_by = (run.attempt > 1 && run.trigger_actor.is_some() && run.trigger_actor != run.actor)
            .then(|| name_of(people, run.trigger_actor))
            .flatten();
        Self {
            pull: run.pull_number.map(|number| (number, format!("{home}/pull/{number}"))),
            schedule: details.schedule,
            inputs: details.inputs,
            more_inputs: u64::try_from(details.more_inputs).unwrap_or(u64::MAX),
            rerun_by,
            since: (!run.status.is_done()).then_some(run.started_at).flatten().map(rbit_core::Timestamp::seconds),
            artifacts,
        }
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{Timestamp, actions::Status};

    use super::*;
    use crate::views::actions::run_layout::tests::run;

    #[test]
    fn later_attempts_by_someone_else_say_who_and_running_runs_tick() {
        let mut people = BTreeMap::new();
        for (id, name) in [(1, "alice"), (2, "bob")] {
            let account = Account {
                id: AccountId(id),
                kind: rbit_core::AccountKind::User,
                name: name.parse().unwrap(),
                display_name: None,
                bio: None,
                created_at: Timestamp(0),
            };
            people.insert(AccountId(id), account);
        }
        let mut rerun = run(Status::Running);
        (rerun.actor, rerun.trigger_actor, rerun.attempt, rerun.pull_number) = (Some(AccountId(1)), Some(AccountId(2)), 2, Some(12));
        rerun.started_at = Some(Timestamp(100));
        let details = TriggerDetails { schedule: None, inputs: vec![("level".into(), "debug".into())], more_inputs: 3 };
        let view = TriggerView::new(&rerun, &people, details, "/a/b", 2);
        assert_eq!(view.rerun_by.as_deref(), Some("bob"));
        assert_eq!(view.pull, Some((12, "/a/b/pull/12".to_owned())));
        assert_eq!((view.since, view.more_inputs, view.artifacts), (Some(100), 3, 2));
        let first = Run { attempt: 1, status: Status::Success, ..rerun };
        let view = TriggerView::new(&first, &people, TriggerDetails::default(), "/a/b", 0);
        assert_eq!((view.rerun_by, view.since), (None, None), "the first attempt, finished");
    }
}
