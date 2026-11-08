//! The contribution calendar shared by profiles, organizations and repository activity.
use crate::catalog::fallback::{MessageFamily, MessageWriter};

labels! {
    /// Fixed text of the contribution calendar.
    ContributionLabel {
        CommitActivity => "contributions.label", "Commit activity";
        Loading => "contributions.loading", "Loading commit activity…";
        ViewCommitActivity => "contributions.view", "View commit activity";
        Year => "contributions.year", "Year";
        View => "contributions.submit", "View";
        AnnualCalendar => "contributions.annual", "Annual commit calendar";
        QuarterlyCalendar => "contributions.quarterly", "Commit calendar by quarter";
        Monday => "contributions.monday", "Mon";
        Wednesday => "contributions.wednesday", "Wed";
        Friday => "contributions.friday", "Fri";
        DatesUseUtc => "contributions.utc", "Dates use UTC.";
        IntensityLegend => "contributions.legend", "Commit intensity from less to more";
        Less => "contributions.less", "Less";
        More => "contributions.more", "More";
        Partial => "contributions.partial", "Partial activity: at least one repository exceeded the 100,000-commit history scan limit.";
        NoCommits => "contributions.none", "No matching commits in this year.";
        SelectDate => "contributions.select", "Select a date to see its commit activity.";
        Commits => "contributions.commits", "Commits";
        ActiveDays => "contributions.active-days", "Active days";
        LongestStreak => "contributions.longest-streak", "Longest streak";
        Repositories => "contributions.repositories", "Repositories";
        HowCounted => "contributions.how-title", "Monthly activity and how commits are counted";
        HowCountedText => "contributions.how", "Counts include commits reachable from the configured default branch, attributed to their author date. Identical commits shared by repositories count once. Only repositories you can access are included. Issues, pull requests, and reviews are not included.";
    }
}

/// Counted text of the contribution calendar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContributionMessage<'a> {
    /// The calendar's heading: "12 commits in 2026".
    CommitsInYear { count: usize, year: i32 },
    /// The calendar's title: "Commit activity in 2026".
    ActivityInYear { year: i32 },
    /// The calendar's description for assistive technology.
    Description { count: usize, days: usize },
    /// One day: "3 commits on 2026-05-01".
    Day { count: usize, date: &'a str },
    /// A quarter's calendar: "Q1 2026 commit activity".
    QuarterActivity { quarter: &'a str, year: i32 },
    /// The longest streak: "4 days".
    Days { count: usize },
}

impl MessageFamily for ContributionMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::CommitsInYear { .. } => "contributions.title",
            Self::ActivityInYear { .. } => "contributions.calendar-title",
            Self::Description { .. } => "contributions.description",
            Self::Day { .. } => "contributions.day",
            Self::QuarterActivity { .. } => "contributions.quarter",
            Self::Days { .. } => "contributions.days",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::CommitsInYear { count, year } => {
                commits(text, count);
                text.literal(" in ");
                text.argument(&year.to_string());
            }
            Self::ActivityInYear { year } => {
                text.literal("Commit activity in ");
                text.argument(&year.to_string());
            }
            Self::Description { count, days } => {
                commits(text, count);
                text.literal(" across ");
                text.number(days as u64);
                text.literal(if days == 1 { " active day" } else { " active days" });
                text.literal(
                    ". Higher intensity indicates more commits. Focus or hover over a day for its count. Select a day for details.",
                );
            }
            Self::Day { count, date } => {
                commits(text, count);
                text.literal(" on ");
                text.argument(date);
            }
            Self::QuarterActivity { quarter, year } => {
                text.argument(quarter);
                text.literal(" ");
                text.argument(&year.to_string());
                text.literal(" commit activity");
            }
            Self::Days { count } => {
                text.number(count as u64);
                text.literal(if count == 1 { " day" } else { " days" });
            }
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

/// "1 commit", "3 commits".
fn commits(text: &mut MessageWriter, count: usize) {
    text.number(count as u64);
    text.literal(if count == 1 { " commit" } else { " commits" });
}

lookup!(ContributionMessage<'_>);
