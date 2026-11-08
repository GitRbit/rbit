//! Existing Actions overview rows, filter choices, workflow navigation and polling policy.
use super::{ActionsUrls, DispatchFormView, StateLook, elapsed, event_label, event_message, name_of, short_ref};
use crate::views::{layout::Page, repo::RepoHeader};
use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    Account, AccountId,
    actions::{Run, RunEvent, Status},
};
use rbit_localization::OverviewControlMessage as Control;
use std::collections::BTreeMap;

/// One row of the runs list.
#[derive(Debug, Clone)]
pub struct RunRow {
    pub url: String,
    pub look: StateLook,
    pub title: String,
    pub number: i64,
    pub workflow: String,
    pub event: &'static str,
    /// The event in the page's language.
    pub event_message: Control,
    pub reference: String,
    pub actor: Option<String>,
    pub when: String,
    pub when_iso: String,
    pub duration: Option<String>,
    /// Why the workflow has no jobs, for workflow-error runs.
    pub error: Option<String>,
}

impl RunRow {
    pub fn new(run: &Run, people: &BTreeMap<AccountId, Account>, urls: &ActionsUrls) -> Self {
        Self {
            url: urls.run(run.number),
            look: StateLook::of_run(run),
            title: run.title.clone(),
            number: run.number,
            workflow: run.workflow_name.clone(),
            event: event_label(run.event),
            event_message: event_message(run.event),
            reference: short_ref(&run.reference),
            actor: name_of(people, run.trigger_actor.or(run.actor)),
            when: run.created_at.relative(),
            when_iso: run.created_at.iso8601(),
            duration: if run.status.is_done() || run.status == Status::Running { elapsed(run.started_at, run.finished_at) } else { None },
            error: run.error.clone(),
        }
    }
}

/// A workflow in the sidebar.
#[derive(Debug, Clone)]
pub struct WorkflowLink {
    pub name: String,
    pub file: String,
    pub url: String,
    pub is_current: bool,
}

/// A choice in a filter `<select>`.
#[derive(Debug, Clone)]
pub struct FilterOption {
    pub value: &'static str,
    pub label: Control,
    pub selected: bool,
}

/// The filters as the form shows them.
#[derive(Debug, Clone, Default)]
pub struct RunFilters {
    /// The list without the workflow filter (the form's target).
    pub action: String,
    pub branch: String,
    pub actor: String,
    pub events: Vec<FilterOption>,
    pub statuses: Vec<FilterOption>,
    /// Whether any filter is set ("Clear" link).
    pub any: bool,
}

impl RunFilters {
    pub fn new(action: String, branch: &str, actor: &str, event: Option<RunEvent>, status: Option<Status>) -> Self {
        let events = RunEvent::ALL
            .into_iter()
            .map(|option| FilterOption { value: option.as_str(), label: event_message(option), selected: event == Some(option) })
            .collect();
        let statuses = Status::ALL
            .into_iter()
            .map(|option| FilterOption {
                value: option.as_str(),
                label: match option {
                    Status::Success => Control::Success,
                    Status::Failure => Control::Failure,
                    Status::Cancelled => Control::Cancelled,
                    Status::Skipped => Control::Skipped,
                    Status::Waiting => Control::Queued,
                    Status::Running => Control::Running,
                    Status::Blocked => Control::Waiting,
                    Status::AwaitingApproval => Control::Approval,
                },
                selected: status == Some(option),
            })
            .collect();
        let any = !branch.is_empty() || !actor.is_empty() || event.is_some() || status.is_some();
        Self { action, branch: branch.to_owned(), actor: actor.to_owned(), events, statuses, any }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/runs.html")]
pub struct RunsPage {
    pub page: Page,
    pub header: RepoHeader,
    pub runs: Vec<RunRow>,
    pub workflows: Vec<WorkflowLink>,
    /// The list of every workflow's runs.
    pub all_url: String,
    /// The selected workflow's name, when the list is filtered by one.
    pub workflow: Option<String>,
    pub filters: RunFilters,
    /// Older runs, when there are more.
    pub next_url: Option<String>,
    // Actions: dispatch form.
    /// "Run workflow", on a `workflow_dispatch` workflow's page for writers.
    pub dispatch: Option<DispatchFormView>,
    // Actions: disabled workflows.
    /// The selected workflow is disabled: it starts no run.
    pub disabled: bool,
    /// Where writers disable or enable the selected workflow.
    pub switch_url: Option<String>,
    /// How often the list refreshes itself (an htmx trigger), when it does.
    pub refresh: Option<&'static str>,
    /// This list, with its filters and page.
    pub self_url: String,
}

/// How often a runs list refreshes itself: often while a run on it is not finished, else
/// now and then on the newest page only, where new runs appear.
pub fn runs_refresh(runs: &[Run], newest: bool) -> Option<&'static str> {
    if runs.iter().any(|run| !run.status.is_done()) {
        Some("every 3s")
    } else if newest {
        Some("every 10s")
    } else {
        None
    }
}

impl RunsPage {
    pub fn labels(&self) -> super::OverviewText {
        super::OverviewText::new(self.page.locale, self.runs.len())
    }
}

#[cfg(test)]
#[path = "overview_controls_tests.rs"]
mod controls_tests;
