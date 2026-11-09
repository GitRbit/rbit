//! Plain localized overview labels, prepared once per render.
use rbit_localization::{
    ActionsMessage, ActionsOverviewMessage, Catalog, Locale, Localized, OverviewControlDisplay, OverviewControlMessage,
};

use super::{
    StateLook,
    job_text::{LocalizedState, localized_state},
};

#[derive(Debug)]
pub struct OverviewText {
    catalog: Catalog,
    pub run_workflow: OverviewControlDisplay,
    pub workflow_from: OverviewControlDisplay,
    pub change: OverviewControlDisplay,
    pub actions: Localized,
    pub workflows: Localized,
    pub all_workflows: Localized,
    pub all_runs: Localized,
    pub filters: Localized,
    pub branch: Localized,
    pub actor: Localized,
    pub event: Localized,
    pub status: Localized,
    pub any_event: Localized,
    pub any_status: Localized,
    pub filter: Localized,
    pub clear: Localized,
    pub enable_workflow: Localized,
    pub disable_workflow: Localized,
    pub workflow_disabled: Localized,
    pub no_matching_runs: Localized,
    pub no_matching_runs_description: Localized,
    pub no_runs_yet: Localized,
    pub dispatch_empty_description: Localized,
    pub no_workflow_runs_yet: Localized,
    pub workflow_empty_description: Localized,
    pub pagination: Localized,
    pub older_runs: Localized,
    pub page_count: Localized,
}

impl OverviewText {
    /// Askama passes typed option fields by reference at the rendering boundary.
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "match Askama's borrowed enum field argument")]
    pub const fn option(&self, message: &OverviewControlMessage) -> OverviewControlDisplay {
        self.catalog.overview_control(*message)
    }

    /// A run's state icon labelled in the page's locale.
    pub fn look(&self, look: &StateLook) -> LocalizedState {
        localized_state(self.catalog, look)
    }

    /// Text that only exists in English, such as a relative timestamp.
    pub fn english(&self, text: &str) -> Localized {
        Localized::english_only(text.to_owned(), self.catalog.locale())
    }

    pub fn new(locale: Locale, count: usize) -> Self {
        let catalog = Catalog::new(locale);
        Self {
            catalog,
            run_workflow: catalog.overview_control(OverviewControlMessage::RunWorkflow),
            workflow_from: catalog.overview_control(OverviewControlMessage::WorkflowFrom),
            change: catalog.overview_control(OverviewControlMessage::Change),
            actions: catalog.localized(ActionsMessage::Actions),
            workflows: catalog.localized(ActionsOverviewMessage::Workflows),
            all_workflows: catalog.localized(ActionsOverviewMessage::AllWorkflows),
            all_runs: catalog.localized(ActionsOverviewMessage::AllRuns),
            filters: catalog.localized(ActionsOverviewMessage::Filters),
            branch: catalog.localized(ActionsOverviewMessage::Branch),
            actor: catalog.localized(ActionsOverviewMessage::Actor),
            event: catalog.localized(ActionsOverviewMessage::Event),
            status: catalog.localized(ActionsOverviewMessage::Status),
            any_event: catalog.localized(ActionsOverviewMessage::AnyEvent),
            any_status: catalog.localized(ActionsOverviewMessage::AnyStatus),
            filter: catalog.localized(ActionsOverviewMessage::Filter),
            clear: catalog.localized(ActionsOverviewMessage::Clear),
            enable_workflow: catalog.localized(ActionsOverviewMessage::EnableWorkflow),
            disable_workflow: catalog.localized(ActionsOverviewMessage::DisableWorkflow),
            workflow_disabled: catalog.localized(ActionsOverviewMessage::WorkflowDisabled),
            no_matching_runs: catalog.localized(ActionsOverviewMessage::NoMatchingRuns),
            no_matching_runs_description: catalog.localized(ActionsOverviewMessage::NoMatchingRunsDescription),
            no_runs_yet: catalog.localized(ActionsOverviewMessage::NoRunsYet),
            dispatch_empty_description: catalog.localized(ActionsOverviewMessage::DispatchEmptyDescription),
            no_workflow_runs_yet: catalog.localized(ActionsOverviewMessage::NoWorkflowRunsYet),
            workflow_empty_description: catalog.localized(ActionsOverviewMessage::WorkflowEmptyDescription),
            pagination: catalog.localized(ActionsOverviewMessage::Pagination),
            older_runs: catalog.localized(ActionsOverviewMessage::OlderRuns),
            page_count: catalog.localized(ActionsOverviewMessage::RunsOnPage { count: u64::try_from(count).unwrap_or(u64::MAX) }),
        }
    }
}
