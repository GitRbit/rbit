//! Plain localized overview labels, prepared once per render.
use rbit_localization::{
    ActionsMessage, ActionsOverviewMessage, Catalog, Locale, LocalizedText, OverviewControlDisplay, OverviewControlMessage,
};

#[derive(Debug)]
pub struct OverviewText {
    catalog: Catalog,
    pub run_workflow: OverviewControlDisplay,
    pub workflow_from: OverviewControlDisplay,
    pub change: OverviewControlDisplay,
    pub actions: LocalizedText,
    pub workflows: LocalizedText,
    pub all_workflows: LocalizedText,
    pub all_runs: LocalizedText,
    pub filters: LocalizedText,
    pub branch: LocalizedText,
    pub actor: LocalizedText,
    pub event: LocalizedText,
    pub status: LocalizedText,
    pub any_event: LocalizedText,
    pub any_status: LocalizedText,
    pub filter: LocalizedText,
    pub clear: LocalizedText,
    pub enable_workflow: LocalizedText,
    pub disable_workflow: LocalizedText,
    pub workflow_disabled: LocalizedText,
    pub no_matching_runs: LocalizedText,
    pub no_matching_runs_description: LocalizedText,
    pub no_runs_yet: LocalizedText,
    pub dispatch_empty_description: LocalizedText,
    pub no_workflow_runs_yet: LocalizedText,
    pub workflow_empty_description: LocalizedText,
    pub pagination: LocalizedText,
    pub older_runs: LocalizedText,
    pub page_count: LocalizedText,
}

impl OverviewText {
    /// Askama passes typed option fields by reference at the rendering boundary.
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "match Askama's borrowed enum field argument")]
    pub const fn option(&self, message: &OverviewControlMessage) -> OverviewControlDisplay {
        self.catalog.overview_control(*message)
    }

    pub fn new(locale: Locale, count: usize) -> Self {
        let catalog = Catalog::new(locale);
        Self {
            catalog,
            run_workflow: catalog.overview_control(OverviewControlMessage::RunWorkflow),
            workflow_from: catalog.overview_control(OverviewControlMessage::WorkflowFrom),
            change: catalog.overview_control(OverviewControlMessage::Change),
            actions: catalog.actions(ActionsMessage::Actions),
            workflows: catalog.overview(ActionsOverviewMessage::Workflows),
            all_workflows: catalog.overview(ActionsOverviewMessage::AllWorkflows),
            all_runs: catalog.overview(ActionsOverviewMessage::AllRuns),
            filters: catalog.overview(ActionsOverviewMessage::Filters),
            branch: catalog.overview(ActionsOverviewMessage::Branch),
            actor: catalog.overview(ActionsOverviewMessage::Actor),
            event: catalog.overview(ActionsOverviewMessage::Event),
            status: catalog.overview(ActionsOverviewMessage::Status),
            any_event: catalog.overview(ActionsOverviewMessage::AnyEvent),
            any_status: catalog.overview(ActionsOverviewMessage::AnyStatus),
            filter: catalog.overview(ActionsOverviewMessage::Filter),
            clear: catalog.overview(ActionsOverviewMessage::Clear),
            enable_workflow: catalog.overview(ActionsOverviewMessage::EnableWorkflow),
            disable_workflow: catalog.overview(ActionsOverviewMessage::DisableWorkflow),
            workflow_disabled: catalog.overview(ActionsOverviewMessage::WorkflowDisabled),
            no_matching_runs: catalog.overview(ActionsOverviewMessage::NoMatchingRuns),
            no_matching_runs_description: catalog.overview(ActionsOverviewMessage::NoMatchingRunsDescription),
            no_runs_yet: catalog.overview(ActionsOverviewMessage::NoRunsYet),
            dispatch_empty_description: catalog.overview(ActionsOverviewMessage::DispatchEmptyDescription),
            no_workflow_runs_yet: catalog.overview(ActionsOverviewMessage::NoWorkflowRunsYet),
            workflow_empty_description: catalog.overview(ActionsOverviewMessage::WorkflowEmptyDescription),
            pagination: catalog.overview(ActionsOverviewMessage::Pagination),
            older_runs: catalog.overview(ActionsOverviewMessage::OlderRuns),
            page_count: catalog.overview(ActionsOverviewMessage::RunsOnPage { count: u64::try_from(count).unwrap_or(u64::MAX) }),
        }
    }
}
