//! Typed text for the Actions overview; machine filter values never use translations.
use super::{Catalog, LocalizedText};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionsOverviewMessage {
    Workflows,
    AllWorkflows,
    AllRuns,
    Filters,
    Branch,
    Actor,
    Event,
    Status,
    AnyEvent,
    AnyStatus,
    Filter,
    Clear,
    EnableWorkflow,
    DisableWorkflow,
    WorkflowDisabled,
    NoMatchingRuns,
    NoMatchingRunsDescription,
    NoRunsYet,
    DispatchEmptyDescription,
    NoWorkflowRunsYet,
    WorkflowEmptyDescription,
    Pagination,
    OlderRuns,
    RunsOnPage { count: u64 },
}

impl ActionsOverviewMessage {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Workflows => "actions.overview.workflows",
            Self::AllWorkflows => "actions.overview.all-workflows",
            Self::AllRuns => "actions.overview.all-runs",
            Self::Filters => "actions.overview.filters",
            Self::Branch => "actions.overview.branch",
            Self::Actor => "actions.overview.actor",
            Self::Event => "actions.overview.event",
            Self::Status => "actions.overview.status",
            Self::AnyEvent => "actions.overview.any-event",
            Self::AnyStatus => "actions.overview.any-status",
            Self::Filter => "actions.overview.filter",
            Self::Clear => "actions.overview.clear",
            Self::EnableWorkflow => "actions.overview.enable",
            Self::DisableWorkflow => "actions.overview.disable",
            Self::WorkflowDisabled => "actions.overview.disabled",
            Self::NoMatchingRuns => "actions.overview.no-matches",
            Self::NoMatchingRunsDescription => "actions.overview.no-matches-description",
            Self::NoRunsYet => "actions.overview.no-runs",
            Self::DispatchEmptyDescription => "actions.overview.dispatch-empty-description",
            Self::NoWorkflowRunsYet => "actions.overview.empty",
            Self::WorkflowEmptyDescription => "actions.overview.empty-description",
            Self::Pagination => "actions.overview.pagination",
            Self::OlderRuns => "actions.overview.older-runs",
            Self::RunsOnPage { .. } => "actions.overview.runs-on-page",
        }
    }
}

impl Catalog {
    pub fn overview(self, message: ActionsOverviewMessage) -> LocalizedText {
        let literal = match message {
            ActionsOverviewMessage::Workflows => "Workflows",
            ActionsOverviewMessage::AllWorkflows => "All workflows",
            ActionsOverviewMessage::AllRuns => "All workflow runs",
            ActionsOverviewMessage::Filters => "Workflow run filters",
            ActionsOverviewMessage::Branch => "Branch",
            ActionsOverviewMessage::Actor => "Actor",
            ActionsOverviewMessage::Event => "Event",
            ActionsOverviewMessage::Status => "Status",
            ActionsOverviewMessage::AnyEvent => "Any event",
            ActionsOverviewMessage::AnyStatus => "Any status",
            ActionsOverviewMessage::Filter => "Filter",
            ActionsOverviewMessage::Clear => "Clear",
            ActionsOverviewMessage::EnableWorkflow => "Enable workflow",
            ActionsOverviewMessage::DisableWorkflow => "Disable workflow",
            ActionsOverviewMessage::WorkflowDisabled => "This workflow is disabled: it starts no run, for any event, until it is enabled.",
            ActionsOverviewMessage::NoMatchingRuns => "No matching runs",
            ActionsOverviewMessage::NoMatchingRunsDescription => "No workflow run matches these filters.",
            ActionsOverviewMessage::NoRunsYet => "No runs yet",
            ActionsOverviewMessage::DispatchEmptyDescription => "This workflow runs when it is started: use Run workflow above.",
            ActionsOverviewMessage::NoWorkflowRunsYet => "No workflow runs yet",
            ActionsOverviewMessage::WorkflowEmptyDescription => {
                "Add a workflow file under .rbit/workflows and push it: runs show up here as runners pick them up."
            }
            ActionsOverviewMessage::Pagination => "Pagination",
            ActionsOverviewMessage::OlderRuns => "Older runs",
            ActionsOverviewMessage::RunsOnPage { count } => {
                let mut output = count.to_string();
                self.literal(&mut output, if count == 1 { " run on this page" } else { " runs on this page" });
                return LocalizedText(output.into());
            }
        };
        self.literal_text(literal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Locale;

    #[test]
    fn overview_counts_describe_only_the_current_page_and_keep_english_plural_rules() {
        let catalog = Catalog::new(Locale::English);
        for (count, expected) in [(0, "0 runs on this page"), (1, "1 run on this page"), (27, "27 runs on this page")] {
            assert_eq!(catalog.overview(ActionsOverviewMessage::RunsOnPage { count }).as_str(), expected);
        }
        let expanded = Catalog::new(Locale::PseudoExpanded).overview(ActionsOverviewMessage::RunsOnPage { count: 27 });
        assert!(expanded.as_str().starts_with("27"));
        assert_ne!(expanded.as_str(), "27 runs on this page");
        assert_eq!(ActionsOverviewMessage::Branch.id(), "actions.overview.branch");
        assert_eq!(catalog.overview(ActionsOverviewMessage::OlderRuns).as_str(), "Older runs");
    }
}
