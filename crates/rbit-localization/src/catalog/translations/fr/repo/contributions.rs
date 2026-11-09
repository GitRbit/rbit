//! French (`fr`) entries for `catalog/repo/contributions.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::contributions::{ContributionLabel, ContributionMessage};
use crate::plural::Plural;

entries! { Fr:
    ContributionLabel => MessageWriter |text| {
        ContributionLabel::CommitActivity => text.literal("Activité des commits"),
        ContributionLabel::Loading => text.literal("Chargement de l’activité des commits…"),
        ContributionLabel::ViewCommitActivity => text.literal("Afficher l’activité des commits"),
        ContributionLabel::Year => text.literal("Année"),
        ContributionLabel::View => text.literal("Afficher"),
        ContributionLabel::AnnualCalendar => text.literal("Calendrier annuel des commits"),
        ContributionLabel::QuarterlyCalendar => text.literal("Calendrier des commits par trimestre"),
        ContributionLabel::Monday => text.literal("lun."),
        ContributionLabel::Wednesday => text.literal("mer."),
        ContributionLabel::Friday => text.literal("ven."),
        ContributionLabel::DatesUseUtc => text.literal("Les dates sont en UTC."),
        ContributionLabel::IntensityLegend => text.literal("Intensité des commits, de faible à forte"),
        ContributionLabel::Less => text.literal("Moins"),
        ContributionLabel::More => text.literal("Plus"),
        ContributionLabel::Partial => text.literal(
            "Activité partielle\u{a0}: au moins un dépôt a dépassé la limite d’analyse de l’historique de 100\u{202f}000\u{a0}commits.",
        ),
        ContributionLabel::NoCommits => text.literal("Aucun commit correspondant cette année."),
        ContributionLabel::SelectDate => text.literal("Sélectionnez une date pour voir son activité de commits."),
        ContributionLabel::Commits => text.literal("Commits"),
        ContributionLabel::ActiveDays => text.literal("Jours actifs"),
        ContributionLabel::LongestStreak => text.literal("Plus longue série"),
        ContributionLabel::Repositories => text.literal("Dépôts"),
        ContributionLabel::HowCounted => text.literal("Activité mensuelle et mode de comptage des commits"),
        ContributionLabel::HowCountedText => text.literal(
            "Le décompte inclut les commits accessibles depuis la branche par défaut configurée, attribués à leur date \
             d’auteur. Les commits identiques partagés entre plusieurs dépôts ne comptent qu’une fois. Seuls les dépôts \
             auxquels vous avez accès sont inclus. Les tickets, les pull requests et les revues ne sont pas comptés.",
        ),
    }
    ContributionMessage<'a> => MessageWriter |text| {
        ContributionMessage::CommitsInYear { count, year } => {
            commits(text, count);
            text.literal(" en ");
            text.argument(&year.to_string());
        }
        ContributionMessage::ActivityInYear { year } => {
            text.literal("Activité des commits en ");
            text.argument(&year.to_string());
        }
        ContributionMessage::Description { count, days } => {
            commits(text, count);
            text.literal(" sur ");
            text.number(days as u64);
            text.literal(match text.plural(days as u64) {
                Plural::One => " jour actif",
                _ => " jours actifs",
            });
            text.literal(
                ". Une intensité plus forte indique davantage de commits. Placez le focus ou survolez un jour pour voir \
                 son nombre de commits. Sélectionnez un jour pour en voir le détail.",
            );
        }
        ContributionMessage::Day { count, date } => {
            commits(text, count);
            text.literal(" le ");
            text.argument(date);
        }
        ContributionMessage::QuarterActivity { quarter, year } => {
            text.literal("Activité des commits du ");
            text.argument(quarter);
            text.literal(" ");
            text.argument(&year.to_string());
        }
        ContributionMessage::Days { count } => {
            text.number(count as u64);
            text.literal(match text.plural(count as u64) {
                Plural::One => " jour",
                _ => " jours",
            });
        }
    }
}

/// « 1 commit », « 3 commits ».
fn commits(text: &mut MessageWriter, count: usize) {
    text.number(count as u64);
    text.literal(match text.plural(count as u64) {
        Plural::One => " commit",
        _ => " commits",
    });
}
