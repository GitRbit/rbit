//! German (`de`) entries for `catalog/repo/contributions.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::contributions::{ContributionLabel, ContributionMessage};
use crate::plural::Plural;

entries! { De:
    ContributionLabel => MessageWriter |text| {
        ContributionLabel::CommitActivity => text.literal("Commit-Aktivität"),
        ContributionLabel::Loading => text.literal("Commit-Aktivität wird geladen…"),
        ContributionLabel::ViewCommitActivity => text.literal("Commit-Aktivität anzeigen"),
        ContributionLabel::Year => text.literal("Jahr"),
        ContributionLabel::View => text.literal("Anzeigen"),
        ContributionLabel::AnnualCalendar => text.literal("Commit-Kalender des Jahres"),
        ContributionLabel::QuarterlyCalendar => text.literal("Commit-Kalender nach Quartal"),
        ContributionLabel::Monday => text.literal("Mo"),
        ContributionLabel::Wednesday => text.literal("Mi"),
        ContributionLabel::Friday => text.literal("Fr"),
        ContributionLabel::DatesUseUtc => text.literal("Datumsangaben in UTC."),
        ContributionLabel::IntensityLegend => text.literal("Commit-Intensität von wenig bis viel"),
        ContributionLabel::Less => text.literal("Weniger"),
        ContributionLabel::More => text.literal("Mehr"),
        ContributionLabel::Partial => text.literal(
            "Unvollständige Aktivität: Mindestens ein Repository hat das Limit von 100.000 Commits für die \
             Verlaufsauswertung überschritten.",
        ),
        ContributionLabel::NoCommits => text.literal("Keine passenden Commits in diesem Jahr."),
        ContributionLabel::SelectDate => {
            text.literal("Wählen Sie ein Datum aus, um dessen Commit-Aktivität zu sehen.");
        }
        ContributionLabel::Commits => text.literal("Commits"),
        ContributionLabel::ActiveDays => text.literal("Aktive Tage"),
        ContributionLabel::LongestStreak => text.literal("Längste Serie"),
        ContributionLabel::Repositories => text.literal("Repositorys"),
        ContributionLabel::HowCounted => text.literal("Monatliche Aktivität und Zählweise der Commits"),
        ContributionLabel::HowCountedText => text.literal(
            "Gezählt werden Commits, die vom konfigurierten Standardbranch aus erreichbar sind, nach ihrem \
             Autordatum. Identische Commits in mehreren Repositorys zählen einmal. Nur Repositorys, auf die Sie \
             Zugriff haben, werden berücksichtigt. Issues, Pull-Requests und Reviews zählen nicht mit.",
        ),
    }
    ContributionMessage<'a> => MessageWriter |text| {
        ContributionMessage::CommitsInYear { count, year } => {
            commits(text, count);
            text.literal(" im Jahr ");
            text.argument(&year.to_string());
        }
        ContributionMessage::ActivityInYear { year } => {
            text.literal("Commit-Aktivität im Jahr ");
            text.argument(&year.to_string());
        }
        ContributionMessage::Description { count, days } => {
            commits(text, count);
            text.literal(" an ");
            text.number(days as u64);
            text.literal(match text.plural(days as u64) {
                Plural::One => " aktiven Tag",
                _ => " aktiven Tagen",
            });
            text.literal(
                ". Höhere Intensität bedeutet mehr Commits. Fokussieren oder zeigen Sie auf einen Tag, um seine \
                 Anzahl zu sehen. Wählen Sie einen Tag für Details aus.",
            );
        }
        ContributionMessage::Day { count, date } => {
            commits(text, count);
            text.literal(" am ");
            text.argument(date);
        }
        ContributionMessage::QuarterActivity { quarter, year } => {
            text.literal("Commit-Aktivität ");
            text.argument(quarter);
            text.literal(" ");
            text.argument(&year.to_string());
        }
        ContributionMessage::Days { count } => {
            text.number(count as u64);
            text.literal(match text.plural(count as u64) {
                Plural::One => " Tag",
                _ => " Tage",
            });
        }
    }
}

/// „1 Commit“, „3 Commits“.
fn commits(text: &mut MessageWriter, count: usize) {
    text.number(count as u64);
    text.literal(match text.plural(count as u64) {
        Plural::One => " Commit",
        _ => " Commits",
    });
}
