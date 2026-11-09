//! Typed text for the Actions overview; machine filter values never use translations.
use super::fallback::{MessageFamily, MessageWriter};
use crate::Locale;

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

impl ActionsOverviewMessage {
    /// The English text of a fixed label.
    const fn english_label(self) -> Option<&'static str> {
        Some(match self {
            Self::Workflows => "Workflows",
            Self::AllWorkflows => "All workflows",
            Self::AllRuns => "All workflow runs",
            Self::Filters => "Workflow run filters",
            Self::Branch => "Branch",
            Self::Actor => "Actor",
            Self::Event => "Event",
            Self::Status => "Status",
            Self::AnyEvent => "Any event",
            Self::AnyStatus => "Any status",
            Self::Filter => "Filter",
            Self::Clear => "Clear",
            Self::EnableWorkflow => "Enable workflow",
            Self::DisableWorkflow => "Disable workflow",
            Self::WorkflowDisabled => "This workflow is disabled: it starts no run, for any event, until it is enabled.",
            Self::NoMatchingRuns => "No matching runs",
            Self::NoMatchingRunsDescription => "No workflow run matches these filters.",
            Self::NoRunsYet => "No runs yet",
            Self::DispatchEmptyDescription => "This workflow runs when it is started: use Run workflow above.",
            Self::NoWorkflowRunsYet => "No workflow runs yet",
            Self::WorkflowEmptyDescription => {
                "Add a workflow file under .rbit/workflows and push it: runs show up here as runners pick them up."
            }
            Self::Pagination => "Pagination",
            Self::OlderRuns => "Older runs",
            Self::RunsOnPage { .. } => return None,
        })
    }
}

impl MessageFamily for ActionsOverviewMessage {
    fn id(self) -> &'static str {
        Self::id(self)
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::RunsOnPage { count } => {
                text.number(count);
                text.literal(if count == 1 { " run on this page" } else { " runs on this page" });
            }
            _ => text.literal(self.english_label().unwrap_or_default()),
        }
    }

    fn english_literal(self) -> Option<&'static str> {
        self.english_label()
    }

    fn translate(self, locale: Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;

    #[test]
    fn overview_counts_describe_only_the_current_page_and_keep_english_plural_rules() {
        let catalog = Catalog::new(Locale::English);
        for (count, expected) in [(0, "0 runs on this page"), (1, "1 run on this page"), (27, "27 runs on this page")] {
            assert_eq!(catalog.localized(ActionsOverviewMessage::RunsOnPage { count }).as_str(), expected);
        }
        let expanded = Catalog::new(Locale::PseudoExpanded).localized(ActionsOverviewMessage::RunsOnPage { count: 27 });
        assert!(expanded.as_str().starts_with("[27"), "diagnostic text is bracketed like every family");
        assert_ne!(expanded.as_str(), "27 runs on this page");
        assert_eq!(ActionsOverviewMessage::Branch.id(), "actions.overview.branch");
        assert_eq!(catalog.localized(ActionsOverviewMessage::OlderRuns).as_str(), "Older runs");
    }
}
