//! A repository's labels page.
use super::super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LabelMessage<'a> {
    NewLabel,
    Name,
    Colour,
    CreateLabel,
    /// The heading: how many labels the repository has.
    Count {
        count: u64,
    },
    EmptyTitle,
    EmptyText,
    CreateDefaults,
    OpenIssues {
        count: u64,
    },
    DeleteConfirm {
        name: &'a str,
    },
}

impl MessageFamily for LabelMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::NewLabel => "labels.new",
            Self::Name => "labels.form.name",
            Self::Colour => "labels.form.colour",
            Self::CreateLabel => "labels.form.create",
            Self::Count { .. } => "labels.count",
            Self::EmptyTitle => "labels.empty-title",
            Self::EmptyText => "labels.empty-text",
            Self::CreateDefaults => "labels.create-defaults",
            Self::OpenIssues { .. } => "labels.open-issues",
            Self::DeleteConfirm { .. } => "labels.delete-confirm",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::NewLabel => text.literal("New label"),
            Self::Name => text.literal("Name"),
            Self::Colour => text.literal("Colour"),
            Self::CreateLabel => text.literal("Create label"),
            Self::Count { count } => {
                text.number(count);
                text.literal(if count == 1 { " label" } else { " labels" });
            }
            Self::EmptyTitle => text.literal("No labels yet"),
            Self::EmptyText => text.literal("Labels sort issues and pull requests: bug, enhancement, good first issue…"),
            Self::CreateDefaults => text.literal("Create default labels"),
            Self::OpenIssues { count } => {
                text.number(count);
                text.literal(if count == 1 { " open issue" } else { " open issues" });
            }
            Self::DeleteConfirm { name } => {
                text.literal("Delete the label “");
                text.argument(name);
                text.literal("”? It is removed from every issue.");
            }
        }
    }
}
